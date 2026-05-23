<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderPlus, FolderOpen, CheckCircle2, Loader2, RefreshCw, X, ToggleLeft, ToggleRight } from "lucide-vue-next";
import DocMindBadge from "../components/docmind/DocMindBadge.vue";
import DocMindTaskCard from "../components/docmind/DocMindTaskCard.vue";
import { docmindApi, formatDocmindError } from "../services/docmindApi";
import type { IndexDirView, IndexRefreshProgressView, IndexStatusView } from "../types/docmind";

const dirs = ref<IndexDirView[]>([]);
const status = ref<IndexStatusView | null>(null);
const loading = ref(false);
const refreshing = ref(false);
const busyPath = ref<string | null>(null);
const errorMessage = ref("");
const infoMessage = ref("");
const refreshJobResolvers = new Map<string, (payload: IndexRefreshProgressView) => void>();
const refreshJobBufferedEvents = new Map<string, IndexRefreshProgressView>();
let unlistenIndexRefreshProgress: null | (() => void) = null;

const taskDisplayState = computed(() => {
  const task = status.value?.current_task;
  if (!task) {
    return { label: "空闲", tone: "default" as const, spinning: false };
  }

  if (task.state === "paused") {
    return { label: "已暂停", tone: "default" as const, spinning: false };
  }
  if (task.state === "running") {
    return { label: "运行中", tone: "warning" as const, spinning: true };
  }

  return { label: task.state || "处理中", tone: "warning" as const, spinning: true };
});

const loadDirs = async () => {
  loading.value = true;
  errorMessage.value = "";

  try {
    dirs.value = await docmindApi.listIndexDirs();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, "目录加载失败");
    console.error("[DocMind] loadDirs failed", error);
  } finally {
    loading.value = false;
  }
};

const loadStatus = async () => {
  try {
    status.value = await docmindApi.getIndexStatus();
  } catch (error) {
    console.error("[DocMind] loadStatus failed", error);
  }
};

const waitForIndexRefreshJob = (jobId: string) => {
  const buffered = refreshJobBufferedEvents.get(jobId);
  if (buffered) {
    refreshJobBufferedEvents.delete(jobId);
    return Promise.resolve(buffered);
  }

  return new Promise<IndexRefreshProgressView>((resolve) => {
    refreshJobResolvers.set(jobId, resolve);
  });
};

const installIndexRefreshListener = async () => {
  if (unlistenIndexRefreshProgress) {
    return;
  }

  unlistenIndexRefreshProgress = await listen<IndexRefreshProgressView>(
    "docmind:index-refresh-progress",
    (event) => {
      const payload = event.payload;
      status.value = payload.status;

      const resolver = refreshJobResolvers.get(payload.job_id);
      if (payload.state !== "running" && resolver) {
        refreshJobResolvers.delete(payload.job_id);
        resolver(payload);
      } else if (payload.state !== "running") {
        refreshJobBufferedEvents.set(payload.job_id, payload);
      }
    },
  );
};

void installIndexRefreshListener();

const chooseAndAddDir = async () => {
  errorMessage.value = "";
  infoMessage.value = "";

  const selected = await open({
    directory: true,
    multiple: false,
    title: "选择要索引的目录",
  });

  if (typeof selected !== "string" || selected.trim().length === 0) {
    return;
  }

  busyPath.value = selected;
  try {
    await docmindApi.addIndexDir(selected);
    infoMessage.value = `已添加目录: ${selected}`;
    await loadDirs();
    await loadStatus();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, "添加目录失败");
    console.error("[DocMind] addIndexDir failed", error);
  } finally {
    busyPath.value = null;
  }
};

const refreshIndex = async () => {
  refreshing.value = true;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    const started = await docmindApi.refreshIndex();
    const finished = await waitForIndexRefreshJob(started.job_id);
    if (finished.state === "failed") {
      throw new Error(finished.message || "重新索引失败");
    }
    await loadDirs();
    await loadStatus();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, "重新索引失败");
    console.error("[DocMind] refreshIndex failed", error);
  } finally {
    refreshing.value = false;
  }
};

const refreshSingleDir = async (path: string) => {
  busyPath.value = path;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    const started = await docmindApi.refreshIndexDir(path);
    const finished = await waitForIndexRefreshJob(started.job_id);
    if (finished.state === "failed") {
      throw new Error(finished.message || "目录重建失败");
    }
    infoMessage.value = `已重新索引: ${path}`;
    await loadDirs();
    await loadStatus();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, "目录重建失败");
    console.error("[DocMind] refreshIndexDir failed", error);
  } finally {
    busyPath.value = null;
  }
};

const toggleDir = async (dir: IndexDirView) => {
  busyPath.value = dir.path;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    await docmindApi.setIndexDirEnabled(dir.path, !dir.enabled);
    infoMessage.value = dir.enabled ? `已禁用: ${dir.path}` : `已启用: ${dir.path}`;
    await loadDirs();
    await loadStatus();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, "目录状态更新失败");
    console.error("[DocMind] setIndexDirEnabled failed", error);
  } finally {
    busyPath.value = null;
  }
};

const removeDir = async (path: string) => {
  if (!window.confirm(`确认删除索引目录？\n${path}`)) {
    return;
  }

  busyPath.value = path;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    await docmindApi.removeIndexDir(path);
    infoMessage.value = `已删除目录: ${path}`;
    await loadDirs();
    await loadStatus();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, "删除目录失败");
    console.error("[DocMind] removeIndexDir failed", error);
  } finally {
    busyPath.value = null;
  }
};

onMounted(loadDirs);
onMounted(loadStatus);

onMounted(() => {
  void installIndexRefreshListener();
});

onBeforeUnmount(() => {
  if (unlistenIndexRefreshProgress) {
    unlistenIndexRefreshProgress();
    unlistenIndexRefreshProgress = null;
  }
  refreshJobResolvers.clear();
  refreshJobBufferedEvents.clear();
});
</script>

<template>
  <div class="h-full overflow-y-auto p-8">
    <div class="mb-7 flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-semibold tracking-tight text-slate-950">文档目录</h1>
        <p class="mt-1 text-sm text-slate-500">添加需要索引的文件夹，docMind 会在本地解析和建立索引。</p>
      </div>
      <button
        class="flex items-center gap-2 rounded-2xl bg-slate-900 px-4 py-2.5 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-70"
        :disabled="refreshing"
        @click="chooseAndAddDir"
      >
        <FolderPlus :size="17" />
        添加目录
      </button>
    </div>

    <DocMindTaskCard
      :task="status?.current_task ?? null"
      title="当前索引任务"
      :description="status?.current_task?.details ?? '正在同步索引状态'"
      :badge-label="taskDisplayState.label"
      :badge-tone="taskDisplayState.tone"
      :badge-spinning="taskDisplayState.spinning"
      class="mb-6"
    />

    <div v-if="errorMessage" class="mb-4 rounded-2xl border border-red-100 bg-red-50 px-4 py-3 text-sm text-red-700">
      {{ errorMessage }}
    </div>

    <div v-if="infoMessage" class="mb-4 rounded-2xl border border-emerald-100 bg-emerald-50 px-4 py-3 text-sm text-emerald-700">
      {{ infoMessage }}
    </div>

    <div v-if="loading" class="rounded-3xl border border-dashed border-slate-300 bg-white p-6 text-sm text-slate-500">
      正在读取索引目录...
    </div>

    <div v-else class="space-y-3">
      <div v-for="dir in dirs" :key="dir.path" class="rounded-3xl border border-slate-200 bg-white p-5 shadow-sm">
        <div class="flex items-center justify-between gap-4">
          <div class="flex min-w-0 items-center gap-4">
            <div class="flex h-11 w-11 items-center justify-center rounded-2xl bg-slate-100">
              <FolderOpen :size="20" class="text-slate-600" />
            </div>
            <div class="min-w-0">
              <div class="truncate text-sm font-semibold text-slate-900">{{ dir.path }}</div>
              <div class="mt-1 text-xs text-slate-500">{{ dir.docs }} 个文档 · {{ dir.chunks.toLocaleString() }} 个段落</div>
            </div>
          </div>

          <div class="flex items-center gap-2">
            <DocMindBadge v-if="dir.status === 'indexing'" tone="warning">
              <Loader2 class="mr-1 animate-spin" :size="13" />
              索引中
            </DocMindBadge>
            <DocMindBadge v-else tone="success">
              <CheckCircle2 class="mr-1" :size="13" />
              已完成
            </DocMindBadge>
            <button
              class="rounded-xl border border-slate-200 p-2 text-slate-500 hover:bg-slate-50"
              :disabled="busyPath === dir.path"
              @click="refreshSingleDir(dir.path)"
            >
              <RefreshCw :size="16" />
            </button>
            <button
              class="rounded-xl border border-slate-200 p-2 text-slate-500 hover:bg-slate-50"
              :disabled="busyPath === dir.path"
              @click="toggleDir(dir)"
            >
              <component :is="dir.enabled ? ToggleRight : ToggleLeft" :size="16" />
            </button>
            <button
              class="rounded-xl border border-slate-200 p-2 text-slate-500 hover:bg-slate-50"
              :disabled="busyPath === dir.path"
              @click="removeDir(dir.path)"
            >
              <X :size="16" />
            </button>
          </div>
        </div>
      </div>
    </div>

    <div class="mt-8 rounded-3xl border border-dashed border-slate-300 bg-slate-50 p-8 text-center">
      <FolderPlus class="mx-auto mb-3 text-slate-400" :size="28" />
      <div class="text-sm font-medium text-slate-700">支持添加、启停和删除索引目录</div>
      <div class="mt-1 text-xs text-slate-500">建议优先添加 Documents、Downloads 或你的项目资料目录。</div>
    </div>
  </div>
</template>
