**You:**

使用embedding分块时，图片表格等如何处理，如何做到在chunk预览时可以看到表格图片等

* * *

**ChatGPT:**

这里要把“用于检索的 chunk”和“用于预览的 chunk”分开设计。Embedding 不一定直接吃图片/表格本体，但预览必须能回到原始文档结构。

可以做到，但关键点是：

不要把 chunk 理解成纯文本。
SeekMind 里应该区分两种数据：
检索 chunk 和 预览 block。

Embedding 阶段主要处理文本语义；预览阶段需要保留文档结构，包括标题、段落、表格、图片、页码、位置等。

. 核心设计思路

你现在不能简单这样做：

文档 → 提取纯文本 → 分段 → embedding → 搜索 → 显示纯文本

这样会导致两个问题：

表格被压平成一堆文本，预览效果差。
图片丢失，用户搜到相关内容后看不到原始上下文。

正确做法应该是：

文档
  ↓
解析为结构化 Blocks
  ↓
Blocks 生成用于搜索的文本 chunk
  ↓
chunk 记录关联的 block 范围
  ↓
搜索命中 chunk
  ↓
根据 block 范围还原预览

也就是说：

Embedding 用的是文本化后的 SearchChunk
预览展示的是结构化的 PreviewBlocks
2. 文档内部模型不要只存 text

建议把解析结果设计成类似这样：

Document
  ├─ Block 1: heading
  ├─ Block 2: paragraph
  ├─ Block 3: table
  ├─ Block 4: image
  ├─ Block 5: paragraph
  └─ Block 6: code

每个 Block 都有自己的类型。

Rust/Python 返回结构可以是：
```json
{
  "document_id": "doc_001",
  "title": "Maven 离线仓库方案",
  "blocks": [
    {
      "id": "b1",
      "type": "heading",
      "level": 2,
      "text": "三、离线依赖导出方案",
      "page_no": 4,
      "order": 1
    },
    {
      "id": "b2",
      "type": "paragraph",
      "text": "通过 dependency:go-offline 拉取 parent POM、BOM、plugin 依赖...",
      "page_no": 4,
      "order": 2
    },
    {
      "id": "b3",
      "type": "table",
      "text": "依赖类型 | 说明\nparent POM | 父工程\nBOM | dependencyManagement",
      "html": "<table>...</table>",
      "markdown": "| 依赖类型 | 说明 |\\n|---|---|",
      "page_no": 4,
      "order": 3
    },
    {
      "id": "b4",
      "type": "image",
      "alt_text": "Maven 离线仓库目录结构截图",
      "ocr_text": null,
      "image_path": "assets/doc_001/b4.png",
      "page_no": 5,
      "order": 4
    }
  ]
}
```


这样你的搜索和预览都能做得很好。

3. 数据库应该怎么设计

不要只存 chunks 表。建议加一张 blocks 表。

documents
CREATE TABLE documents (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    file_name TEXT NOT NULL,
    file_ext TEXT,
    file_size INTEGER,
    modified_at INTEGER,
    content_hash TEXT,
    indexed_at INTEGER,
    status TEXT NOT NULL,
    error_message TEXT
);
document_blocks
CREATE TABLE document_blocks (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    block_type TEXT NOT NULL,
    block_index INTEGER NOT NULL,

    page_no INTEGER,
    heading_level INTEGER,
    heading_path TEXT,

    text TEXT,
    markdown TEXT,
    html TEXT,

    asset_path TEXT,
    asset_mime TEXT,

    start_offset INTEGER,
    end_offset INTEGER,

    metadata_json TEXT,

    FOREIGN KEY(document_id) REFERENCES documents(id)
);

其中：

block_type 可以是：
heading
paragraph
table
image
code
list
quote
page_break
search_chunks

再单独设计搜索 chunk：

CREATE TABLE search_chunks (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,

    chunk_index INTEGER NOT NULL,
    text_for_search TEXT NOT NULL,

    start_block_id TEXT NOT NULL,
    end_block_id TEXT NOT NULL,

    block_ids_json TEXT NOT NULL,

    page_no INTEGER,
    heading_path TEXT,

    token_count INTEGER,
    embedding_status TEXT,

    FOREIGN KEY(document_id) REFERENCES documents(id)
);

这里的重点是：

search_chunks 不直接承担预览
search_chunks 只负责检索
预览时通过 block_ids_json 找回原始 blocks
4. Embedding 时表格怎么处理？

表格不能直接扔掉，也不能只保存 HTML。Embedding 需要一份“文本化表达”。

例如原始表格：

类型	说明
parent POM	父工程依赖
BOM	dependencyManagement 依赖
plugin	Maven 插件依赖

可以转成搜索文本：

表格：Maven 离线依赖类型说明。
列：类型，说明。
第 1 行：parent POM，父工程依赖。
第 2 行：BOM，dependencyManagement 依赖。
第 3 行：plugin，Maven 插件依赖。

这份文本用于：

全文索引
embedding
语义搜索

但预览时不要展示这段纯文本，而是展示原始表格。

也就是说，表格 block 里同时保存：

{
  "type": "table",
  "text": "表格：Maven 离线依赖类型说明。列：类型，说明。第1行...",
  "markdown": "| 类型 | 说明 |\\n|---|---|...",
  "html": "<table>...</table>"
}

搜索用 text，预览用 html 或 markdown。

5. Embedding 时图片怎么处理？

图片分三种情况。

情况一：图片只是装饰图

例如 logo、背景图、分割线。

处理方式：

不参与 embedding
预览时保留
情况二：图片有 alt、caption 或附近文字

例如：

图 3：Maven 离线仓库目录结构

这时候可以生成图片的搜索文本：

图片：Maven 离线仓库目录结构。说明：展示 .offline-repo 下的 com、org、net 目录。

这部分可以参与 embedding。

图片 block：

{
  "type": "image",
  "asset_path": "assets/doc_001/img_003.png",
  "caption": "图 3：Maven 离线仓库目录结构",
  "text": "图片：Maven 离线仓库目录结构",
  "page_no": 5
}
情况三：图片里有重要文字

例如截图、扫描件、架构图。

第一阶段可以先不做 OCR，只保留图片。

第二阶段加入 OCR 后，图片 block 可以变成：

{
  "type": "image",
  "asset_path": "assets/doc_001/img_004.png",
  "caption": "网关鉴权流程图",
  "ocr_text": "X-App-Key X-Timestamp X-Nonce X-Signature Authorization Filter",
  "text": "图片：网关鉴权流程图。OCR 内容：X-App-Key X-Timestamp X-Nonce X-Signature..."
}

Embedding 用：

caption + ocr_text + surrounding_text

预览展示：

图片本体 + caption + OCR 摘要
6. Chunk 和 Block 的关系

一个搜索 chunk 可以包含多个 block。

例如文档结构：

Block 10: 标题：三、离线依赖导出方案
Block 11: 段落：通过 dependency:go-offline...
Block 12: 表格：依赖类型说明
Block 13: 图片：目录结构截图
Block 14: 段落：生成后的 .offline-repo...

生成搜索 chunk：

{
  "id": "chunk_003",
  "text_for_search": "三、离线依赖导出方案。通过 dependency:go-offline... 表格：依赖类型说明... 图片：目录结构截图... 生成后的 .offline-repo...",
  "block_ids": ["b10", "b11", "b12", "b13", "b14"],
  "start_block_id": "b10",
  "end_block_id": "b14",
  "page_no": 4
}

搜索命中 chunk_003 后：

1. 从 search_chunks 拿到 block_ids
2. 查询 document_blocks
3. 按 block_index 排序
4. 渲染成预览

这样 chunk 预览就可以看到：

标题
段落
表格
图片
段落
7. 前端预览怎么渲染？

前端不要只接收 snippet，还要有 preview_blocks。

接口返回可以这样：

{
  "document_id": "doc_001",
  "chunk_id": "chunk_003",
  "file_name": "Maven离线仓库方案.docx",
  "file_path": "/Users/zhaoyang/Documents/maven.docx",
  "score": 0.89,
  "snippet": "通过 dependency:go-offline 拉取 parent POM、BOM、plugin 依赖...",
  "preview_blocks": [
    {
      "type": "heading",
      "level": 2,
      "text": "三、离线依赖导出方案"
    },
    {
      "type": "paragraph",
      "text": "通过 dependency:go-offline 拉取 parent POM、BOM、plugin 依赖..."
    },
    {
      "type": "table",
      "html": "<table>...</table>"
    },
    {
      "type": "image",
      "asset_url": "SeekMind://asset/doc_001/img_003.png",
      "caption": "图 3：离线仓库目录结构"
    }
  ]
}

Vue 中可以设计一个 PreviewBlockRenderer：

<template>
  <div class=\"preview-blocks\">
    <component
      v-for=\"block in blocks\"
      :key=\"block.id\"
      :is=\"resolveBlockComponent(block.type)\"
      :block=\"block\"
    />
  </div>
</template>

组件：

ParagraphBlock.vue
HeadingBlock.vue
TableBlock.vue
ImageBlock.vue
CodeBlock.vue
ListBlock.vue
QuoteBlock.vue
8. 图片文件存哪里？

建议放在 SeekMind 的应用数据目录下。

例如：

~/Library/Application Support/SeekMind/assets/
  └─ doc_001/
      ├─ img_001.png
      ├─ img_002.jpg
      └─ table_001.html

数据库里只保存相对路径：

assets/doc_001/img_001.png

前端通过 Tauri command 获取：

#[tauri::command]
async fn get_asset_url(document_id: String, asset_path: String) -> Result<String, String>

或者更简单：

#[tauri::command]
async fn read_asset_base64(asset_path: String) -> Result<String, String>

MVP 可以先用 base64 返回。

但正式版建议不要大量 base64，因为会占内存。可以用 Tauri 的 asset protocol 或本地文件 URL 方式。

9. PDF 的表格和图片怎么处理？

PDF 最麻烦，因为 PDF 本质上不是结构化文档。

第一阶段建议这样处理：

PDF：
  - 提取每页文本
  - 每页生成 blocks
  - 图片先不抽取
  - 表格先不还原，只作为文本

也就是第一阶段 PDF 预览只保证：

页码
段落文本
上下文

不要承诺 PDF 中的图片和复杂表格还原。

第二阶段再升级：

PDF:
  - PyMuPDF 提取图片
  - Camelot / pdfplumber 提取表格
  - 保存 page_no 和 bbox
  - 预览时展示图片、表格

对于 PDF 图片，最好保存：

{
  "type": "image",
  "page_no": 3,
  "bbox": [72, 120, 480, 360],
  "asset_path": "assets/doc_001/page_3_img_1.png"
}
10. DOCX 的表格和图片怎么处理？

DOCX 比 PDF 好处理很多，因为它本身是结构化 XML。

DOCX 解析建议：

段落 → paragraph block
标题 → heading block
表格 → table block
图片 → image block

图片实际在 docx zip 包里：

word/media/image1.png
word/media/image2.jpeg

你可以在 Python sidecar 里：

1. 解压 docx
2. 读取 word/document.xml
3. 找到段落、表格、图片引用
4. 把图片复制到 assets/doc_id/
5. block 中记录 asset_path

第一阶段如果时间有限，DOCX 可以先做到：

段落 + 表格文本

第二阶段再支持：

图片提取 + 原始表格 HTML 渲染
11. HTML / Markdown 最适合做结构化预览

HTML 和 Markdown 很好处理。

Markdown 可以转成 blocks：

# 标题 → heading
正文 → paragraph
表格 → table
图片 → image
代码块 → code
引用 → quote
列表 → list

Markdown 图片：

![架构图](./images/arch.png)

block：

{
  "type": "image",
  "asset_path": "/原始相对路径/images/arch.png",
  "caption": "架构图"
}

HTML 可以保留安全清洗后的局部 HTML：

{
  "type": "table",
  "html": "<table>...</table>"
}
12. Embedding 文本应该如何生成？

每种 block 生成一份 search_text。

paragraph
原文
heading
标题：xxx
table
表格：xxx。列：A、B、C。第1行：...
image
图片：caption。OCR：xxx。附近上下文：xxx
code
代码片段：语言 Java。内容：...

然后 chunker 拼接：

heading_path + paragraph + table_text + image_text + surrounding_context

例如：

三、离线依赖导出方案

通过 dependency:go-offline 拉取 parent POM、BOM、plugin 依赖。

表格：依赖类型说明。
列：依赖类型，处理方式。
第 1 行：parent POM，递归下载父 POM。
第 2 行：BOM，解析 import dependencyManagement。
第 3 行：plugin，解析 build plugins。

图片：离线仓库目录结构截图。

这段用于 embedding。

13. 搜索命中图片怎么办？

如果搜索命中的是图片 OCR 或 caption，结果应该显示：

文件名
页码
命中类型：图片
图片缩略图
caption / OCR 摘要

例如：

网关鉴权设计.pdf
第 3 页 · 图片

命中：X-App-Key、X-Timestamp、X-Signature
[图片缩略图]

这会比只显示 OCR 文本好很多。

14. 搜索命中表格怎么办？

如果命中的是表格，结果应该显示：

文件名
页码 / 标题
命中类型：表格
表格预览

例如：

API平台设计.docx
三、认证参数

| 参数 | 说明 |
|---|---|
| X-App-Key | 应用标识 |
| X-Timestamp | 时间戳 |
| X-Signature | 签名 |

这就是为什么要保存 html/markdown，而不是只保存表格文本。

15. MVP 阶段怎么取舍？

我建议 SeekMind MVP 分成两层。

MVP 0.1：先实现结构化文本预览

支持：

段落
标题
代码块
Markdown 表格
HTML 表格
DOCX 表格转 Markdown
PDF 页码文本

暂不支持：

PDF 图片抽取
PDF 复杂表格还原
扫描版 OCR
图片语义理解
MVP 0.2：增强图片和表格

支持：

DOCX 图片提取
Markdown 图片预览
HTML 图片预览
PDF 图片提取
PDF 表格提取
图片 OCR
16. 推荐最终架构
Python Parser Sidecar
  ↓
输出 DocumentBlocks
  ↓
Rust 保存 blocks
  ↓
Rust 根据 blocks 生成 SearchChunks
  ↓
Tantivy / Vector DB 索引 SearchChunks
  ↓
搜索命中 SearchChunk
  ↓
根据 block_ids 回查 DocumentBlocks
  ↓
前端渲染结构化预览

一句话：

Embedding 不负责保存原文结构，Block 模型才负责。
搜索命中 chunk，预览回查 blocks。

17. 你可以这样改 SeekMind 的 MVP 数据流

原来的：

Parser → ParsedDocument → chunks → index

建议改成：

Parser → DocumentBlocks → SearchChunks → index

其中：

DocumentBlocks：保留原始结构，用于预览
SearchChunks：由 blocks 生成，用于检索

这样后面支持图片、表格、代码块、公式、OCR 都不会推翻架构。

18. 最关键的设计原则

最终你要记住这 4 条：

1. 文档解析结果必须结构化，不要只提取纯文本。
2. 表格要同时保存 markdown/html 和 text_for_search。
3. 图片要保存 asset_path，并尽量生成 caption/ocr_text/surrounding_text。
4. 搜索 chunk 要记录 block_ids，预览时通过 block_ids 还原原始结构。

这样 SeekMind 才能做到：

搜得到
看得懂
能定位
能预览
后续还能升级成语义搜索和本地问答