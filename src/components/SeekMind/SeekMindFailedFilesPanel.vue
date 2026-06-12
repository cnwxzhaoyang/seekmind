<script setup lang="ts">
/**
 * @author MorningSun
 * @CreatedDate 2026/06/12
 * @Description 索引失败文件列表面板，展示失败文件、原因并提供重试与复制能力。
 */
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { AlertTriangle, Copy, RotateCcw } from "lucide-vue-next";
import type { FailedFileView } from "../../types/SeekMind";

const props = defineProps<{
  items: FailedFileView[];
  retryingTarget: string | null;
}>();

const emit = defineEmits<{
  retry: [path: string];
  copyPath: [path: string];
  copyReason: [reason: string];
}>();

const { t } = useI18n();

const sortedItems = computed(() =>
  [...props.items].sort((left, right) =>
    (right.last_failed_at || "").localeCompare(left.last_failed_at || ""),
  ),
);

const formatTimestamp = (value: string) => {
  if (!value) {
    return "--";
  }

  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }

  return parsed.toLocaleString();
};

const categoryLabel = (item: FailedFileView) =>
  item.category || item.code || t("page.status.exception.noException");

const onRetry = (path: string) => emit("retry", path);
const onCopyPath = (path: string) => emit("copyPath", path);
const onCopyReason = (reason: string) => emit("copyReason", reason);
</script>

<template>
  <div class="card">
    <div class="card-header">
      <span class="card-icon seekmind-page-header-icon"><AlertTriangle :size="18" /></span>
      <div class="card-head-copy">
        <h2>{{ t("page.status.section.failedFiles") }}</h2>
      </div>
      <span class="failed-files-count">{{ sortedItems.length }}</span>
    </div>

    <div v-if="sortedItems.length === 0" class="failed-files-empty">
      {{ t("page.status.failed.noFailed") }}
    </div>

    <div v-else class="failed-files-list">
      <div
        v-for="item in sortedItems"
        :key="`${item.file}-${item.last_failed_at}`"
        class="failed-file-item"
      >
        <div class="failed-file-main">
          <div class="failed-file-topline">
            <div class="failed-file-name" :title="item.file">
              {{ item.file.split('/').pop() || item.file }}
            </div>
            <span class="failed-file-category" :title="categoryLabel(item)">
              {{ categoryLabel(item) }}
            </span>
          </div>

          <div class="failed-file-path" :title="item.file">
            {{ item.file }}
          </div>

          <div class="failed-file-reason" :title="item.reason">
            {{ item.reason }}
          </div>

          <div class="failed-file-meta">
            <span>{{ t("page.status.failed.retryCount", { count: item.retry_count }) }}</span>
            <span>{{ t("page.status.failed.lastFailedAt") }} {{ formatTimestamp(item.last_failed_at) }}</span>
          </div>
        </div>

        <div class="failed-file-actions">
          <button
            type="button"
            class="failed-file-action"
            :title="t('page.status.action.copyPath')"
            @click="onCopyPath(item.file)"
          >
            <Copy :size="14" />
          </button>
          <button
            type="button"
            class="failed-file-action"
            :title="t('page.status.failed.copyReason')"
            @click="onCopyReason(item.reason)"
          >
            <AlertTriangle :size="14" />
          </button>
          <button
            type="button"
            class="failed-file-action failed-file-action--primary"
            :disabled="retryingTarget === item.file"
            :title="t('page.status.failed.retryFile')"
            @click="onRetry(item.file)"
          >
            <RotateCcw :size="14" />
            <span>
              {{ retryingTarget === item.file ? t("page.status.failed.retrying") : t("page.status.failed.retryFile") }}
            </span>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.failed-files-count {
  margin-left: auto;
  min-width: 30px;
  height: 30px;
  padding: 0 10px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 999px;
  background: rgba(148, 163, 184, 0.14);
  color: var(--color-text-secondary);
  font-size: 12px;
  font-weight: 600;
}

.failed-files-empty {
  padding: 18px;
  border-radius: 18px;
  color: var(--color-text-secondary);
  background: rgba(255, 255, 255, 0.72);
  border: 1px solid rgba(148, 163, 184, 0.18);
}

.failed-files-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
  max-height: 420px;
  overflow-y: auto;
  padding-right: 2px;
}

.failed-file-item {
  display: flex;
  gap: 12px;
  justify-content: space-between;
  align-items: flex-start;
  padding: 14px 16px;
  border-radius: 18px;
  background: rgba(255, 255, 255, 0.72);
  border: 1px solid rgba(148, 163, 184, 0.18);
}

.failed-file-main {
  min-width: 0;
  flex: 1;
}

.failed-file-topline {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 8px;
}

.failed-file-name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 14px;
  font-weight: 700;
  color: var(--color-text-primary);
}

.failed-file-category {
  flex: none;
  display: inline-flex;
  align-items: center;
  padding: 3px 8px;
  border-radius: 999px;
  background: rgba(185, 28, 28, 0.08);
  color: var(--color-danger);
  font-size: 11px;
  font-weight: 600;
}

.failed-file-path,
.failed-file-reason {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.failed-file-path {
  margin-bottom: 6px;
  color: var(--color-text-secondary);
  font-size: 12px;
}

.failed-file-reason {
  margin-bottom: 8px;
  color: var(--color-text-primary);
  font-size: 13px;
  line-height: 1.45;
}

.failed-file-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  color: var(--color-text-secondary);
  font-size: 11px;
}

.failed-file-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: none;
}

.failed-file-action {
  height: 32px;
  min-width: 32px;
  padding: 0 10px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  border: 1px solid rgba(148, 163, 184, 0.22);
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.86);
  color: var(--color-text-secondary);
  cursor: pointer;
}

.failed-file-action--primary {
  color: var(--color-accent);
}

.failed-file-action:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

html.dark .failed-files-empty,
html.dark .failed-file-item,
html.dark .failed-file-action {
  background: rgba(13, 17, 23, 0.92);
  border-color: rgba(48, 54, 61, 0.92);
}

html.dark .failed-files-empty,
html.dark .failed-file-path,
html.dark .failed-file-meta,
html.dark .failed-file-action {
  color: var(--color-text-secondary);
}

html.dark .failed-file-action--primary {
  color: #58a6ff;
}

@media (max-width: 1200px) {
  .failed-file-item {
    flex-direction: column;
  }

  .failed-file-actions {
    width: 100%;
    justify-content: flex-end;
  }
}
</style>
