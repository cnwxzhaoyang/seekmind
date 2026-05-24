<template>
  <div class="docmind-app">
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

      <div class="local-card">
        <p class="local-title">本地优先</p>
        <p class="local-desc">文档仅在当前 Mac 上解析和索引，不上传云端。</p>
      </div>
    </aside>

    <!-- 主内容 -->
    <main class="main-content">
      <!-- 顶部栏 -->
      <header class="top-bar">
        <div class="search-input-wrapper">
          <i class="fa-solid fa-magnifying-glass search-icon"></i>
          <input
            v-model="keyword"
            type="text"
            class="search-input"
            placeholder="搜索文档内容、标题或文件名... (Cmd+K)"
            @keydown.enter="handleSearch"
          />
        </div>

        <div class="stats-strip">
          <div class="stats-item">
            SQLite: <span>{{ stats.sqlite }}</span>
          </div>
          <div class="stats-item">
            Tantivy: <span>{{ stats.tantivy }}</span>
          </div>
          <div class="stats-item">
            语义权重: <span>{{ stats.semanticWeight }}</span>
          </div>
        </div>
      </header>

      <!-- 工作区 -->
      <div class="workspace">
        <!-- 左侧辅助面板 -->
        <section class="left-panel">
          <div class="panel-section">
            <div class="section-title">最近搜索</div>
            <div class="tag-list">
              <button
                v-for="item in recentSearches"
                :key="item"
                class="tag"
                @click="keyword = item"
              >
                {{ item }}
              </button>
            </div>
          </div>

          <div class="panel-section">
            <div class="section-title">常用目录</div>
            <div
              v-for="dir in commonDirs"
              :key="dir"
              class="list-item"
              :title="dir"
            >
              <i class="fa-regular fa-folder"></i>
              <span>{{ dir }}</span>
            </div>
          </div>

          <div class="panel-section">
            <div class="section-title">最近打开</div>
            <div
              v-for="file in recentFiles"
              :key="file"
              class="list-item"
              :title="file"
            >
              <i class="fa-regular fa-file-lines"></i>
              <span>{{ file }}</span>
            </div>
          </div>
        </section>

        <!-- 搜索结果 -->
        <section class="results-panel">
          <div class="results-header">
            <span>
              找到 {{ resultSummary.documentCount }} 个相关文档，共
              {{ resultSummary.paragraphCount }} 个相关段落
            </span>

            <button class="filter-btn" type="button">
              <i class="fa-solid fa-sliders"></i>
              筛选
            </button>
          </div>

          <div class="results-scroll">
            <article
              v-for="result in results"
              :key="result.id"
              class="result-card"
            >
              <div class="result-meta">
                <span>
                  <i :class="result.icon"></i>
                  {{ result.fileName }}
                </span>
                <span>• {{ result.size }}</span>
                <span>• {{ result.date }}</span>
              </div>

              <div class="result-title">{{ result.title }}</div>

              <div class="result-snippet" v-html="result.snippet"></div>
            </article>

            <div v-if="results.length === 0" class="empty-state">
              请输入关键词开始搜索
            </div>

            <div v-else class="load-end">
              已加载全部结果
            </div>
          </div>
        </section>
      </div>

      <!-- 状态栏 -->
      <footer class="status-bar">
        <div>
          <i class="fa-solid fa-circle-check text-green"></i>
          已索引 {{ indexedCount }} 个文档
        </div>
        <div>
          <i class="fa-solid fa-bolt text-yellow"></i>
          Python 解析已就绪
        </div>

        <div class="status-right">
          <span>
            <i class="fa-solid fa-terminal"></i>
            日志 ({{ logCount }})
          </span>
          <span>{{ version }}</span>
        </div>
      </footer>
    </main>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'

type NavItem = {
  key: string
  label: string
  icon: string
}

type SearchResult = {
  id: number
  icon: string
  fileName: string
  size: string
  date: string
  title: string
  snippet: string
}

const activeNav = ref('search')
const keyword = ref('')

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

const stats = {
  sqlite: '50/1773',
  tantivy: '1773',
  semanticWeight: '25%',
}

const recentSearches = ['什么是AEOS', '数据加密', 'Cd linux', 'oracle']

const commonDirs = [
  '/Documents/503-协同...',
  '/Documents/803 一体...',
  '/Documents/产品研发...',
]

const recentFiles = ['AI大模型与架构']

const indexedCount = 50
const logCount = 3
const version = 'v1.0.2'

const mockResults: SearchResult[] = [
  {
    id: 1,
    icon: 'fa-regular fa-file-pdf',
    fileName: 'AEOS_Technical_Specs.pdf',
    size: '3MB',
    date: '2024-05-20',
    title: 'AEOS 核心架构与安全性说明',
    snippet:
      '...AEOS 系统采用分层加密机制，其中 <strong>数据加密</strong> 模块位于内核态，确保了在高性能搜索的同时不牺牲安全性。在索引阶段，docMind 会自动提取...',
  },
  {
    id: 2,
    icon: 'fa-regular fa-file-word',
    fileName: '2024产品研发规划.docx',
    size: '1.2MB',
    date: '2024-05-15',
    title: '产品研发安全合规性要求',
    snippet:
      '...所有本地文档必须经过 <strong>本地优先</strong> 解析流程。文档仅在当前设备上进行索引，严禁上传至任何第三方云端 API 进行解析，除非用户手动开启...',
  },
]

const results = computed(() => {
  // 原型阶段：输入为空也展示示例结果。接入真实搜索时替换为 API/Tauri invoke 返回值即可。
  return mockResults
})

const resultSummary = computed(() => ({
  documentCount: 12,
  paragraphCount: 45,
}))

function handleSearch() {
  // TODO: Tauri 接入示例：
  // const data = await invoke<SearchResult[]>('search_documents', { keyword: keyword.value })
  console.log('search:', keyword.value)
}
</script>

<style scoped>
:global(body) {
  margin: 0;
}

:global(*) {
  box-sizing: border-box;
}

.docmind-app {
  --sidebar-bg: #f3f4f6;
  --main-bg: #ffffff;
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
  padding: 8px 12px;
  margin-bottom: 16px;
}

.logo-icon {
  width: 24px;
  height: 24px;
  display: grid;
  place-items: center;
  border-radius: 4px;
  background: var(--accent-color);
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

.nav-item i {
  width: 14px;
}

.local-card {
  margin-top: auto;
  padding: 12px;
  border: 1px solid #e0e7ff;
  border-radius: 10px;
  background: #eef2ff;
}

.local-title {
  margin: 0 0 4px;
  color: #4f46e5;
  font-size: 10px;
  font-weight: 700;
}

.local-desc {
  margin: 0;
  color: #818cf8;
  font-size: 10px;
  line-height: 1.45;
}

.main-content {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  background-color: var(--main-bg);
}

.top-bar {
  height: 48px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 0 16px;
  border-bottom: 1px solid var(--border-color);
}

.search-input-wrapper {
  position: relative;
  flex: 1;
  max-width: 600px;
}

.search-input {
  width: 100%;
  padding: 6px 12px 6px 32px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  outline: none;
  font-size: 13px;
  transition:
    border-color 0.2s ease,
    box-shadow 0.2s ease;
}

.search-input:focus {
  border-color: var(--accent-color);
  box-shadow: 0 0 0 2px rgba(79, 70, 229, 0.1);
}

.search-icon {
  position: absolute;
  top: 50%;
  left: 10px;
  color: var(--text-muted);
  font-size: 12px;
  transform: translateY(-50%);
}

.stats-strip {
  display: flex;
  gap: 16px;
  color: var(--text-muted);
  font-size: 12px;
  white-space: nowrap;
}

.stats-item span {
  color: var(--text-main);
  font-weight: 500;
}

.workspace {
  flex: 1;
  min-height: 0;
  display: flex;
  overflow: hidden;
}

.left-panel {
  width: 260px;
  flex-shrink: 0;
  padding: 16px;
  overflow-y: auto;
  border-right: 1px solid var(--border-color);
}

.panel-section {
  margin-bottom: 24px;
}

.section-title {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
  color: var(--text-muted);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
}

.tag-list {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.tag {
  border: 0;
  border-radius: 4px;
  background: #f3f4f6;
  padding: 2px 6px;
  color: var(--text-main);
  font-size: 10px;
  cursor: pointer;
}

.tag:hover {
  background: #e5e7eb;
}

.list-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-radius: 4px;
  font-size: 12px;
  cursor: pointer;
}

.list-item:hover {
  background-color: #f9fafb;
}

.list-item i {
  width: 14px;
  color: var(--text-muted);
}

.list-item span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.results-panel {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  background-color: #f9fafb;
}

.results-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 16px;
  border-bottom: 1px solid var(--border-color);
  background: #ffffff;
  color: #6b7280;
  font-size: 12px;
  font-weight: 500;
}

.filter-btn {
  border: 0;
  background: transparent;
  color: var(--accent-color);
  font-size: 12px;
  cursor: pointer;
}

.filter-btn:hover {
  text-decoration: underline;
}

.results-scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 16px;
}

.result-card {
  margin-bottom: 12px;
  padding: 12px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  background: #ffffff;
  transition:
    border-color 0.1s ease,
    box-shadow 0.1s ease,
    transform 0.1s ease;
}

.result-card:hover {
  border-color: var(--accent-color);
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.05);
}

.result-meta {
  display: flex;
  gap: 12px;
  margin-bottom: 4px;
  color: var(--text-muted);
  font-size: 11px;
}

.result-title {
  margin-bottom: 6px;
  color: var(--accent-color);
  font-size: 14px;
  font-weight: 600;
}

.result-snippet {
  color: #4b5563;
  font-size: 13px;
  line-height: 1.5;
}

.empty-state,
.load-end {
  padding: 32px 0;
  text-align: center;
  color: #9ca3af;
  font-size: 12px;
}

.status-bar {
  height: 24px;
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

.text-yellow {
  color: #eab308;
}

@media (max-width: 960px) {
  .stats-strip {
    display: none;
  }

  .left-panel {
    display: none;
  }
}
</style>
