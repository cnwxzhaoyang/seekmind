import React, { useMemo, useState } from "react";
import { Search, FolderPlus, Settings, FileText, File, AlertCircle, CheckCircle2, Loader2, ExternalLink, Eye, FolderOpen, RefreshCw, X, Clock, Database, ChevronRight, Filter } from "lucide-react";
import { motion } from "framer-motion";

const mockDirs = [
  { path: "/Users/zhaoyang/Documents", enabled: true, docs: 842, chunks: 13280, status: "indexed" },
  { path: "/Users/zhaoyang/Downloads", enabled: true, docs: 196, chunks: 2844, status: "indexing" },
];

const mockResults = [
  {
    id: "1",
    fileName: "Maven离线仓库方案.md",
    path: "/Users/zhaoyang/Documents/dev/maven-offline.md",
    ext: "md",
    heading: "三、离线依赖导出方案",
    snippet: "通过 dependency:go-offline 拉取 parent POM、BOM、plugin 依赖，并生成 .offline-repo 目录，用于内网环境构建。",
    paragraph: 5,
    modified: "今天 09:42",
    score: 0.92,
  },
  {
    id: "2",
    fileName: "SpringBoot内网构建记录.docx",
    path: "/Users/zhaoyang/Documents/work/springboot-offline.docx",
    ext: "docx",
    heading: "BOM 与插件依赖处理",
    snippet: "对于 spring-boot-dependencies、spring-cloud-dependencies 等 import BOM，需要递归解析 dependencyManagement 并下载对应 POM。",
    paragraph: 12,
    modified: "昨天 18:20",
    score: 0.86,
  },
  {
    id: "3",
    fileName: "Jenkins离线部署.pdf",
    path: "/Users/zhaoyang/Documents/ops/jenkins-offline.pdf",
    ext: "pdf",
    heading: "插件目录与启动参数",
    snippet: "插件可以放在 JENKINS_HOME/plugins 目录下，离线环境需要同时准备插件本体以及 transitive dependencies。",
    page: 4,
    modified: "2026-04-10",
    score: 0.78,
  },
];

const failedFiles = [
  { file: "扫描版合同.pdf", reason: "暂不支持 OCR，无法提取文本" },
  { file: "old-report.doc", reason: "暂不支持旧版 .doc 格式" },
];

function Badge({ children, type = "default" }) {
  const cls = type === "success" ? "bg-emerald-50 text-emerald-700 border-emerald-100" : type === "warning" ? "bg-amber-50 text-amber-700 border-amber-100" : type === "danger" ? "bg-red-50 text-red-700 border-red-100" : "bg-slate-50 text-slate-600 border-slate-100";
  return <span className={`inline-flex items-center rounded-full border px-2 py-0.5 text-xs ${cls}`}>{children}</span>;
}

function FileIcon({ ext }) {
  const label = ext?.toUpperCase() || "DOC";
  return (
    <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-slate-100 text-[10px] font-semibold text-slate-600">
      {label}
    </div>
  );
}

function Sidebar({ page, setPage }) {
  const items = [
    { key: "search", label: "搜索", icon: Search },
    { key: "library", label: "文档目录", icon: FolderOpen },
    { key: "status", label: "索引状态", icon: Database },
    { key: "settings", label: "设置", icon: Settings },
  ];
  return (
    <aside className="flex h-full w-56 flex-col border-r border-slate-200 bg-white/80 p-4 backdrop-blur-xl">
      <div className="mb-8 flex items-center gap-3 px-2">
        <div className="flex h-9 w-9 items-center justify-center rounded-2xl bg-slate-900 text-white shadow-sm">dM</div>
        <div>
          <div className="text-sm font-semibold text-slate-900">docMind</div>
          <div className="text-xs text-slate-500">Local document search</div>
        </div>
      </div>
      <nav className="space-y-1">
        {items.map((item) => {
          const Icon = item.icon;
          const active = page === item.key;
          return (
            <button key={item.key} onClick={() => setPage(item.key)} className={`flex w-full items-center gap-3 rounded-2xl px-3 py-2.5 text-sm transition ${active ? "bg-slate-900 text-white shadow-sm" : "text-slate-600 hover:bg-slate-100"}`}>
              <Icon size={17} />
              {item.label}
            </button>
          );
        })}
      </nav>
      <div className="mt-auto rounded-2xl bg-slate-50 p-3 text-xs text-slate-500">
        <div className="mb-1 font-medium text-slate-700">本地优先</div>
        文档只在当前 Mac 上解析和索引，不上传云端。
      </div>
    </aside>
  );
}

function SearchPage() {
  const [query, setQuery] = useState("maven 离线仓库");
  const [selectedId, setSelectedId] = useState("1");
  const selected = mockResults.find((r) => r.id === selectedId) || mockResults[0];
  const results = useMemo(() => query.trim() ? mockResults : [], [query]);

  return (
    <div className="flex h-full flex-col">
      <header className="border-b border-slate-200 bg-white/70 px-8 py-5 backdrop-blur-xl">
        <div className="mb-4 flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-semibold tracking-tight text-slate-950">搜索文档内容</h1>
            <p className="mt-1 text-sm text-slate-500">输入关键词，定位到文档中的具体段落。</p>
          </div>
          <Badge type="success"><CheckCircle2 className="mr-1" size={13} /> 已索引 1,038 个文档</Badge>
        </div>
        <div className="flex items-center gap-3 rounded-3xl border border-slate-200 bg-white px-4 py-3 shadow-sm">
          <Search size={20} className="text-slate-400" />
          <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="搜索文档内容、标题或文件名..." className="flex-1 bg-transparent text-base outline-none placeholder:text-slate-400" />
          <button className="rounded-2xl bg-slate-900 px-4 py-2 text-sm font-medium text-white">搜索</button>
        </div>
      </header>

      <main className="grid min-h-0 flex-1 grid-cols-[minmax(420px,0.95fr)_minmax(360px,0.8fr)] gap-0">
        <section className="min-h-0 overflow-y-auto border-r border-slate-200 bg-slate-50/50 p-5">
          <div className="mb-4 flex items-center justify-between">
            <div className="text-sm text-slate-500">找到 <span className="font-semibold text-slate-800">{results.length}</span> 个相关段落</div>
            <button className="flex items-center gap-1 rounded-xl border border-slate-200 bg-white px-3 py-1.5 text-xs text-slate-600"><Filter size={14} />筛选</button>
          </div>
          <div className="space-y-3">
            {results.map((item) => (
              <motion.button key={item.id} layout onClick={() => setSelectedId(item.id)} className={`w-full rounded-3xl border p-4 text-left shadow-sm transition ${selectedId === item.id ? "border-slate-300 bg-white ring-2 ring-slate-200" : "border-slate-200 bg-white hover:border-slate-300"}`}>
                <div className="flex gap-3">
                  <FileIcon ext={item.ext} />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-start justify-between gap-2">
                      <div className="truncate text-sm font-semibold text-slate-900">{item.fileName}</div>
                      <div className="text-xs text-slate-400">{Math.round(item.score * 100)}%</div>
                    </div>
                    <div className="mt-1 truncate text-xs text-slate-400">{item.path}</div>
                    <div className="mt-2 text-sm leading-6 text-slate-700">{item.snippet}</div>
                    <div className="mt-3 flex items-center gap-2 text-xs text-slate-500">
                      <Badge>{item.heading}</Badge>
                      <span>{item.page ? `第 ${item.page} 页` : `第 ${item.paragraph} 段`}</span>
                      <span>·</span>
                      <span>{item.modified}</span>
                    </div>
                  </div>
                </div>
              </motion.button>
            ))}
          </div>
        </section>

        <aside className="min-h-0 overflow-y-auto bg-white p-6">
          <div className="mb-5 flex items-start justify-between gap-3">
            <div>
              <div className="text-lg font-semibold text-slate-950">{selected.fileName}</div>
              <div className="mt-1 break-all text-xs text-slate-400">{selected.path}</div>
            </div>
            <FileIcon ext={selected.ext} />
          </div>
          <div className="mb-4 flex flex-wrap gap-2">
            <Badge>{selected.ext.toUpperCase()}</Badge>
            <Badge>{selected.page ? `第 ${selected.page} 页` : `第 ${selected.paragraph} 段`}</Badge>
            <Badge><Clock className="mr-1" size={12} />{selected.modified}</Badge>
          </div>
          <div className="rounded-3xl border border-slate-200 bg-slate-50 p-5">
            <div className="mb-2 text-sm font-medium text-slate-700">命中段落</div>
            <p className="text-sm leading-7 text-slate-700">{selected.snippet}</p>
          </div>
          <div className="mt-5 rounded-3xl border border-slate-200 bg-white p-5">
            <div className="mb-2 text-sm font-medium text-slate-700">上下文预览</div>
            <p className="text-sm leading-7 text-slate-500">上一段：构建离线仓库时，需要先解析项目的 parent POM、BOM 以及 build plugins。</p>
            <p className="mt-3 text-sm leading-7 text-slate-800">当前段：{selected.snippet}</p>
            <p className="mt-3 text-sm leading-7 text-slate-500">下一段：生成后的 .offline-repo 可以通过 settings.xml 指定为本地仓库路径。</p>
          </div>
          <div className="mt-6 grid grid-cols-2 gap-3">
            <button className="flex items-center justify-center gap-2 rounded-2xl bg-slate-900 px-4 py-3 text-sm font-medium text-white"><ExternalLink size={16} />打开文件</button>
            <button className="flex items-center justify-center gap-2 rounded-2xl border border-slate-200 bg-white px-4 py-3 text-sm font-medium text-slate-700"><Eye size={16} />快速预览</button>
          </div>
        </aside>
      </main>
    </div>
  );
}

function LibraryPage() {
  return (
    <div className="h-full overflow-y-auto p-8">
      <div className="mb-7 flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight text-slate-950">文档目录</h1>
          <p className="mt-1 text-sm text-slate-500">添加需要索引的文件夹，docMind 会在本地解析和建立索引。</p>
        </div>
        <button className="flex items-center gap-2 rounded-2xl bg-slate-900 px-4 py-2.5 text-sm font-medium text-white"><FolderPlus size={17} />添加目录</button>
      </div>
      <div className="space-y-3">
        {mockDirs.map((dir) => (
          <div key={dir.path} className="rounded-3xl border border-slate-200 bg-white p-5 shadow-sm">
            <div className="flex items-center justify-between gap-4">
              <div className="flex min-w-0 items-center gap-4">
                <div className="flex h-11 w-11 items-center justify-center rounded-2xl bg-slate-100"><FolderOpen size={20} className="text-slate-600" /></div>
                <div className="min-w-0">
                  <div className="truncate text-sm font-semibold text-slate-900">{dir.path}</div>
                  <div className="mt-1 text-xs text-slate-500">{dir.docs} 个文档 · {dir.chunks.toLocaleString()} 个段落</div>
                </div>
              </div>
              <div className="flex items-center gap-2">
                {dir.status === "indexing" ? <Badge type="warning"><Loader2 className="mr-1 animate-spin" size={13} /> 索引中</Badge> : <Badge type="success"><CheckCircle2 className="mr-1" size={13} /> 已完成</Badge>}
                <button className="rounded-xl border border-slate-200 p-2 text-slate-500 hover:bg-slate-50"><RefreshCw size={16} /></button>
                <button className="rounded-xl border border-slate-200 p-2 text-slate-500 hover:bg-slate-50"><X size={16} /></button>
              </div>
            </div>
          </div>
        ))}
      </div>
      <div className="mt-8 rounded-3xl border border-dashed border-slate-300 bg-slate-50 p-8 text-center">
        <FolderPlus className="mx-auto mb-3 text-slate-400" size={28} />
        <div className="text-sm font-medium text-slate-700">添加更多文档目录</div>
        <div className="mt-1 text-xs text-slate-500">建议优先添加 Documents、Downloads 或你的项目资料目录。</div>
      </div>
    </div>
  );
}

function StatusPage() {
  return (
    <div className="h-full overflow-y-auto p-8">
      <div className="mb-7">
        <h1 className="text-2xl font-semibold tracking-tight text-slate-950">索引状态</h1>
        <p className="mt-1 text-sm text-slate-500">查看当前索引进度、失败文件和可重新处理的项目。</p>
      </div>
      <div className="mb-6 grid grid-cols-4 gap-4">
        {[{ label: "已扫描文档", value: "1,128" }, { label: "已索引文档", value: "1,038" }, { label: "段落块", value: "16,124" }, { label: "失败文件", value: "2" }].map((card) => (
          <div key={card.label} className="rounded-3xl border border-slate-200 bg-white p-5 shadow-sm">
            <div className="text-xs text-slate-500">{card.label}</div>
            <div className="mt-2 text-2xl font-semibold text-slate-950">{card.value}</div>
          </div>
        ))}
      </div>
      <div className="rounded-3xl border border-slate-200 bg-white p-6 shadow-sm">
        <div className="mb-4 flex items-center justify-between">
          <div>
            <div className="text-sm font-semibold text-slate-900">当前任务</div>
            <div className="mt-1 text-xs text-slate-500">正在解析 Downloads 目录中的新文档</div>
          </div>
          <Badge type="warning"><Loader2 className="mr-1 animate-spin" size={13} /> 处理中</Badge>
        </div>
        <div className="h-2 rounded-full bg-slate-100">
          <div className="h-2 w-[68%] rounded-full bg-slate-900" />
        </div>
        <div className="mt-2 text-xs text-slate-500">768 / 1,128 个文件</div>
      </div>
      <div className="mt-6 rounded-3xl border border-slate-200 bg-white p-6 shadow-sm">
        <div className="mb-4 flex items-center gap-2 text-sm font-semibold text-slate-900"><AlertCircle size={18} className="text-amber-500" /> 解析失败</div>
        <div className="space-y-3">
          {failedFiles.map((f) => (
            <div key={f.file} className="flex items-center justify-between rounded-2xl bg-slate-50 px-4 py-3">
              <div>
                <div className="text-sm font-medium text-slate-800">{f.file}</div>
                <div className="mt-1 text-xs text-slate-500">{f.reason}</div>
              </div>
              <button className="text-xs font-medium text-slate-600">重新处理</button>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function SettingsPage() {
  return (
    <div className="h-full overflow-y-auto p-8">
      <div className="mb-7">
        <h1 className="text-2xl font-semibold tracking-tight text-slate-950">设置</h1>
        <p className="mt-1 text-sm text-slate-500">控制文件过滤、索引策略和本地数据。</p>
      </div>
      <div className="space-y-5">
        <div className="rounded-3xl border border-slate-200 bg-white p-6 shadow-sm">
          <div className="mb-4 text-sm font-semibold text-slate-900">索引规则</div>
          <div className="space-y-4">
            <label className="flex items-center justify-between text-sm text-slate-700"><span>最大文件大小</span><select className="rounded-xl border border-slate-200 bg-white px-3 py-2"><option>50 MB</option><option>100 MB</option><option>200 MB</option></select></label>
            <label className="flex items-center justify-between text-sm text-slate-700"><span>隐藏文件</span><Badge>默认排除</Badge></label>
            <label className="flex items-center justify-between text-sm text-slate-700"><span>支持格式</span><span className="text-xs text-slate-500">TXT, MD, HTML, DOCX, PDF</span></label>
          </div>
        </div>
        <div className="rounded-3xl border border-slate-200 bg-white p-6 shadow-sm">
          <div className="mb-4 text-sm font-semibold text-slate-900">排除目录</div>
          <div className="flex flex-wrap gap-2">
            {["node_modules", ".git", "target", "Library", "Applications", "System"].map((x) => <Badge key={x}>{x}</Badge>)}
          </div>
        </div>
        <div className="rounded-3xl border border-red-100 bg-red-50 p-6">
          <div className="text-sm font-semibold text-red-800">危险操作</div>
          <div className="mt-1 text-xs text-red-600">清空索引不会删除原始文件，只会删除 docMind 的本地索引数据。</div>
          <button className="mt-4 rounded-2xl bg-white px-4 py-2 text-sm font-medium text-red-700 shadow-sm">清空全部索引</button>
        </div>
      </div>
    </div>
  );
}

export default function DocMindMvpPrototype() {
  const [page, setPage] = useState("search");
  return (
    <div className="h-screen w-full overflow-hidden bg-slate-100 text-slate-900">
      <div className="flex h-full">
        <Sidebar page={page} setPage={setPage} />
        <div className="min-w-0 flex-1 bg-slate-50">
          {page === "search" && <SearchPage />}
          {page === "library" && <LibraryPage />}
          {page === "status" && <StatusPage />}
          {page === "settings" && <SettingsPage />}
        </div>
      </div>
    </div>
  );
}
