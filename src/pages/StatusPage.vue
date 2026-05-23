<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { AlertCircle, Loader2, RefreshCw, FolderOpen, Database, Cpu } from "lucide-vue-next";
import DocMindBadge from "../components/docmind/DocMindBadge.vue";
import { docmindApi, formatDocmindError } from "../services/docmindApi";
import type { FailedFileView, IndexDirView, IndexStatusView, ParserRuntimeView } from "../types/docmind";

const status = ref<IndexStatusView | null>(null);
const dirs = ref<IndexDirView[]>([]);
const parserRuntime = ref<ParserRuntimeView | null>(null);
const loading = ref(false);
const refreshing = ref(false);
const errorMessage = ref("");
let pollTimer: number | null = null;

const failedGroups = computed(() => {
  const groups = new Map<string, FailedFileView[]>();

  for (const item of status.value?.failed_items ?? []) {
    const dir = item.file.includes("/") ? item.file.slice(0, item.file.lastIndexOf("/")) : "未分类";
    const items = groups.get(dir) ?? [];
    items.push(item);
    groups.set(dir, items);
  }

  return [...groups.entries()].map(([dir, items]) => ({ dir, items }));
});

const loadStatus = async () => {
  if (!status.value) {
    loading.value = true;
  }

  try {
    status.value = await docmindApi.getIndexStatus();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, "状态加载失败");
    console.error("[DocMind] loadStatus failed", error);
  } finally {
    loading.value = false;
  }
};

const loadDirs = async () => {
  try {
    dirs.value = await docmindApi.listIndexDirs();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, "目录加载失败");
    console.error("[DocMind] loadDirs failed", error);
  }
};

const loadParserRuntime = async () => {
  try {
    parserRuntime.value = await docmindApi.getParserRuntime();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, "解析器状态加载失败");
    console.error("[DocMind] loadParserRuntime failed", error);
  }
};

const refreshDashboard = async () => {
  await Promise.all([loadStatus(), loadDirs(), loadParserRuntime()]);
};

const refreshIndex = async () => {
  refreshing.value = true;
  errorMessage.value = "";

  try {
    status.value = await docmindApi.refreshIndex();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, "重新索引失败");
    console.error("[DocMind] refreshIndex failed", error);
  } finally {
    refreshing.value = false;
    await refreshDashboard();
  }
};

const retryFailedFile = async (path: string) => {
  refreshing.value = true;
  errorMessage.value = "";

  try {
    status.value = await docmindApi.retryFailedFile(path);
  } catch (error) {
    errorMessage.value = formatDocmindError(error, "重新处理失败");
    console.error("[DocMind] retryFailedFile failed", error);
  } finally {
    refreshing.value = false;
    await refreshDashboard();
  }
};

onMounted(async () => {
  await refreshDashboard();

  pollTimer = window.setInterval(() => {
    if (!refreshing.value) {
      void refreshDashboard();
    }
  }, 2000);
});

onUnmounted(() => {
  if (pollTimer !== null) {
    window.clearInterval(pollTimer);
  }
});
</script>

<template>
  <div class="h-full overflow-y-auto p-8">
    <div class="mb-7">
      <h1 class="text-2xl font-semibold tracking-tight text-slate-950">索引状态</h1>
      <p class="mt-1 text-sm text-slate-500">查看当前索引进度、失败文件和可重新处理的项目。</p>
    </div>
    <div class="mb-6 flex items-center justify-end">
      <button
        class="inline-flex items-center gap-2 rounded-2xl border border-slate-200 bg-white px-4 py-2 text-sm font-medium text-slate-700 shadow-sm hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
        :disabled="refreshing || loading"
        @click="refreshIndex"
      >
        <RefreshCw :size="16" :class="{ 'animate-spin': refreshing }" />
        {{ refreshing ? "重建中..." : "重新索引" }}
      </button>
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

    <div class="mb-6 rounded-3xl border border-slate-200 bg-white p-5 shadow-sm">
      <div class="mb-4 flex items-center justify-between">
        <div>
          <div class="text-sm font-semibold text-slate-900">解析器状态</div>
          <div class="mt-1 text-xs text-slate-500">当前索引默认使用哪条解析链路。</div>
        </div>
        <DocMindBadge :tone="parserRuntime?.active === 'python' ? 'success' : 'warning'">
          <Cpu class="mr-1" :size="13" />
          {{ parserRuntime?.active === 'python' ? 'Python 解析' : 'Rust 回退' }}
        </DocMindBadge>
      </div>
      <div v-if="parserRuntime" class="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
        <div class="rounded-2xl bg-slate-50 px-4 py-3">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">启用 Python</div>
          <div class="mt-1 text-sm font-semibold text-slate-900">
            {{ parserRuntime.enabled ? "是" : "否" }}
          </div>
        </div>
        <div class="rounded-2xl bg-slate-50 px-4 py-3">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">Python 可用</div>
          <div class="mt-1 text-sm font-semibold text-slate-900">
            {{ parserRuntime.available ? "是" : "否" }}
          </div>
        </div>
        <div class="rounded-2xl bg-slate-50 px-4 py-3">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">Python Bin</div>
          <div class="mt-1 truncate text-sm font-medium text-slate-900">{{ parserRuntime.python_bin }}</div>
        </div>
        <div class="rounded-2xl bg-slate-50 px-4 py-3">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">超时</div>
          <div class="mt-1 text-sm font-semibold text-slate-900">{{ parserRuntime.timeout_ms }} ms</div>
        </div>
      </div>
      <div v-if="parserRuntime" class="mt-3 text-xs text-slate-500">
        Script: {{ parserRuntime.script_path }}
      </div>
    </div>

    <div class="mb-6 rounded-3xl border border-slate-200 bg-white p-5 shadow-sm">
      <div class="mb-4 flex items-center justify-between">
        <div>
          <div class="text-sm font-semibold text-slate-900">索引目录</div>
          <div class="mt-1 text-xs text-slate-500">当前本地正在管理的索引位置。</div>
        </div>
        <DocMindBadge tone="default">
          <Database class="mr-1" :size="13" />
          {{ dirs.length }} 个目录
        </DocMindBadge>
      </div>
      <div v-if="dirs.length === 0" class="rounded-2xl bg-slate-50 px-4 py-6 text-sm text-slate-500">
        当前没有可索引目录，请先到“文档目录”页添加一个目录。
      </div>
      <div v-else class="space-y-2">
        <div
          v-for="dir in dirs"
          :key="dir.path"
          class="flex items-center justify-between rounded-2xl bg-slate-50 px-4 py-3"
        >
          <div class="min-w-0">
            <div class="truncate text-sm font-medium text-slate-800">{{ dir.path }}</div>
            <div class="mt-1 text-xs text-slate-500">{{ dir.docs }} 个文档 · {{ dir.chunks.toLocaleString() }} 个段落</div>
          </div>
          <DocMindBadge :tone="dir.enabled ? 'success' : 'default'">
            {{ dir.enabled ? "已启用" : "已禁用" }}
          </DocMindBadge>
        </div>
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
        <DocMindBadge :tone="status?.current_task ? 'warning' : 'default'">
          <Loader2 v-if="status?.current_task" class="mr-1 animate-spin" :size="13" />
          {{ status?.current_task ? "处理中" : "空闲" }}
        </DocMindBadge>
      </div>
      <div class="h-2 rounded-full bg-slate-100">
        <div
          class="h-2 rounded-full bg-slate-900 transition-[width] duration-500"
          :style="{
            width: status?.current_task
              ? `${Math.max(status.current_task.progress, 8)}%`
              : '0%',
          }"
        />
      </div>

      <div v-if="status?.current_task" class="mt-4 grid gap-3 md:grid-cols-2 xl:grid-cols-4">
        <div class="rounded-2xl bg-slate-50 px-4 py-3">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">正在处理目录</div>
          <div class="mt-1 truncate text-sm font-medium text-slate-900">
            {{ status.current_task.current_dir || "暂无目录" }}
          </div>
        </div>
        <div class="rounded-2xl bg-slate-50 px-4 py-3">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">当前文件</div>
          <div class="mt-1 truncate text-sm font-medium text-slate-900">
            {{ status.current_task.current_file || "暂无文件" }}
          </div>
        </div>
        <div class="rounded-2xl bg-slate-50 px-4 py-3">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">累计成功</div>
          <div class="mt-1 text-sm font-semibold text-emerald-700">{{ status.current_task.succeeded }}</div>
        </div>
        <div class="rounded-2xl bg-slate-50 px-4 py-3">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">累计失败</div>
          <div class="mt-1 text-sm font-semibold text-rose-700">{{ status.current_task.failed }}</div>
        </div>
      </div>

      <div class="mt-4 flex flex-wrap items-center gap-3 text-xs text-slate-500">
        <span>{{ status?.current_task?.scanned ?? 0 }} / {{ status?.current_task?.total ?? 0 }} 个文件</span>
        <span>·</span>
        <span>{{ status?.current_task?.progress ?? 0 }}%</span>
        <span>·</span>
        <span>{{ status?.current_task?.details ?? "" }}</span>
      </div>
    </div>

    <div class="mt-6 rounded-3xl border border-slate-200 bg-white p-6 shadow-sm">
      <div class="mb-4 flex items-center gap-2 text-sm font-semibold text-slate-900">
        <AlertCircle :size="18" class="text-amber-500" />
        解析失败
      </div>
      <div v-if="failedGroups.length === 0" class="rounded-2xl bg-slate-50 px-4 py-6 text-sm text-slate-500">
        当前没有失败项
      </div>
      <div v-else class="space-y-5">
        <div v-for="group in failedGroups" :key="group.dir" class="rounded-3xl border border-slate-100 bg-slate-50 p-4">
          <div class="mb-3 flex items-center gap-2 text-xs font-semibold uppercase tracking-wide text-slate-500">
            <FolderOpen :size="14" />
            {{ group.dir }}
          </div>
          <div class="space-y-3">
            <div v-for="file in group.items" :key="file.file" class="flex items-center justify-between rounded-2xl bg-white px-4 py-3 shadow-sm">
              <div>
                <div class="text-sm font-medium text-slate-800">{{ file.file }}</div>
                <div class="mt-1 text-xs text-slate-500">{{ file.reason }}</div>
              </div>
              <button
                class="text-xs font-medium text-slate-600"
                @click="retryFailedFile(file.file)"
              >
                重新处理
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
