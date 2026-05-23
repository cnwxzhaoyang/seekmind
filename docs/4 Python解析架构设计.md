# DocMind Python 解析架构设计

本文定义 DocMind 的 Python 解析 Sidecar 协议、数据结构、错误码和超时机制。目标是把文档解析和切片能力从 Rust 主进程中拆出去，先由 Python 负责 `md/docx/html/txt` 的解析与 chunk 生成，Rust 继续负责扫描、SQLite、Tantivy、搜索和 UI。

## 1. 架构目标

- Rust 保持主控：目录管理、任务调度、索引写入、搜索、状态展示。
- Python 负责解析：把原始文档转换成结构化文本和切片。
- 解析进程与主进程隔离：Python 崩溃、超时、依赖异常不影响 Tauri 主进程稳定性。
- 协议尽量简单：以 JSON 请求/响应为主，便于调试、记录和回放。

## 2. 运行方式

DocMind 默认按“单次调用、单次返回”的方式使用 Python 解析器：

1. Rust 根据文件路径构造 JSON 请求。
2. Rust 启动 Python sidecar。
3. Rust 将请求写入 Python 的 `stdin`。
4. Python 读取请求、解析文档、输出 JSON 到 `stdout`。
5. Rust 读取结果，校验 `request_id` 和 `ok` 字段。
6. Rust 将解析后的文档和切片写入 SQLite，并同步 Tantivy 索引。

## 3. 通信协议

### 3.1 请求

Rust -> Python 的请求格式：

```json
{
  "request_id": "b7e7dd9b-7b6d-4c1f-9a5f-5d2f2ff0a1f7",
  "command": "parse_document",
  "path": "/Users/zhaoyang/Documents/example.md",
  "options": {
    "include_chunks": true,
    "max_chunk_chars": 800,
    "max_chunks": null
  }
}
```

字段说明：

- `request_id`
  - 请求唯一标识，用于请求/响应匹配和日志串联。
- `command`
  - 当前固定为 `parse_document`。
- `path`
  - 待解析文件的绝对路径。
- `options`
  - 解析参数。
  - `include_chunks`：是否返回切片。
  - `max_chunk_chars`：单个 chunk 的建议最大字符数。
  - `max_chunks`：最大 chunk 数量，`null` 表示不限制。

### 3.2 响应

Python -> Rust 的响应格式：

```json
{
  "request_id": "b7e7dd9b-7b6d-4c1f-9a5f-5d2f2ff0a1f7",
  "ok": true,
  "document": {
    "title": "example",
    "file_type": "md",
    "content": "第一段正文...\n\n第二段正文...",
    "chunks": [
      {
        "heading": "标题一",
        "page_no": null,
        "text": "第一段正文...",
        "order": 1
      }
    ]
  },
  "error": null
}
```

字段说明：

- `request_id`
  - 必须与请求一致。
- `ok`
  - `true` 表示成功，`false` 表示失败。
- `document`
  - 成功时返回。
  - 失败时为 `null`。
- `error`
  - 失败时返回。
  - 成功时为 `null`。

## 4. 文档结构

### 4.1 ParsedDocument

```json
{
  "title": "文档标题",
  "file_type": "docx",
  "content": "完整可索引正文",
  "chunks": []
}
```

- `title`
  - 文档标题，若无法识别可为空。
- `file_type`
  - 文件类型，优先使用扩展名。
- `content`
  - 适合全文索引的纯文本内容。
- `chunks`
  - 文档切片列表。

### 4.2 ParsedChunk

```json
{
  "heading": "一、背景",
  "page_no": null,
  "text": "这一段是切片内容",
  "order": 1
}
```

- `heading`
  - 当前切片所属标题或上下文。
- `page_no`
  - 页码信息，`md/txt/html/docx` 通常为 `null`，保留给后续 PDF。
- `text`
  - 切片正文。
- `order`
  - 切片顺序，从 `1` 开始。

## 5. 错误码

Python 解析器通过 `error.code` 返回错误类型。建议统一使用以下错误码：

| code | 含义 | 处理建议 |
| --- | --- | --- |
| `INVALID_REQUEST` | 请求格式不合法，缺少字段或类型错误 | 终止本次任务，记录到失败项 |
| `FILE_NOT_FOUND` | 文件不存在或已被删除 | 终止本次任务，记录到失败项 |
| `UNSUPPORTED_FILE_TYPE` | 文件类型不在当前支持范围 | 终止本次任务，记录到失败项 |
| `PARSE_FAILED` | 文档解析失败 | 记录到失败项，可重试 |
| `DEPENDENCY_MISSING` | Python 解析依赖缺失 | 记录到失败项，提示安装依赖 |
| `INTERNAL_ERROR` | 未分类内部错误 | 记录到失败项，输出详细日志 |

说明：

- 当前阶段支持 `md/docx/html/txt`。
- `pdf` 暂不纳入 Python 侧首版支持范围，后续再单独接入。
- Rust 侧遇到 Python 错误时，会将其转换成可读的 `String` 写入失败项。

## 6. 超时机制

### 6.1 Rust 侧超时

Rust 负责强制超时。

- 环境变量：`DOCMIND_PARSER_TIMEOUT_MS`
- 默认值：`30000`
- 语义：单次文件解析允许的最长时间
- 超时后：
  - Rust kill 掉 Python 子进程
  - 返回 `Timeout` 错误
  - 记录到索引失败项

### 6.2 Python 侧约束

Python 侧不负责总超时控制，只负责尽快返回：

- 不要在单次解析里做不必要的全量阻塞 I/O。
- 遇到异常要返回结构化错误，不要静默退出。
- 解析实现应保持幂等，便于失败重试。

## 7. 环境变量

### 7.1 Rust 侧

- `DOCMIND_USE_PY_PARSER`
  - `1` 表示启用 Python 解析器
  - 未设置或 `0` 表示继续使用 Rust fallback
- `DOCMIND_PARSER_BIN`
  - Python 可执行文件，默认 `python3`
- `DOCMIND_PARSER_SCRIPT`
  - 入口脚本路径，默认 `parser/docmind_parser/__main__.py`
- `DOCMIND_PARSER_TIMEOUT_MS`
  - 单次解析超时时间，默认 `30000`
- `DOCMIND_TRACE_INDEXER`
  - 输出索引任务日志到终端，便于排查

### 7.2 Python 侧

Python 侧当前不强制依赖额外环境变量。后续若引入第三方库，可通过 `requirements.txt` 或 `pyproject.toml` 管理。

## 8. 支持范围

首版 Python 解析器只处理以下文件类型：

- `md`
- `docx`
- `html`
- `txt`

切块原则：

- 尽量按标题、段落、空行、列表项等语义边界切分。
- 避免把整篇文档压成单个 chunk。
- chunk 数量比“完全精细”更重要，先保证召回可用。

## 9. 非目标

以下能力不在首版 Python 解析器范围内：

- PDF 解析
- OCR
- 复杂表格还原
- 布局坐标级定位
- 远程解析服务

## 10. 后续演进

未来如果解析复杂度继续上升，可以继续沿着下面的方向扩展：

1. Python 增强解析库，引入 `python-docx`、`beautifulsoup4`、`PyMuPDF`、`unstructured`、`docling` 等。
2. 统一 chunk schema，给每种文件类型保留更多来源信息。
3. 引入批量解析协议，减少 sidecar 启动开销。
4. 再考虑 PDF 和 OCR。

