<template>
  <div class="result-card" @click="$emit('click')">
    <!-- 文件路径 -->
    <div class="result-path">{{ result.path }}</div>

    <!-- 标题（带高亮） -->
    <div class="result-title">
      <HighlightText :text="result.title" :search-query="searchQuery" />
    </div>

    <!-- 内容摘要（带高亮） -->
    <div class="result-snippet">
      <HighlightText :text="result.content" :search-query="searchQuery" />
    </div>

    <!-- 元数据 -->
    <div class="result-meta">
      <!-- 文件类型 -->
      <div class="meta-item">
        <span class="file-type-badge" :class="`${result.type.toLowerCase()}-badge`">
          <SvgIcon :icon="getFileIcon(result.type)" size="sm" />
          {{ result.type }}
        </span>
      </div>

      <!-- 匹配度 -->
      <div class="meta-item">
        匹配度 <strong>{{ result.match }}%</strong>
      </div>

      <!-- 页数 -->
      <div class="meta-item">
        <strong>{{ result.pages }}</strong> 页
      </div>

      <!-- 更新时间 -->
      <div class="meta-item" style="margin-left: auto; color: #8b949e">
        <SvgIcon icon="icon-clock" size="sm" />
        更新于 {{ result.updated }}
      </div>

      <!-- 操作按钮 -->
      <div class="result-actions">
        <button class="action-btn" title="更多选项">
          <SvgIcon icon="icon-more" size="md" />
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import SvgIcon from './SvgIcon.vue'
import HighlightText from './HighlightText.vue'

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

defineProps<{
  result: SearchResult
  searchQuery: string
}>()

defineEmits<{
  click: []
}>()

const getFileIcon = (type: string): string => {
  const icons: Record<string, string> = {
    PDF: 'icon-pdf',
    DOCX: 'icon-word',
    TXT: 'icon-file'
  }
  return icons[type] || 'icon-file'
}
</script>

<style scoped>
.result-card {
  background-color: #161b22;
  border: 1px solid #30363d;
  border-radius: 8px;
  padding: 16px;
  transition: all 0.2s;
  cursor: pointer;
}

.result-card:hover {
  border-color: #58a6ff;
  background-color: #0d1117;
  box-shadow: 0 0 0 1px #58a6ff;
}

.result-path {
  font-size: 12px;
  color: #58a6ff;
  margin-bottom: 8px;
  font-family: 'Monaco', 'Menlo', 'Ubuntu Mono', monospace;
  word-break: break-all;
}

.result-title {
  font-size: 16px;
  font-weight: 600;
  color: #e6edf3;
  margin-bottom: 8px;
  line-height: 1.4;
}

.result-snippet {
  font-size: 13px;
  color: #c9d1d9;
  line-height: 1.6;
  margin-bottom: 12px;
  word-break: break-word;
}

.result-meta {
  display: flex;
  gap: 16px;
  align-items: center;
  font-size: 12px;
  color: #8b949e;
  padding-top: 12px;
  border-top: 1px solid #30363d;
  flex-wrap: wrap;
}

.meta-item {
  display: flex;
  align-items: center;
  gap: 6px;
}

.file-type-badge {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 4px 8px;
  background-color: #21262d;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 600;
}

.pdf-badge {
  color: #f85149;
}

.docx-badge {
  color: #58a6ff;
}

.txt-badge {
  color: #79c0ff;
}

.result-actions {
  display: flex;
  gap: 8px;
}

.action-btn {
  width: 24px;
  height: 24px;
  border: none;
  background: none;
  color: #8b949e;
  cursor: pointer;
  font-size: 14px;
  transition: color 0.2s;
  padding: 0;
  display: flex;
  align-items: center;
  justify-content: center;
}

.action-btn:hover {
  color: #58a6ff;
}
</style>
