<template>
  <div class="docmind-chunks-app">
    <!-- 侧边栏 -->
    <aside class="sidebar">
      <div class="logo-area">
        <div class="logo-icon">dm</div>
        <span class="logo-text">docMind</span>
      </div>

      <nav class="nav">
        <a
          v-for="item in navItems"
          :key="item.key"
          href="#"
          class="nav-item"
          :class="{ active: activeNav === item.key }"
          @click.prevent="activeNav = item.key"
        >
          <i :class="item.icon"></i>
          {{ item.label }}
        </a>
      </nav>
    </aside>

    <!-- 主内容 -->
    <main class="main-content">
      <header class="top-bar">
        <div class="page-title">
          文档切片
          <span class="page-badge">按目录管理</span>
        </div>

        <div class="top-actions">
          <div class="parser-status">
            当前解析器:
            <span>Python</span>
          </div>

          <button class="refresh-btn" type="button" @click="handleRefresh">
            <i class="fa-solid fa-rotate"></i>
            刷新
          </button>
        </div>
      </header>

      <div class="three-pane-layout">
        <!-- 第一栏：目录 -->
        <section class="pane-directories">
          <div class="pane-header">
            索引目录
            <span>{{ directories.length }}</span>
          </div>

          <div class="scroll-area">
            <button
              v-for="dir in directories"
              :key="dir.id"
              class="dir-item"
              :class="{ active: selectedDirectoryId === dir.id }"
              type="button"
              @click="selectedDirectoryId = dir.id"
            >
              <div class="dir-path" :title="dir.path">
                {{ dir.path }}
              </div>
              <div class="dir-meta">
                <span>{{ dir.docCount }} 文档 · {{ dir.chunkCount }} 切片</span>
                <span class="enabled-dot">● Enabled</span>
              </div>
            </button>
          </div>
        </section>

        <!-- 第二栏：文件 -->
        <section class="pane-files">
          <div class="pane-header">
            文件列表
            <span>{{ filteredFiles.length }}</span>
          </div>

          <div class="file-filter">
            <div class="filter-input-wrapper">
              <i class="fa-solid fa-filter filter-icon"></i>
              <input
                v-model="fileKeyword"
                type="text"
                class="filter-input"
                placeholder="过滤文件名或路径..."
              />
            </div>
          </div>

          <div class="scroll-area">
            <button
              v-for="file in filteredFiles"
              :key="file.id"
              class="file-item"
              :class="{ active: selectedFileId === file.id }"
              type="button"
              @click="selectedFileId = file.id"
            >
              <div class="file-name">
                <i :class="file.icon"></i>
                <span>{{ file.name }}</span>
              </div>

              <div class="file-info">
                {{ file.chunkCount }} 个切片 · {{ file.date }}
              </div>

              <i
                class="fa-solid fa-wand-magic-sparkles re-chunk-btn"
                title="重新切片"
                @click.stop="handleRechunk(file)"
              ></i>
            </button>
          </div>
        </section>

        <!-- 第三栏：详情 -->
        <section class="pane-details">
          <div class="pane-header detail-header">
            切片详情预览
            <span>{{ selectedFile?.name || '未选择文件' }}</span>
          </div>

          <div class="chunk-block">
            <article
              v-for="chunk in selectedChunks"
              :key="chunk.id"
              class="chunk-item"
            >
              <div class="chunk-number">#{{ chunk.index }}</div>

              <div class="chunk-content">
                <strong v-if="chunk.title">{{ chunk.title }}</strong>
                <br v-if="chunk.title" />
                {{ chunk.content }}
              </div>

              <div class="chunk-footer">
                <span>字符数: {{ chunk.charCount }}</span>
                <span>位置: {{ chunk.position }}</span>
              </div>
            </article>

            <div v-if="selectedChunks.length === 0" class="empty-state">
              当前文件暂无切片
            </div>
          </div>
        </section>
      </div>

      <footer class="status-bar">
        <div>
          <i class="fa-solid fa-circle-check text-green"></i>
          解析就绪
        </div>

        <div class="status-right">
          <span>
            <i class="fa-solid fa-database"></i>
            存储: SQLite + Tantivy
          </span>
          <span>{{ version }}</span>
        </div>
      </footer>
    </main>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'

type NavItem = {
  key: string
  label: string
  icon: string
}

type DirectoryItem = {
  id: number
  path: string
  docCount: number
  chunkCount: number
}

type FileItem = {
  id: number
  directoryId: number
  name: string
  icon: string
  chunkCount: number
  date: string
}

type ChunkItem = {
  id: number
  fileId: number
  index: number
  title?: string
  content: string
  charCount: number
  position: string
}

const activeNav = ref('chunks')
const selectedDirectoryId = ref(1)
const selectedFileId = ref(1)
const fileKeyword = ref('')
const version = 'v1.0.2'

const navItems: NavItem[] = [
  {
    key: 'search',
    label: '全局搜索',
    icon: 'fa-solid fa-magnifying-glass',
  },
  {
    key: 'chunks',
    label: '文档切片',
    icon: 'fa-solid fa-layer-group',
  },
  {
    key: 'directories',
    label: '文档目录',
    icon: 'fa-solid fa-folder-tree',
  },
  {
    key: 'index',
    label: '索引状态',
    icon: 'fa-solid fa-chart-line',
  },
  {
    key: 'settings',
    label: '设置',
    icon: 'fa-solid fa-gear',
  },
]

const directories: DirectoryItem[] = [
  {
    id: 1,
    path: '/Documents/503-协同...',
    docCount: 16,
    chunkCount: 359,
  },
  {
    id: 2,
    path: '/Documents/803 一体...',
    docCount: 15,
    chunkCount: 158,
  },
  {
    id: 3,
    path: '/Documents/MarkdownHome',
    docCount: 4,
    chunkCount: 218,
  },
]

const files: FileItem[] = [
  {
    id: 1,
    directoryId: 1,
    name: '15 设计集成系统培训文档.pdf',
    icon: 'fa-regular fa-file-pdf file-pdf',
    chunkCount: 6,
    date: '2024-05-20',
  },
  {
    id: 2,
    directoryId: 1,
    name: '16 设计集成系统测试大纲.docx',
    icon: 'fa-regular fa-file-word file-word',
    chunkCount: 12,
    date: '2024-05-18',
  },
  {
    id: 3,
    directoryId: 1,
    name: '17 系统运行总结报告.txt',
    icon: 'fa-regular fa-file-lines file-text',
    chunkCount: 25,
    date: '2024-05-15',
  },
  {
    id: 4,
    directoryId: 2,
    name: '803 一体化平台设计说明.pdf',
    icon: 'fa-regular fa-file-pdf file-pdf',
    chunkCount: 18,
    date: '2024-05-12',
  },
  {
    id: 5,
    directoryId: 3,
    name: 'AI大模型与架构.md',
    icon: 'fa-regular fa-file-lines file-text',
    chunkCount: 32,
    date: '2024-05-10',
  },
]

const chunks: ChunkItem[] = [
  {
    id: 1,
    fileId: 1,
    index: 1,
    title: '项目概况',
    content:
      '图 1 系统建设目标 图 2 系统实施范围 图 3 系统应用架构 图 4 系统之间关系 图 5 系统内部模块关系 图 6 项目历程',
    charCount: 124,
    position: '第 1 页',
  },
  {
    id: 2,
    fileId: 1,
    index: 2,
    title: '需求实现情况',
    content:
      '图 7 系统总体完成情况 图 8 研发流程管理—完成情况（研发工作台） 图 9 研发流程管理—研发门户 图 10 研发流程管理—完成情况（研发项目管理 1） 图 11 研发流程管理—完成情况（研发项目管理 2） 图 12 研发流程管理—项目列表 图 13 研发流程管理—项目团队 图 14 研发流程管理—项目任务 图 15 研发流程管理—综合看板 图 16 研发流程管理—完成情况（资源库管理） 图 17 研发流程管理—技术要素库 图 18 研发流程管理—完成情况（研发模板管理与定制） 图 19 研发流程管理—完成情况（研制阶段管理）',
    charCount: 456,
    position: '第 2-3 页',
  },
  {
    id: 3,
    fileId: 1,
    index: 3,
    content:
      '图 20 研发流程管理—完成情况（WBS 分解 1） 图 21 研发流程管理—完成情况（WBS 分解 2） 图 22 研发流程管理—完成情况（产品设计仿真流程管理—项目策划管理—项目包模板） 图 23 研发流程管理—工具模板 图 24 研发流程管理—项目策划封装 图 25 研发流程管理—工作包模板 图 26 研发流程管理—工具模板 图 27 研发流程管理—完成情况（工作流协同） 图 28 研发流程管理—完成情况（工作项处理）',
    charCount: 382,
    position: '第 4 页',
  },
]

const filteredFiles = computed(() => {
  const keyword = fileKeyword.value.trim().toLowerCase()

  return files.filter((file) => {
    const matchDirectory = file.directoryId === selectedDirectoryId.value
    const matchKeyword = !keyword || file.name.toLowerCase().includes(keyword)
    return matchDirectory && matchKeyword
  })
})

const selectedFile = computed(() => {
  return files.find((file) => file.id === selectedFileId.value)
})

const selectedChunks = computed(() => {
  return chunks.filter((chunk) => chunk.fileId === selectedFileId.value)
})

watch(
  () => selectedDirectoryId.value,
  () => {
    const firstFile = filteredFiles.value[0]
    selectedFileId.value = firstFile?.id ?? -1
  },
)

function handleRefresh() {
  // TODO: Tauri 接入示例：
  // await invoke('refresh_chunks')
  console.log('refresh chunks')
}

function handleRechunk(file: FileItem) {
  // TODO: Tauri 接入示例：
  // await invoke('rechunk_file', { fileId: file.id })
  console.log('rechunk file:', file.name)
}
</script>

<style scoped>
:global(body) {
  margin: 0;
}

:global(*) {
  box-sizing: border-box;
}

.docmind-chunks-app {
  --sidebar-bg: #f3f4f6;
  --pane-bg: #f9fafb;
  --content-bg: #ffffff;
  --accent-color: #4f46e5;
  --border-color: #e5e7eb;
  --text-main: #1f2937;
  --text-muted: #6b7280;

  height: 100vh;
  display: flex;
  overflow: hidden;
  background-color: var(--sidebar-bg);
  color: var(--text-main);
  font-family:
    Inter,
    -apple-system,
    BlinkMacSystemFont,
    "Segoe UI",
    Roboto,
    sans-serif;
}

.sidebar {
  width: 200px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  padding: 12px 8px;
  border-right: 1px solid var(--border-color);
  background-color: var(--sidebar-bg);
}

.logo-area {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 12px;
  margin-bottom: 16px;
}

.logo-icon {
  width: 24px;
  height: 24px;
  display: grid;
  place-items: center;
  border-radius: 4px;
  background: #4f46e5;
  color: #ffffff;
  font-size: 12px;
  font-weight: 700;
}

.logo-text {
  font-size: 14px;
  font-weight: 700;
}

.nav {
  display: flex;
  flex-direction: column;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 2px;
  padding: 8px 12px;
  border-radius: 6px;
  color: var(--text-muted);
  text-decoration: none;
  font-size: 13px;
  transition: all 0.2s ease;
}

.nav-item:hover {
  background-color: #e5e7eb;
  color: var(--text-main);
}

.nav-item.active {
  background-color: #e5e7eb;
  color: var(--accent-color);
  font-weight: 500;
}

.main-content {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  background-color: var(--content-bg);
}

.top-bar {
  height: 48px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 16px;
  border-bottom: 1px solid var(--border-color);
  background: #ffffff;
}

.page-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 14px;
  font-weight: 600;
}

.page-badge {
  border-radius: 4px;
  background: #f3f4f6;
  padding: 2px 6px;
  color: #6b7280;
  font-size: 10px;
  font-weight: 400;
}

.top-actions {
  display: flex;
  align-items: center;
  gap: 16px;
}

.parser-status {
  color: #6b7280;
  font-size: 11px;
}

.parser-status span {
  color: #4f46e5;
  font-weight: 500;
}

.refresh-btn {
  display: flex;
  align-items: center;
  gap: 5px;
  border: 1px solid #e5e7eb;
  border-radius: 6px;
  background: #ffffff;
  padding: 4px 8px;
  color: #374151;
  font-size: 12px;
  cursor: pointer;
}

.refresh-btn:hover {
  background: #f9fafb;
}

.three-pane-layout {
  flex: 1;
  min-height: 0;
  display: flex;
  overflow: hidden;
}

.pane-directories {
  width: 240px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  border-right: 1px solid var(--border-color);
  background-color: var(--pane-bg);
}

.pane-files {
  width: 300px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  border-right: 1px solid var(--border-color);
  background-color: var(--pane-bg);
}

.pane-details {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  overflow-y: auto;
  background-color: var(--content-bg);
}

.pane-header {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  min-height: 36px;
  padding: 10px 16px;
  border-bottom: 1px solid var(--border-color);
  background: rgba(255, 255, 255, 0.5);
  color: var(--text-muted);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
}

.detail-header {
  position: sticky;
  top: 0;
  z-index: 10;
}

.detail-header span {
  font-size: 10px;
  font-weight: 400;
  letter-spacing: 0;
  text-transform: none;
}

.scroll-area {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 8px;
}

.dir-item {
  width: 100%;
  margin-bottom: 4px;
  padding: 8px 12px;
  border: 1px solid transparent;
  border-radius: 6px;
  background: transparent;
  text-align: left;
  cursor: pointer;
  transition:
    background 0.2s ease,
    border-color 0.2s ease,
    box-shadow 0.2s ease;
}

.dir-item:hover {
  background: #f3f4f6;
}

.dir-item.active {
  border-color: var(--border-color);
  background: #ffffff;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
}

.dir-path {
  overflow: hidden;
  color: var(--text-main);
  font-size: 12px;
  font-weight: 500;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dir-meta {
  display: flex;
  justify-content: space-between;
  margin-top: 2px;
  color: var(--text-muted);
  font-size: 10px;
}

.enabled-dot {
  color: #16a34a;
}

.file-filter {
  padding: 8px;
}

.filter-input-wrapper {
  position: relative;
}

.filter-icon {
  position: absolute;
  top: 50%;
  left: 8px;
  color: #9ca3af;
  font-size: 10px;
  transform: translateY(-50%);
}

.filter-input {
  width: 100%;
  border: 1px solid #e5e7eb;
  border-radius: 6px;
  background: #ffffff;
  padding: 6px 8px 6px 24px;
  outline: none;
  font-size: 12px;
}

.filter-input:focus {
  border-color: #6366f1;
}

.file-item {
  position: relative;
  width: 100%;
  margin-bottom: 4px;
  padding: 10px 12px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  text-align: left;
  cursor: pointer;
  transition:
    background 0.2s ease,
    box-shadow 0.2s ease;
}

.file-item:hover {
  background: #f3f4f6;
}

.file-item.active {
  background: #ffffff;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
}

.file-name {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 2px;
  padding-right: 20px;
  font-size: 12px;
  font-weight: 500;
}

.file-name span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.file-info {
  color: var(--text-muted);
  font-size: 10px;
}

.file-pdf {
  color: #ef4444;
}

.file-word {
  color: #3b82f6;
}

.file-text {
  color: #6b7280;
}

.re-chunk-btn {
  position: absolute;
  top: 50%;
  right: 12px;
  color: var(--accent-color);
  opacity: 0;
  transform: translateY(-50%);
  transition: opacity 0.2s ease;
}

.file-item:hover .re-chunk-btn {
  opacity: 1;
}

.chunk-block {
  width: 100%;
  max-width: 800px;
  margin: 0 auto;
  padding: 24px;
}

.chunk-item {
  position: relative;
  margin-bottom: 32px;
  padding-left: 20px;
  border-left: 2px solid #e5e7eb;
}

.chunk-item:hover {
  border-left-color: var(--accent-color);
}

.chunk-number {
  position: absolute;
  top: 0;
  left: -10px;
  background: #ffffff;
  color: var(--text-muted);
  padding: 2px 0;
  font-size: 10px;
  font-weight: 700;
}

.chunk-content {
  color: #374151;
  font-size: 14px;
  line-height: 1.6;
}

.chunk-footer {
  display: flex;
  gap: 12px;
  margin-top: 8px;
  color: var(--text-muted);
  font-size: 11px;
}

.empty-state {
  padding: 48px 0;
  text-align: center;
  color: #9ca3af;
  font-size: 12px;
}

.status-bar {
  height: 24px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 0 12px;
  border-top: 1px solid var(--border-color);
  background: var(--sidebar-bg);
  color: var(--text-muted);
  font-size: 11px;
}

.status-right {
  display: flex;
  gap: 12px;
  margin-left: auto;
}

.text-green {
  color: #22c55e;
}

@media (max-width: 1100px) {
  .pane-directories {
    width: 220px;
  }

  .pane-files {
    width: 260px;
  }
}
</style>
