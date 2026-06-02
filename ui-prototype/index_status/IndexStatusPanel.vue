<template>
  <div class="index-status-panel">
    <!-- 顶部头部 -->
    <div class="panel-header">
      <div class="header-left">
        <div class="header-title">
          <span class="title-icon">📚</span>
          <h1>索引状态</h1>
        </div>
        <p class="header-description">实时监控索引进度、状态和系统信息</p>
      </div>
      <div class="header-right">
        <div class="status-badge" :class="`status-${indexStatus.toLowerCase()}`">
          <span class="status-dot"></span>
          {{ indexStatus }}
        </div>
        <div class="last-update">
          <span class="update-icon">🕐</span>
          最后更新：{{ lastUpdateTime }}
        </div>
        <button class="refresh-btn" @click="refreshStatus" title="刷新状态">
          🔄
        </button>
      </div>
    </div>

    <!-- 主容器 -->
    <div class="panel-content">
      <!-- 左侧面板 -->
      <div class="left-panel">
        <!-- 索引控制 -->
        <div class="control-card">
          <div class="card-header">
            <span class="card-icon">⚙️</span>
            <h2>索引控制</h2>
          </div>
          <p class="card-description">
            控制索引任务的执行状态，您可以暂停、恢复或重新开始索引过程。
          </p>
          <div class="control-buttons">
            <button class="btn btn-secondary" @click="pauseIndex">
              <span class="btn-icon">⏸</span>
              暂停索引
            </button>
            <button class="btn btn-secondary" @click="resumeIndex">
              <span class="btn-icon">▶️</span>
              恢复索引
            </button>
            <button class="btn btn-primary" @click="reindexAll">
              <span class="btn-icon">🔄</span>
              重新索引
            </button>
          </div>
        </div>

        <!-- 统计信息 -->
        <div class="stats-card">
          <div class="card-header">
            <span class="card-icon">📊</span>
            <h2>统计信息</h2>
          </div>
          <div class="stats-grid">
            <div class="stat-item">
              <div class="stat-icon indexed">📄</div>
              <div class="stat-content">
                <div class="stat-label">已索引文件</div>
                <div class="stat-value">{{ statistics.indexedFiles }}</div>
                <div class="stat-desc">成功索引的文件</div>
              </div>
            </div>
            <div class="stat-item">
              <div class="stat-icon">📁</div>
              <div class="stat-content">
                <div class="stat-label">已索引文件数</div>
                <div class="stat-value">{{ statistics.indexedCount }}</div>
                <div class="stat-desc">已处理的文件总数</div>
              </div>
            </div>
            <div class="stat-item">
              <div class="stat-icon error">⚠️</div>
              <div class="stat-content">
                <div class="stat-label">错误数</div>
                <div class="stat-value">{{ statistics.errorCount }}</div>
                <div class="stat-desc">索引过程中错误总数</div>
              </div>
            </div>
            <div class="stat-item">
              <div class="stat-icon pending">📋</div>
              <div class="stat-content">
                <div class="stat-label">待处理文件数</div>
                <div class="stat-value">{{ statistics.pendingFiles }}</div>
                <div class="stat-desc">待处理的文件数量</div>
              </div>
            </div>
          </div>
        </div>

        <!-- 索引进度 -->
        <div class="progress-card">
          <div class="card-header">
            <span class="card-icon">⏱️</span>
            <h2>索引进度</h2>
          </div>

          <!-- 进度条 -->
          <div class="progress-section">
            <div class="progress-label">
              <span>总体进度</span>
              <span class="progress-percentage">{{ progress.overall }}%</span>
            </div>
            <div class="progress-bar">
              <div class="progress-fill" :style="{ width: progress.overall + '%' }"></div>
            </div>
            <div class="progress-current">
              正在索引文件：<span class="file-name">{{ progress.currentFile }}</span>
            </div>
          </div>

          <!-- 时间信息 -->
          <div class="time-info">
            <div class="time-item">
              <span class="time-icon">⏰</span>
              <div>
                <div class="time-label">本次索引耗时</div>
                <div class="time-value">{{ progress.duration }}</div>
              </div>
            </div>
            <div class="time-item">
              <span class="time-icon">📅</span>
              <div>
                <div class="time-label">开始时间</div>
                <div class="time-value">{{ progress.startTime }}</div>
              </div>
            </div>
          </div>

          <!-- 状态统计 -->
          <div class="status-stats">
            <div class="status-stat success">
              <span class="stat-icon">✓</span>
              <div>
                <div class="stat-label">成功</div>
                <div class="stat-value">{{ progress.success }}</div>
              </div>
            </div>
            <div class="status-stat error">
              <span class="stat-icon">✕</span>
              <div>
                <div class="stat-label">失败</div>
                <div class="stat-value">{{ progress.failed }}</div>
              </div>
            </div>
            <div class="status-stat skipped">
              <span class="stat-icon">⊘</span>
              <div>
                <div class="stat-label">跳过</div>
                <div class="stat-value">{{ progress.skipped }}</div>
              </div>
            </div>
          </div>
        </div>

        <!-- 错误摘要 -->
        <div class="error-card">
          <div class="card-header">
            <span class="card-icon">🔴</span>
            <h2>错误摘要</h2>
          </div>

          <!-- 错误圆形图 -->
          <div class="error-circle-container">
            <svg class="error-circle" viewBox="0 0 100 100">
              <circle cx="50" cy="50" r="45" class="circle-bg"></circle>
              <circle cx="50" cy="50" r="45" class="circle-progress" 
                :style="{ strokeDashoffset: 282.7 - (282.7 * statistics.errorCount / 21) }"></circle>
            </svg>
            <div class="error-text">
              <div class="error-count">{{ statistics.errorCount }}</div>
              <div class="error-label">总错误数</div>
            </div>
          </div>

          <!-- 错误类型 -->
          <div class="error-types">
            <div class="error-type-header">错误类型</div>
            <div class="error-type-item" v-for="(error, index) in errorTypes" :key="index">
              <div class="error-type-name">{{ error.name }}</div>
              <div class="error-type-bar">
                <div class="error-bar-fill" :style="{ width: error.percentage + '%' }"></div>
              </div>
              <div class="error-type-count">{{ error.count }} ({{ error.percentage }}%)</div>
            </div>
          </div>

          <!-- 环境信息 -->
          <div class="environment-info">
            <div class="env-item">
              <span class="env-label">Python 版本</span>
              <span class="env-value">{{ environment.pythonVersion }}</span>
            </div>
            <div class="env-item">
              <span class="env-label">超时设置</span>
              <span class="env-value">{{ environment.timeout }}</span>
            </div>
          </div>
        </div>
      </div>

      <!-- 右侧面板 -->
      <div class="right-panel">
        <!-- 索引信息 -->
        <div class="info-card">
          <div class="card-header">
            <span class="card-icon">ℹ️</span>
            <h2>索引信息</h2>
            <span class="file-count">{{ indexedFiles.length }} 个文件</span>
          </div>

          <!-- 文件列表 -->
          <div class="file-list-header">
            <label class="checkbox-label">
              <input type="checkbox" v-model="selectAll" @change="toggleSelectAll" />
              <span class="checkbox-text">文件名</span>
            </label>
            <span class="file-status-header">状态</span>
            <span class="file-size-header">大小</span>
          </div>

          <div class="file-list">
            <div class="file-item" v-for="file in indexedFiles" :key="file.id">
              <label class="checkbox-label">
                <input type="checkbox" v-model="file.selected" />
                <span class="file-icon">{{ getFileIcon(file.type) }}</span>
                <span class="file-name">{{ file.name }}</span>
              </label>
              <div class="file-status" :class="`status-${file.status.toLowerCase()}`">
                <span class="status-dot"></span>
                {{ file.status }}
              </div>
              <span class="file-size">{{ file.size }}</span>
            </div>
          </div>

          <div class="file-list-footer">
            已选择 {{ selectedCount }} / {{ indexedFiles.length }} 个文件
            <button class="reindex-btn" @click="reindexSelected">刷新列表</button>
          </div>
        </div>

        <!-- 最新异常 -->
        <div class="exception-card">
          <div class="card-header">
            <span class="card-icon">⚠️</span>
            <h2>最新异常</h2>
            <span class="exception-count">{{ exceptions.length }}</span>
          </div>

          <div class="exception-content" v-if="exceptions.length > 0">
            <div class="exception-item">
              <div class="exception-header">
                <span class="exception-file">错误文件：{{ exceptions[0].file }}</span>
                <span class="exception-type">{{ exceptions[0].type }}</span>
              </div>
              <div class="exception-time">
                异常时间：{{ exceptions[0].time }}
              </div>
              <div class="exception-message">
                {{ exceptions[0].message }}
              </div>
              <div class="exception-traceback">
                <div class="traceback-label">堆栈信息：</div>
                <pre class="traceback-code">{{ exceptions[0].traceback }}</pre>
              </div>
            </div>
          </div>

          <div class="exception-empty" v-else>
            <span class="empty-icon">✓</span>
            <p>暂无异常信息</p>
          </div>

          <div class="exception-footer">
            <button class="view-all-btn" @click="viewAllExceptions">
              查看所有异常 →
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'

interface IndexedFile {
  id: number
  name: string
  type: string
  status: 'success' | 'indexing' | 'failed'
  size: string
  selected?: boolean
}

interface Exception {
  file: string
  type: string
  time: string
  message: string
  traceback: string
}

// 状态数据
const indexStatus = ref('Python 索引中')
const lastUpdateTime = ref('2025-05-24 15:32:45')

const statistics = ref({
  indexedFiles: 3,
  indexedCount: 3,
  errorCount: 21,
  pendingFiles: 0
})

const progress = ref({
  overall: 60,
  currentFile: 'pdf_test.pdf',
  duration: '00:02:48',
  startTime: '2025-05-24 15:29:57',
  success: 2,
  failed: 0,
  skipped: 0
})

const errorTypes = ref([
  { name: '解析错误 (Parse Error)', count: 12, percentage: 57.1 },
  { name: '超时错误 (Timeout Error)', count: 6, percentage: 28.6 },
  { name: '编码错误 (Encoding Error)', count: 3, percentage: 14.3 },
  { name: '其他错误 (Other Error)', count: 0, percentage: 0 }
])

const environment = ref({
  pythonVersion: '3.11.9',
  timeout: '300 秒'
})

const indexedFiles = ref<IndexedFile[]>([
  {
    id: 1,
    name: 'pdf_test.pdf',
    type: 'pdf',
    status: 'success',
    size: '1.24 MB',
    selected: true
  },
  {
    id: 2,
    name: '电话导入.xlsx',
    type: 'xlsx',
    status: 'indexing',
    size: '18.45 KB',
    selected: true
  }
])

const exceptions = ref<Exception[]>([
  {
    file: '电话导入.xlsx',
    type: '解析错误 (Parse Error)',
    time: '2025-05-24 15:32:12',
    message: 'ValueError: Unable to parse content from file \'电话导入.xlsx\'. The file format may be unsupported or the file may be corrupted.',
    traceback: 'at parse_excel() in parser.py:128\nat process_file() in indexer.py:256\nat run_index() in indexer.py:89'
  }
])

const selectAll = ref(false)

// 计算属性
const selectedCount = computed(() => {
  return indexedFiles.value.filter(f => f.selected).length
})

// 方法
const refreshStatus = () => {
  console.log('刷新状态')
  lastUpdateTime.value = new Date().toLocaleString('zh-CN')
}

const pauseIndex = () => {
  console.log('暂停索引')
  indexStatus.value = 'Python 索引已暂停'
}

const resumeIndex = () => {
  console.log('恢复索引')
  indexStatus.value = 'Python 索引中'
}

const reindexAll = () => {
  console.log('重新索引所有文件')
  progress.value.overall = 0
}

const toggleSelectAll = () => {
  indexedFiles.value.forEach(file => {
    file.selected = selectAll.value
  })
}

const reindexSelected = () => {
  console.log('刷新列表')
}

const viewAllExceptions = () => {
  console.log('查看所有异常')
}

const getFileIcon = (type: string): string => {
  const icons: Record<string, string> = {
    pdf: '📄',
    xlsx: '📊',
    docx: '📝',
    txt: '📋'
  }
  return icons[type.toLowerCase()] || '📄'
}
</script>

<style scoped>
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

.index-status-panel {
  background-color: #0d1117;
  color: #e6edf3;
  padding: 24px;
  border-radius: 8px;
  min-height: 100vh;
}

/* 头部 */
.panel-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 32px;
  padding-bottom: 20px;
  border-bottom: 1px solid #30363d;
}

.header-left {
  flex: 1;
}

.header-title {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 8px;
}

.title-icon {
  font-size: 28px;
}

.header-title h1 {
  font-size: 28px;
  font-weight: 600;
  color: #e6edf3;
}

.header-description {
  font-size: 13px;
  color: #8b949e;
}

.header-right {
  display: flex;
  align-items: center;
  gap: 20px;
}

.status-badge {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border-radius: 6px;
  font-size: 13px;
  font-weight: 600;
}

.status-badge.status-python\ 索引中 {
  background-color: rgba(76, 175, 80, 0.2);
  color: #4caf50;
  border: 1px solid #4caf50;
}

.status-dot {
  display: inline-block;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background-color: #4caf50;
  animation: pulse 2s infinite;
}

@keyframes pulse {
  0%, 100% {
    opacity: 1;
  }
  50% {
    opacity: 0.5;
  }
}

.last-update {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: #8b949e;
}

.update-icon {
  font-size: 14px;
}

.refresh-btn {
  width: 36px;
  height: 36px;
  border: 1px solid #30363d;
  background-color: #161b22;
  border-radius: 6px;
  color: #8b949e;
  cursor: pointer;
  font-size: 16px;
  transition: all 0.2s;
}

.refresh-btn:hover {
  background-color: #21262d;
  border-color: #58a6ff;
  color: #58a6ff;
}

/* 主容器 */
.panel-content {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 24px;
}

/* 卡片通用样式 */
.control-card,
.stats-card,
.progress-card,
.error-card,
.info-card,
.exception-card {
  background-color: #161b22;
  border: 1px solid #30363d;
  border-radius: 8px;
  padding: 20px;
  margin-bottom: 24px;
}

.card-header {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 16px;
  padding-bottom: 12px;
  border-bottom: 1px solid #30363d;
}

.card-icon {
  font-size: 20px;
}

.card-header h2 {
  font-size: 16px;
  font-weight: 600;
  color: #e6edf3;
  flex: 1;
}

.file-count,
.exception-count {
  font-size: 12px;
  color: #8b949e;
  background-color: #0d1117;
  padding: 4px 8px;
  border-radius: 4px;
}

.card-description {
  font-size: 13px;
  color: #8b949e;
  margin-bottom: 16px;
  line-height: 1.5;
}

/* 控制按钮 */
.control-buttons {
  display: flex;
  gap: 12px;
  flex-wrap: wrap;
}

.btn {
  padding: 10px 16px;
  border-radius: 6px;
  border: 1px solid #30363d;
  background-color: #0d1117;
  color: #e6edf3;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 6px;
  transition: all 0.2s;
}

.btn:hover {
  background-color: #21262d;
  border-color: #58a6ff;
  color: #58a6ff;
}

.btn-primary {
  background-color: #1f6feb;
  border-color: #1f6feb;
  color: white;
}

.btn-primary:hover {
  background-color: #388bfd;
  border-color: #388bfd;
}

.btn-icon {
  font-size: 14px;
}

/* 统计信息 */
.stats-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 16px;
}

.stat-item {
  display: flex;
  gap: 12px;
  padding: 12px;
  background-color: #0d1117;
  border-radius: 6px;
  border: 1px solid #30363d;
}

.stat-icon {
  width: 40px;
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 6px;
  font-size: 20px;
  background-color: #21262d;
}

.stat-icon.indexed {
  background-color: rgba(88, 166, 255, 0.2);
}

.stat-icon.error {
  background-color: rgba(248, 81, 73, 0.2);
}

.stat-icon.pending {
  background-color: rgba(255, 200, 87, 0.2);
}

.stat-content {
  flex: 1;
}

.stat-label {
  font-size: 12px;
  color: #8b949e;
  margin-bottom: 4px;
}

.stat-value {
  font-size: 24px;
  font-weight: 600;
  color: #e6edf3;
  margin-bottom: 4px;
}

.stat-desc {
  font-size: 11px;
  color: #6e7681;
}

/* 进度条 */
.progress-section {
  margin-bottom: 20px;
}

.progress-label {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 8px;
  font-size: 13px;
  color: #e6edf3;
}

.progress-percentage {
  font-weight: 600;
  color: #58a6ff;
}

.progress-bar {
  width: 100%;
  height: 8px;
  background-color: #0d1117;
  border-radius: 4px;
  overflow: hidden;
  margin-bottom: 8px;
}

.progress-fill {
  height: 100%;
  background: linear-gradient(90deg, #1f6feb, #58a6ff);
  border-radius: 4px;
  transition: width 0.3s ease;
}

.progress-current {
  font-size: 12px;
  color: #8b949e;
}

.file-name {
  color: #58a6ff;
  font-weight: 500;
}

/* 时间信息 */
.time-info {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
  margin-bottom: 20px;
  padding: 12px;
  background-color: #0d1117;
  border-radius: 6px;
}

.time-item {
  display: flex;
  gap: 8px;
  align-items: flex-start;
}

.time-icon {
  font-size: 14px;
  margin-top: 2px;
}

.time-label {
  font-size: 11px;
  color: #8b949e;
}

.time-value {
  font-size: 13px;
  color: #e6edf3;
  font-weight: 500;
  margin-top: 2px;
}

/* 状态统计 */
.status-stats {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 12px;
}

.status-stat {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px;
  background-color: #0d1117;
  border-radius: 6px;
  border-left: 3px solid;
}

.status-stat.success {
  border-left-color: #4caf50;
}

.status-stat.error {
  border-left-color: #f85149;
}

.status-stat.skipped {
  border-left-color: #ffc857;
}

.status-stat .stat-icon {
  width: 24px;
  height: 24px;
  font-size: 12px;
  background: none;
}

.status-stat.success .stat-icon {
  color: #4caf50;
}

.status-stat.error .stat-icon {
  color: #f85149;
}

.status-stat.skipped .stat-icon {
  color: #ffc857;
}

.status-stat .stat-label {
  font-size: 11px;
  color: #8b949e;
}

.status-stat .stat-value {
  font-size: 18px;
  color: #e6edf3;
  margin: 0;
}

/* 错误圆形图 */
.error-circle-container {
  position: relative;
  width: 120px;
  height: 120px;
  margin: 0 auto 20px;
}

.error-circle {
  width: 100%;
  height: 100%;
}

.circle-bg {
  fill: none;
  stroke: #21262d;
  stroke-width: 8;
}

.circle-progress {
  fill: none;
  stroke: #f85149;
  stroke-width: 8;
  stroke-dasharray: 282.7;
  stroke-dashoffset: 282.7;
  transform: rotate(-90deg);
  transform-origin: 50% 50%;
  transition: stroke-dashoffset 0.3s ease;
}

.error-text {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  text-align: center;
}

.error-count {
  font-size: 28px;
  font-weight: 600;
  color: #f85149;
}

.error-label {
  font-size: 11px;
  color: #8b949e;
}

/* 错误类型 */
.error-types {
  margin-bottom: 20px;
}

.error-type-header {
  font-size: 12px;
  font-weight: 600;
  color: #e6edf3;
  margin-bottom: 12px;
}

.error-type-item {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}

.error-type-name {
  font-size: 12px;
  color: #8b949e;
  min-width: 150px;
}

.error-type-bar {
  flex: 1;
  height: 6px;
  background-color: #0d1117;
  border-radius: 3px;
  overflow: hidden;
}

.error-bar-fill {
  height: 100%;
  background-color: #f85149;
  transition: width 0.3s ease;
}

.error-type-count {
  font-size: 11px;
  color: #8b949e;
  min-width: 60px;
  text-align: right;
}

/* 环境信息 */
.environment-info {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
  padding-top: 12px;
  border-top: 1px solid #30363d;
}

.env-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.env-label {
  font-size: 12px;
  color: #8b949e;
}

.env-value {
  font-size: 12px;
  color: #e6edf3;
  font-weight: 500;
}

/* 文件列表 */
.file-list-header {
  display: grid;
  grid-template-columns: 1fr auto auto;
  gap: 12px;
  padding: 12px;
  background-color: #0d1117;
  border-radius: 6px;
  margin-bottom: 12px;
  font-size: 12px;
  font-weight: 600;
  color: #8b949e;
  border-bottom: 1px solid #30363d;
}

.checkbox-label {
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  user-select: none;
}

.checkbox-label input[type="checkbox"] {
  width: 16px;
  height: 16px;
  cursor: pointer;
  accent-color: #58a6ff;
}

.checkbox-text {
  color: #e6edf3;
}

.file-status-header,
.file-size-header {
  text-align: right;
}

.file-list {
  max-height: 300px;
  overflow-y: auto;
}

.file-item {
  display: grid;
  grid-template-columns: 1fr auto auto;
  gap: 12px;
  align-items: center;
  padding: 12px;
  border-bottom: 1px solid #30363d;
  transition: background-color 0.2s;
}

.file-item:hover {
  background-color: #0d1117;
}

.file-icon {
  font-size: 14px;
}

.file-name {
  color: #e6edf3;
  font-size: 13px;
}

.file-status {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  padding: 4px 8px;
  border-radius: 4px;
  background-color: #0d1117;
}

.file-status.status-success {
  color: #4caf50;
}

.file-status.status-indexing {
  color: #58a6ff;
}

.file-status.status-failed {
  color: #f85149;
}

.status-dot {
  display: inline-block;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background-color: currentColor;
}

.file-size {
  font-size: 12px;
  color: #8b949e;
  text-align: right;
}

.file-list-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px;
  background-color: #0d1117;
  border-radius: 6px;
  font-size: 12px;
  color: #8b949e;
  margin-top: 12px;
}

.reindex-btn {
  padding: 6px 12px;
  background-color: #161b22;
  border: 1px solid #30363d;
  border-radius: 4px;
  color: #58a6ff;
  font-size: 12px;
  cursor: pointer;
  transition: all 0.2s;
}

.reindex-btn:hover {
  background-color: #21262d;
  border-color: #58a6ff;
}

/* 异常信息 */
.exception-content {
  margin-bottom: 16px;
}

.exception-item {
  background-color: #0d1117;
  border-left: 3px solid #f85149;
  padding: 12px;
  border-radius: 4px;
}

.exception-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 8px;
  flex-wrap: wrap;
  gap: 8px;
}

.exception-file {
  font-size: 12px;
  color: #e6edf3;
  font-weight: 500;
}

.exception-type {
  font-size: 11px;
  color: #f85149;
  background-color: rgba(248, 81, 73, 0.2);
  padding: 4px 8px;
  border-radius: 4px;
}

.exception-time {
  font-size: 11px;
  color: #8b949e;
  margin-bottom: 8px;
}

.exception-message {
  font-size: 12px;
  color: #c9d1d9;
  margin-bottom: 12px;
  line-height: 1.5;
}

.exception-traceback {
  margin-top: 12px;
}

.traceback-label {
  font-size: 11px;
  color: #8b949e;
  margin-bottom: 6px;
}

.traceback-code {
  background-color: #0d1117;
  border: 1px solid #30363d;
  border-radius: 4px;
  padding: 8px;
  font-size: 11px;
  color: #8b949e;
  font-family: 'Monaco', 'Menlo', monospace;
  overflow-x: auto;
  margin: 0;
}

.exception-empty {
  text-align: center;
  padding: 32px 12px;
  color: #8b949e;
}

.empty-icon {
  font-size: 32px;
  display: block;
  margin-bottom: 8px;
}

.exception-empty p {
  font-size: 13px;
}

.exception-footer {
  padding-top: 12px;
  border-top: 1px solid #30363d;
}

.view-all-btn {
  width: 100%;
  padding: 10px;
  background-color: #161b22;
  border: 1px solid #30363d;
  border-radius: 6px;
  color: #58a6ff;
  font-size: 13px;
  cursor: pointer;
  transition: all 0.2s;
}

.view-all-btn:hover {
  background-color: #21262d;
  border-color: #58a6ff;
}

/* 响应式设计 */
@media (max-width: 1400px) {
  .panel-content {
    grid-template-columns: 1fr;
  }

  .stats-grid {
    grid-template-columns: repeat(2, 1fr);
  }
}

@media (max-width: 768px) {
  .panel-header {
    flex-direction: column;
    align-items: flex-start;
    gap: 16px;
  }

  .header-right {
    width: 100%;
    flex-wrap: wrap;
  }

  .control-buttons {
    flex-direction: column;
  }

  .btn {
    width: 100%;
    justify-content: center;
  }

  .stats-grid {
    grid-template-columns: 1fr;
  }

  .time-info {
    grid-template-columns: 1fr;
  }

  .status-stats {
    grid-template-columns: 1fr;
  }

  .file-list-header,
  .file-item {
    grid-template-columns: 1fr;
  }
}
</style>
