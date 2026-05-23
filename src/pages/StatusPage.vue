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
const retryingTarget = ref<string | null>(null);
const errorMessage = ref("");
const dashboardRefreshing = ref(false);
const actionState = ref<"pausing" | "resuming" | null>(null);
let pollTimer: number | null = null;

const failedGroups = computed(() => {
  const groups = new Map<string, FailedFileView[]>();

  for (const item of status.value?.failed_items ?? []) {
    const code = item.code || "unknown";
    const items = groups.get(code) ?? [];
    items.push(item);
    groups.set(code, items);
  }

  return [...groups.entries()].map(([code, items]) => ({
    code,
    category: items[0]?.category || "未知",
    items,
  }));
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
  if (dashboardRefreshing.value) {
    return;
  }

  dashboardRefreshing.value = true;
  try {
    await loadStatus();
    await loadDirs();
    await loadParserRuntime();
  } finally {
    dashboardRefreshing.value = false;
  }
};

const stopPolling = () => {
  if (pollTimer !== null) {
    window.clearTimeout(pollTimer);
    pollTimer = null;
  }
};

const scheduleNextRefresh = () => {
  stopPolling();
  if (status.value?.current_task && status.value.current_task.state !== "paused") {
    pollTimer = window.setTimeout(async () => {
      await refreshDashboard();
      if (status.value?.current_task && status.value.current_task.state !== "paused") {
        scheduleNextRefresh();
      } else {
        await refreshDashboard();
      }
    }, 2500);
  }
};

const syncDashboardState = async () => {
  await refreshDashboard();
  scheduleNextRefresh();
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
    await syncDashboardState();
  }
};

const pauseIndexing = async () => {
    refreshing.value = true;
    actionState.value = "pausing";
    errorMessage.value = "";

    try {
      status.value = await docmindApi.pauseIndexing();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, "暂停索引失败");
    console.error("[DocMind] pauseIndexing failed", error);
    } finally {
      refreshing.value = false;
      actionState.value = null;
      await syncDashboardState();
    }
  };

const resumeIndexing = async () => {
    refreshing.value = true;
    actionState.value = "resuming";
    errorMessage.value = "";

    try {
      status.value = await docmindApi.resumeIndexing();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, "恢复索引失败");
    console.error("[DocMind] resumeIndexing failed", error);
    } finally {
      refreshing.value = false;
      actionState.value = null;
      await syncDashboardState();
    }
  };

const taskDisplayState = computed(() => {
  const task = status.value?.current_task;
  if (!task) {
    return { label: "空闲", tone: "default" as const, spinning: false };
  }

  if (actionState.value === "pausing") {
    return { label: "暂停中", tone: "warning" as const, spinning: true };
  }
  if (actionState.value === "resuming") {
    return { label: "恢复中", tone: "warning" as const, spinning: true };
  }
  if (task.state === "paused") {
    return { label: "已暂停", tone: "default" as const, spinning: false };
  }
  if (task.state === "running") {
    return { label: "运行中", tone: "warning" as const, spinning: true };
  }

  return { label: task.state || "处理中", tone: "warning" as const, spinning: true };
});

const retryFailedFile = async (path: string) => {
  retryingTarget.value = path;
  errorMessage.value = "";

  try {
    status.value = await docmindApi.retryFailedFile(path);
  } catch (error) {
    errorMessage.value = formatDocmindError(error, "重新处理失败");
    console.error("[DocMind] retryFailedFile failed", error);
  } finally {
    retryingTarget.value = null;
    await syncDashboardState();
  }
};

const retryFailedGroup = async (code: string, items: FailedFileView[]) => {
  retryingTarget.value = code;
  errorMessage.value = "";

  try {
    for (const item of items) {
      await docmindApi.retryFailedFile(item.file);
    }
  } catch (error) {
    errorMessage.value = formatDocmindError(error, "批量重新处理失败");
    console.error("[DocMind] retryFailedGroup failed", error);
  } finally {
    retryingTarget.value = null;
    await syncDashboardState();
  }
};

onMounted(async () => {
  await syncDashboardState();
});

onUnmounted(() => {
  stopPolling();
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
        class="mr-2 inline-flex items-center gap-2 rounded-2xl border border-slate-200 bg-white px-4 py-2 text-sm font-medium text-slate-700 shadow-sm hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
        :disabled="refreshing || loading || !status?.current_task || status.current_task.state === 'paused'"
        @click="pauseIndexing"
      >
        <Loader2 v-if="actionState === 'pausing'" :size="16" class="animate-spin" />
        {{ actionState === 'pausing' ? "暂停中..." : "暂停索引" }}
      </button>
      <button
        class="mr-2 inline-flex items-center gap-2 rounded-2xl border border-slate-200 bg-white px-4 py-2 text-sm font-medium text-slate-700 shadow-sm hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
        :disabled="refreshing || loading || !status?.current_task || status.current_task.state !== 'paused'"
        @click="resumeIndexing"
      >
        <Loader2 v-if="actionState === 'resuming'" :size="16" class="animate-spin" />
        <RefreshCw v-else :size="16" />
        {{ actionState === 'resuming' ? "恢复中..." : "恢复索引" }}
      </button>
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
          <div class="text-sm font-semibold text-slate-900">本次增量摘要</div>
          <div class="mt-1 text-xs text-slate-500">
            最近一次索引执行后的增量结果，任务结束后也会保留。
          </div>
        </div>
        <DocMindBadge tone="default">
          {{ status?.last_run ? status.last_run.completed_at : "暂无最近运行" }}
        </DocMindBadge>
      </div>
      <div v-if="status?.last_run" class="grid gap-3 md:grid-cols-2 xl:grid-cols-5">
        <div class="rounded-2xl bg-slate-50 px-4 py-3">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">更新</div>
          <div class="mt-1 text-sm font-semibold text-slate-900">{{ status.last_run.updated }}</div>
        </div>
        <div class="rounded-2xl bg-slate-50 px-4 py-3">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">跳过</div>
          <div class="mt-1 text-sm font-semibold text-slate-900">{{ status.last_run.skipped }}</div>
        </div>
        <div class="rounded-2xl bg-slate-50 px-4 py-3">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">删除</div>
          <div class="mt-1 text-sm font-semibold text-slate-900">{{ status.last_run.deleted }}</div>
        </div>
        <div class="rounded-2xl bg-slate-50 px-4 py-3">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">扫描候选</div>
          <div class="mt-1 text-sm font-semibold text-slate-900">{{ status.last_run.scanned }}</div>
        </div>
        <div class="rounded-2xl bg-slate-50 px-4 py-3">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">成功 / 失败</div>
          <div class="mt-1 text-sm font-semibold text-slate-900">
            {{ status.last_run.succeeded }} / {{ status.last_run.failed }}
          </div>
        </div>
      </div>
      <div v-else class="rounded-2xl bg-slate-50 px-4 py-6 text-sm text-slate-500">
        还没有形成最近一次增量摘要。
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
        <DocMindBadge :tone="taskDisplayState.tone">
          <Loader2 v-if="taskDisplayState.spinning" class="mr-1 animate-spin" :size="13" />
          {{ taskDisplayState.label }}
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

      <div v-if="status?.current_task" class="mt-4 grid gap-3 md:grid-cols-2 xl:grid-cols-6">
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
        <div class="rounded-2xl bg-slate-50 px-4 py-3">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">本次更新</div>
          <div class="mt-1 text-sm font-semibold text-slate-900">{{ status.current_task.updated }}</div>
        </div>
        <div class="rounded-2xl bg-slate-50 px-4 py-3">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">本次跳过</div>
          <div class="mt-1 text-sm font-semibold text-slate-900">{{ status.current_task.skipped }}</div>
        </div>
        <div class="rounded-2xl bg-slate-50 px-4 py-3">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">本次删除</div>
          <div class="mt-1 text-sm font-semibold text-slate-900">{{ status.current_task.deleted }}</div>
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
        <div v-for="group in failedGroups" :key="group.code" class="rounded-3xl border border-slate-100 bg-slate-50 p-4">
          <div class="mb-3 flex items-center justify-between gap-3">
            <div class="flex items-center gap-2 text-xs font-semibold uppercase tracking-wide text-slate-500">
              <FolderOpen :size="14" />
              {{ group.category }}
              <span class="rounded-full bg-white px-2 py-0.5 text-[10px] text-slate-400">{{ group.code }}</span>
              <span class="rounded-full bg-white px-2 py-0.5 text-[10px] text-slate-400">{{ group.items.length }}</span>
            </div>
            <button
              class="inline-flex items-center gap-1 rounded-xl border border-slate-200 bg-white px-3 py-1.5 text-xs font-medium text-slate-600 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
              :disabled="retryingTarget === group.code"
              @click="retryFailedGroup(group.code, group.items)"
            >
              <RefreshCw :size="13" :class="{ 'animate-spin': retryingTarget === group.code }" />
              组内重试
            </button>
          </div>
          <div class="space-y-3">
            <div v-for="file in group.items" :key="file.file" class="flex items-center justify-between rounded-2xl bg-white px-4 py-3 shadow-sm">
              <div>
                <div class="text-sm font-medium text-slate-800">{{ file.file }}</div>
                <div class="mt-1 text-xs text-slate-500">{{ file.reason }}</div>
                <div class="mt-1 flex flex-wrap gap-2 text-[11px] text-slate-400">
                  <span>代码 {{ file.code }}</span>
                  <span>重试 {{ file.retry_count }} 次</span>
                  <span>{{ file.last_failed_at }}</span>
                </div>
              </div>
              <button
                class="text-xs font-medium text-slate-600 disabled:cursor-not-allowed disabled:opacity-70"
                :disabled="retryingTarget === file.file"
                @click="retryFailedFile(file.file)"
              >
                {{ retryingTarget === file.file ? "处理中..." : "重新处理" }}
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
