<template>
  <div class="search-highlight-wrapper">
    <!-- 搜索框 -->
    <div class="search-container">
      <SvgIcon icon="icon-search" class="search-icon" />
      <input
        v-model="searchQuery"
        type="text"
        class="search-box"
        placeholder="输入搜索关键词..."
        @input="handleSearch"
      />
      <span class="search-shortcut">Cmd+K</span>
    </div>

    <!-- 搜索结果 -->
    <div class="results-section">
      <div class="results-header">
        <h2 class="results-title">搜索结果</h2>
        <p class="results-count" v-if="filteredResults.length > 0">
          找到 {{ filteredResults.length }} 个相关文档
        </p>
        <p class="no-results" v-else>
          {{ searchQuery ? '未找到匹配的结果' : '输入关键词开始搜索' }}
        </p>
      </div>

      <!-- 结果卡片列表 -->
      <div class="results-list">
        <SearchResultCardWithIcons
          v-for="result in filteredResults"
          :key="result.id"
          :result="result"
          :search-query="searchQuery"
          @click="handleResultClick(result)"
        />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import SvgIcon from './SvgIcon.vue'
import SearchResultCardWithIcons from './SearchResultCardWithIcons.vue'

interface SearchResult {
  id: number
  path: string
  title: string
  content: string
  type: 'PDF' | 'DOCX' | 'TXT'
  match: number
  pages: number
  updated: string
}

// 数据
const searchQuery = ref<string>('')
const results = ref<SearchResult[]>([
  {
    id: 1,
    path: '/Legal/Corporate Governance/2024/',
    title: '备忘录 - ACME 与 Globex 之间的谅解备忘录',
    content: '这份备忘录是由 ACME 公司（"ACME"）和 Globex 公司（"Globex"）于 2024 年 5 月 15 日签署的，涉及潜在的战略合作。',
    type: 'PDF',
    match: 98,
    pages: 24,
    updated: '2024年5月15日'
  },
  {
    id: 2,
    path: '/Product/PRD/2024/',
    title: '产品需求备忘录 - 项目 Orion',
    content: '这份产品需求备忘录 (PRD) 概述了项目 Orion 的愿景、目标、功能需求和成功指标。',
    type: 'DOCX',
    match: 92,
    pages: 18,
    updated: '2024年4月28日'
  },
  {
    id: 3,
    path: '/Engineering/Design Docs/2024/',
    title: '技术设计备忘录 - Auth Service v2',
    content: '这份技术设计备忘录描述了 Auth Service v2 的架构和设计决策。',
    type: 'PDF',
    match: 88,
    pages: 31,
    updated: '2024年4月10日'
  }
])

// 计算属性：根据搜索词过滤结果
const filteredResults = computed(() => {
  if (!searchQuery.value.trim()) {
    return []
  }

  const query = searchQuery.value.toLowerCase()
  return results.value.filter(result => {
    const titleMatch = result.title.toLowerCase().includes(query)
    const contentMatch = result.content.toLowerCase().includes(query)
    const pathMatch = result.path.toLowerCase().includes(query)
    return titleMatch || contentMatch || pathMatch
  })
})

// 方法
const handleSearch = () => {
  console.log(`搜索: ${searchQuery.value}`)
}

const handleResultClick = (result: SearchResult) => {
  console.log('点击结果:', result)
}
</script>

<style scoped>
.search-highlight-wrapper {
  background-color: #0d1117;
  color: #e6edf3;
  padding: 24px;
  border-radius: 8px;
  min-height: 100vh;
}

.search-container {
  position: relative;
  margin-bottom: 32px;
  max-width: 600px;
}

.search-icon {
  position: absolute;
  left: 12px;
  top: 50%;
  transform: translateY(-50%);
  color: #8b949e;
}

.search-box {
  width: 100%;
  padding: 10px 40px 10px 40px;
  background-color: #161b22;
  border: 1px solid #30363d;
  border-radius: 8px;
  color: #e6edf3;
  font-size: 14px;
  transition: all 0.2s;
}

.search-box:focus {
  outline: none;
  border-color: #58a6ff;
  background-color: #0d1117;
  box-shadow: 0 0 0 3px rgba(88, 166, 255, 0.1);
}

.search-shortcut {
  position: absolute;
  right: 12px;
  top: 50%;
  transform: translateY(-50%);
  font-size: 12px;
  color: #8b949e;
  background-color: #21262d;
  padding: 4px 8px;
  border-radius: 4px;
  border: 1px solid #30363d;
}

.results-section {
  margin-top: 24px;
}

.results-header {
  margin-bottom: 20px;
}

.results-title {
  font-size: 20px;
  font-weight: 600;
  color: #e6edf3;
  margin-bottom: 6px;
}

.results-count {
  font-size: 14px;
  color: #8b949e;
  margin: 0;
}

.no-results {
  font-size: 14px;
  color: #8b949e;
  margin: 0;
}

.results-list {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
</style>
