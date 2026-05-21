<script setup lang="ts">
import { onMounted, ref } from "vue";
import { AlertCircle, Loader2 } from "lucide-vue-next";
import DocMindBadge from "../components/docmind/DocMindBadge.vue";
import { docmindApi } from "../services/docmindApi";
import type { IndexStatusView } from "../types/docmind";

const status = ref<IndexStatusView | null>(null);
const loading = ref(false);
const errorMessage = ref("");

const loadStatus = async () => {
  loading.value = true;
  errorMessage.value = "";

  try {
    status.value = await docmindApi.getIndexStatus();
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : "状态加载失败";
  } finally {
    loading.value = false;
  }
};

onMounted(loadStatus);
</script>

<template>
  <div class="h-full overflow-y-auto p-8">
    <div class="mb-7">
      <h1 class="text-2xl font-semibold tracking-tight text-slate-950">索引状态</h1>
      <p class="mt-1 text-sm text-slate-500">查看当前索引进度、失败文件和可重新处理的项目。</p>
    </div>

    <div class="mb-6 grid grid-cols-4 gap-4">
      <div v-for="card in [
        { label: '已扫描文档', value: status?.scanned_docs ?? 0 },
        { label: '已索引文档', value: status?.indexed_docs ?? 0 },
        { label: '段落块', value: status?.indexed_chunks ?? 0 },
        { label: '失败文件', value: status?.failed_files ?? 0 },
      ]" :key="card.label" class="rounded-3xl border border-slate-200 bg-white p-5 shadow-sm">
        <div class="text-xs text-slate-500">{{ card.label }}</div>
        <div class="mt-2 text-2xl font-semibold text-slate-950">{{ card.value }}</div>
      </div>
    </div>

    <div v-if="errorMessage" class="mb-4 rounded-2xl border border-red-100 bg-red-50 px-4 py-3 text-sm text-red-700">
      {{ errorMessage }}
    </div>

    <div v-if="loading" class="rounded-3xl border border-dashed border-slate-300 bg-white p-6 text-sm text-slate-500">
      正在读取索引状态...
    </div>

    <div v-else class="rounded-3xl border border-slate-200 bg-white p-6 shadow-sm">
      <div class="mb-4 flex items-center justify-between">
        <div>
          <div class="text-sm font-semibold text-slate-900">当前任务</div>
          <div class="mt-1 text-xs text-slate-500">{{ status?.current_task?.label ?? "暂无任务" }}</div>
        </div>
        <DocMindBadge tone="warning">
          <Loader2 class="mr-1 animate-spin" :size="13" />
          处理中
        </DocMindBadge>
      </div>
      <div class="h-2 rounded-full bg-slate-100">
        <div
          class="h-2 rounded-full bg-slate-900"
          :style="{ width: `${status?.current_task?.progress ?? 0}%` }"
        />
      </div>
      <div class="mt-2 text-xs text-slate-500">
        {{ status?.current_task?.scanned ?? 0 }} / {{ status?.current_task?.total ?? 0 }} 个文件
      </div>
      <div class="mt-2 text-xs text-slate-500">{{ status?.current_task?.details ?? "" }}</div>
    </div>

    <div class="mt-6 rounded-3xl border border-slate-200 bg-white p-6 shadow-sm">
      <div class="mb-4 flex items-center gap-2 text-sm font-semibold text-slate-900">
        <AlertCircle :size="18" class="text-amber-500" />
        解析失败
      </div>
      <div class="space-y-3">
        <div v-for="file in (status?.failed_items ?? [])" :key="file.file" class="flex items-center justify-between rounded-2xl bg-slate-50 px-4 py-3">
          <div>
            <div class="text-sm font-medium text-slate-800">{{ file.file }}</div>
            <div class="mt-1 text-xs text-slate-500">{{ file.reason }}</div>
          </div>
          <button class="text-xs font-medium text-slate-600">重新处理</button>
        </div>
      </div>
    </div>
  </div>
</template>
