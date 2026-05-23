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

如果你想看索引流程日志，可以用：

```bash
npm run tauri:dev:trace
```

### 4. 如果要清空并重新启动开发环境

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
