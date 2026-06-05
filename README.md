# DocMind

DocMind 是一个基于 Tauri + Vue 3 + Rust 的本地文档检索工具。它会扫描用户选择的目录，把文档内容切块后建立本地全文索引，之后可以直接按关键词定位到文档和具体段落。

## 现在支持的功能

- 本地目录管理
- 添加、删除、启用、禁用索引目录
- 目录级重新索引
- 全量重新索引
- 失败文件单独重试
- 文档内容搜索
- 段落级结果展示
- 打开原文件
- 本地持久化存储
- SQLite 元数据管理
- Tantivy 全文索引

## 当前支持的文档格式

目前第一阶段支持这些格式：

- `.txt`
- `.md`
- `.markdown`
- `.html`
- `.htm`
- `.docx`
- `.log`
- `.toml`
- `.json`
- `.yaml`
- `.yml`
- `.xml`
- `.csv`
- `.rs`
- `.js`
- `.ts`
- `.tsx`
- `.jsx`
- `.py`

说明：

- `.pdf` 目前还未接入稳定解析
- 扫描版 PDF、旧版 `.doc`、图片 OCR 仍属于后续阶段

## 启动方式

### 1. 安装依赖

```bash
npm install
```

### 2. 启动前端开发服务

```bash
npm run dev
```

如果你想直接在浏览器里调试前端页面，可以用：

```bash
npm run dev:browser
```

### 3. 启动 Tauri 桌面应用

```bash
npm run tauri dev
```

开发环境默认会启用 Python 解析器 sidecar：

- `DOCMIND_USE_PY_PARSER=1`
- Markdown / DOCX / HTML / TXT 会优先走 Python 解析
- Rust 侧仍保留 fallback，便于调试和对照

Python 侧任务调用约定和流式进度模式见：

- [`docs/20-Python侧任务调用经验规范(2026-05-24).md`](/Users/zhaoyang/Desktop/enjoy/docmind/docs/20-Python侧任务调用经验规范(2026-05-24).md)

RAG 问答现在也支持 Python sidecar 侧的回归评测命令 `rag_eval`，用于在本地直接跑一组固定样本，检查 `answered` 和 `insufficient_evidence` 这两类终态是否符合预期。你可以直接给 Python sidecar 发送一段 JSON 请求，命令示例如下：

```json
{
  "request_id": "rag-eval-001",
  "command": "rag_eval",
  "db_path": "/path/to/docmind.sqlite",
  "scope_paths": [],
  "session_context": "",
  "recent_questions": [],
  "settings": {
    "provider": "openai_compatible",
    "base_url": "http://127.0.0.1:11434/v1",
    "api_key": "",
    "model": "deepseek-r1:latest",
    "temperature": 0.2,
    "max_output_tokens": 6000,
    "context_chunk_limit": 8,
    "context_token_budget": 6000,
    "min_evidence_count": 1,
    "min_retrieval_score": 0.0
  }
}
```

默认评测样本清单见：

- [`docs/35-RAG回归评测集(2026-06-05).md`](/Users/zhaoyang/Desktop/enjoy/docmind/docs/35-RAG%E5%9B%9E%E5%BD%92%E8%AF%84%E6%B5%8B%E9%9B%86(2026-06-05).md)

语义搜索的 embedding 也走 Python sidecar，依赖 `parser/requirements.txt` 里的 `fastembed`。首次启用前建议执行：

```bash
python3 -m pip install -r parser/requirements.txt
```

首次使用 `BAAI/bge-small-zh-v1.5` 时需要下载本地 ONNX 模型。建议先执行一次预热命令，确认模型已经缓存成功：

```bash
npm run semantic:warmup
```

如果模型下载超时，可以尝试 Hugging Face 镜像：

```bash
npm run semantic:warmup:mirror
```

开发环境会把 FastEmbed 模型缓存固定到：

```text
.docmind-cache/fastembed
```

如果之前下载中断过，直接重新执行预热命令即可，不会继续使用系统临时目录里的半截缓存。

如果你想看索引流程日志，可以用：

```bash
npm run tauri:dev:trace
```

### 5. 打包 macOS App

DocMind 可以直接打包成 macOS 应用。当前开发阶段如果还没有 Apple 开发者签名，可以先用 ad-hoc 签名：

```bash
npm run tauri:build:macos:adhoc
```

这条命令等价于：

```bash
APPLE_SIGNING_IDENTITY=- tauri build
```

构建产物会输出到：

```text
src-tauri/target/release/bundle/macos/
```

通常会包含：

- `.app`
- `.dmg`

说明：

- `APPLE_SIGNING_IDENTITY=-` 是 Tauri 支持的 ad-hoc 签名方式
- 如果是 Apple Silicon 机器，macOS 对未签名应用的安装限制会更明显，ad-hoc 仍可能需要在“隐私与安全性”里手动放行
- 后续如果接入正式 Apple 开发者证书，再把这个环境变量替换成真实 signing identity 即可

如果你要打包“完整 app”，也就是把 Python 解析器一起冻结成 sidecar 再打包，可以用：

```bash
npm run tauri:build:macos:sidecar
```

这条命令会先：

- 用 PyInstaller 把 `parser/docmind_parser/__main__.py` 冻结成独立可执行文件
- 放进 `src-tauri/app-resources/`
- 再执行 Tauri macOS 构建

因此产物里的 app 会优先使用冻结后的 parser sidecar；如果没有侧边可执行文件，运行时仍会回落到开发态的 Python 脚本链路。

### 6. 如果要清空并重新启动开发环境

```bash
npm run tauri:dev:fresh
```

这个命令等价于：

```bash
npm run tauri dev -- --reset-local-storage
```

含义是：

- 启动前先清空本地 SQLite 数据库和 Tantivy 索引
- 跳过启动时的自动重建
- 适合调试索引初始化、目录种子数据和首次扫描流程

补充说明：

- `--reset-local-storage` 是传给应用本身的启动参数
- 这个参数只影响本地开发数据，不会删除你选择的原始文档目录
- 正常启动仍然使用 `npm run tauri dev`

如果你还想同时打开索引日志，可以用：

```bash
npm run tauri:dev:fresh:trace
```

## 构建

```bash
npm run build
```

## 数据存储位置

DocMind 会把本地数据保存在系统用户数据目录下：

- SQLite 数据库：`DocMind/docmind.sqlite`
- Tantivy 索引：`DocMind/tantivy`

## 技术栈

- 前端：Vue 3 + Vite + TypeScript
- 桌面端：Tauri v2
- 后端：Rust
- 元数据：SQLite + `sqlx`
- 全文检索：Tantivy

## 项目目标

DocMind 的目标是解决“我记得内容，但不记得文件名”的本地文档检索问题。第一阶段先把全文检索、段落定位、目录管理和本地索引稳定做好，后续再考虑语义搜索、OCR 和更复杂的问答能力。

## 开发说明

- 当前项目主要在 macOS 上开发和验证
- 目录扫描和索引逻辑都在 Rust 侧
- 前端通过 Tauri `invoke` 调用后端命令
- 你可以直接修改 Vue 页面或 Rust 模块，而不需要改动整个启动链路

## 许可证

当前未单独声明许可证。
