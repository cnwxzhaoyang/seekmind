<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { listen } from "@tauri-apps/api/event";
import { AlertCircle, Loader2, RefreshCw, FolderOpen, Database, Cpu, Eye, Copy, FileText } from "lucide-vue-next";
import DocMindBadge from "../components/docmind/DocMindBadge.vue";
import DocMindIndexTree from "../components/docmind/DocMindIndexTree.vue";
import DocMindTaskCard from "../components/docmind/DocMindTaskCard.vue";
import { useIndexDirTree } from "../composables/useIndexDirTree";
import { docmindApi, formatDocmindError } from "../services/docmindApi";
import { formatDirectoryCitation } from "../utils/citation";
import type {
  FailedFileView,
  IndexDirView,
  IndexRefreshProgressView,
  IndexStatusView,
  ParserRuntimeView,
} from "../types/docmind";

const { t } = useI18n();

const status = ref<IndexStatusView | null>(null);
const dirs = ref<IndexDirView[]>([]);
const parserRuntime = ref<ParserRuntimeView | null>(null);
const loading = ref(false);
const refreshing = ref(false);
const retryingTarget = ref<string | null>(null);
const treeActionTarget = ref<string | null>(null);
const errorMessage = ref("");
const infoMessage = ref("");
const dashboardRefreshing = ref(false);
const actionState = ref<"pausing" | "resuming" | null>(null);
let pollTimer: number | null = null;
const indexRefreshJobResolvers = new Map<string, (payload: IndexRefreshProgressView) => void>();
const indexRefreshJobBufferedEvents = new Map<string, IndexRefreshProgressView>();
let unlistenIndexRefreshProgress: null | (() => void) = null;

const { visibleRows: visibleDirRows } = useIndexDirTree(dirs);

const explicitIndexDirCount = computed(() => dirs.value.filter((dir) => dir.is_explicit).length);

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
    category: items[0]?.category || t("common.unknown"),
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
    errorMessage.value = formatDocmindError(error, t("page.status.error.loadStatus"));
    console.error("[DocMind] loadStatus failed", error);
  } finally {
    loading.value = false;
  }
};

const copyText = async (text: string, successMessage: string) => {
  if (!text.trim()) {
    return;
  }

  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
    } else {
      const textarea = document.createElement("textarea");
      textarea.value = text;
      textarea.setAttribute("readonly", "true");
      textarea.style.position = "absolute";
      textarea.style.left = "-9999px";
      document.body.appendChild(textarea);
      textarea.select();
      document.execCommand("copy");
      document.body.removeChild(textarea);
    }
    infoMessage.value = successMessage;
  } catch (error) {
    errorMessage.value = formatDocmindError(error, successMessage);
  }
};

const loadDirs = async () => {
  try {
    dirs.value = await docmindApi.listIndexDirs();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.status.error.loadDirs"));
    console.error("[DocMind] loadDirs failed", error);
  }
};

const loadParserRuntime = async () => {
  try {
    parserRuntime.value = await docmindApi.getParserRuntime();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.status.error.loadParser"));
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

const waitForIndexRefreshJob = (jobId: string) => {
  const buffered = indexRefreshJobBufferedEvents.get(jobId);
  if (buffered) {
    indexRefreshJobBufferedEvents.delete(jobId);
    return Promise.resolve(buffered);
  }

  return new Promise<IndexRefreshProgressView>((resolve) => {
    indexRefreshJobResolvers.set(jobId, resolve);
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
      if (payload.state === "running") {
        return;
      }

      const resolver = indexRefreshJobResolvers.get(payload.job_id);
      if (resolver) {
        indexRefreshJobResolvers.delete(payload.job_id);
        resolver(payload);
      } else {
        indexRefreshJobBufferedEvents.set(payload.job_id, payload);
      }
    },
  );
};

void installIndexRefreshListener();

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
    const started = await docmindApi.refreshIndex();
    const finished = await waitForIndexRefreshJob(started.job_id);
    status.value = finished.status;
    if (finished.state === "failed") {
      errorMessage.value = finished.message;
    }
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.status.error.reindex"));
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
    errorMessage.value = formatDocmindError(error, t("page.status.error.pause"));
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
    errorMessage.value = formatDocmindError(error, t("page.status.error.resume"));
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
    return { label: t("status.idle"), tone: "default" as const, spinning: false };
  }

  if (actionState.value === "pausing") {
    return { label: t("status.pausing"), tone: "warning" as const, spinning: true };
  }
  if (actionState.value === "resuming") {
    return { label: t("status.resuming"), tone: "warning" as const, spinning: true };
  }
  if (task.state === "paused") {
    return { label: t("status.paused"), tone: "default" as const, spinning: false };
  }
  if (task.state === "running") {
    return { label: t("status.running"), tone: "warning" as const, spinning: true };
  }

  return { label: task.state || t("status.processing"), tone: "warning" as const, spinning: true };
});

const retryFailedFile = async (path: string) => {
  retryingTarget.value = path;
  errorMessage.value = "";

  try {
    status.value = await docmindApi.retryFailedFile(path);
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.status.error.retryFile"));
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
    errorMessage.value = formatDocmindError(error, t("page.status.error.retryGroup"));
    console.error("[DocMind] retryFailedGroup failed", error);
  } finally {
    retryingTarget.value = null;
    await syncDashboardState();
  }
};

const quickLookDir = async (path: string) => {
  treeActionTarget.value = path;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    await docmindApi.quickLookFile(path);
    infoMessage.value = t("page.status.action.quickLookOpened");
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.status.action.quickLookFailed"));
    console.error("[DocMind] quickLookDir failed", error);
  } finally {
    treeActionTarget.value = null;
  }
};

const copyDirPath = async (path: string) => {
  await copyText(path, t("page.status.action.copiedPath"));
};

const copyDirCitation = async (row: { displayName: string; fullPath: string; dir: IndexDirView }) => {
  await copyText(
    formatDirectoryCitation({
      displayName: row.displayName,
      path: row.fullPath,
      docs: row.dir.docs,
      chunks: row.dir.chunks,
    }),
    t("page.status.action.copiedCitation"),
  );
};

onMounted(async () => {
  await installIndexRefreshListener();
  await syncDashboardState();
});

onBeforeUnmount(() => {
  stopPolling();
  if (unlistenIndexRefreshProgress) {
    unlistenIndexRefreshProgress();
    unlistenIndexRefreshProgress = null;
  }
  indexRefreshJobResolvers.clear();
  indexRefreshJobBufferedEvents.clear();
});
</script>

<template>
  <div class="flex h-full min-h-0 flex-col bg-slate-50 text-slate-900">
    <header class="flex h-12 items-center justify-between gap-4 border-b border-slate-200 bg-white px-5">
      <div class="min-w-0">
        <h1 class="text-base font-semibold tracking-tight text-slate-950">{{ t("page.status.title") }}</h1>
        <p class="mt-0.5 text-xs text-slate-500">{{ t("page.status.subtitle") }}</p>
      </div>
      <DocMindBadge :tone="parserRuntime?.active === 'python' ? 'success' : 'warning'">
        <Cpu class="mr-1" :size="13" />
        {{ parserRuntime?.active === 'python' ? t("status.parser.python") : t("status.parser.pythonFallback") }}
      </DocMindBadge>
    </header>

    <main class="min-h-0 flex-1 overflow-y-auto p-4">
      <div class="mb-3 flex items-center justify-between border-b border-slate-200 bg-white px-4 py-2">
        <div>
          <div class="text-[10px] font-semibold uppercase tracking-[0.16em] text-slate-500">{{ t("page.status.section.taskOps") }}</div>
          <div class="mt-1 text-xs text-slate-500">{{ t("page.status.section.taskOpsDesc") }}</div>
        </div>
        <div class="flex flex-wrap items-center gap-2">
          <button
            class="inline-flex items-center gap-2 rounded-md border border-slate-200 bg-white px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
            :disabled="refreshing || loading || !status?.current_task || status.current_task.state === 'paused'"
            @click="pauseIndexing"
          >
            <Loader2 v-if="actionState === 'pausing'" :size="15" class="animate-spin" />
            {{ actionState === 'pausing' ? t("page.status.btn.pausing") : t("page.status.btn.pause") }}
          </button>
          <button
            class="inline-flex items-center gap-2 rounded-md border border-slate-200 bg-white px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
            :disabled="refreshing || loading || !status?.current_task || status.current_task.state !== 'paused'"
            @click="resumeIndexing"
          >
            <Loader2 v-if="actionState === 'resuming'" :size="15" class="animate-spin" />
            <RefreshCw v-else :size="15" />
            {{ actionState === 'resuming' ? t("page.status.btn.resuming") : t("page.status.btn.resume") }}
          </button>
          <button
            class="inline-flex items-center gap-2 rounded-md bg-slate-900 px-3 py-1.5 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-70"
            :disabled="refreshing || loading"
            @click="refreshIndex"
          >
            <RefreshCw :size="15" :class="{ 'animate-spin': refreshing }" />
            {{ refreshing ? t("page.status.btn.rebuilding") : t("page.status.btn.reindex") }}
          </button>
        </div>
      </div>

      <div class="grid gap-2 sm:grid-cols-2 xl:grid-cols-4">
        <div
          v-for="card in [
            { label: t('page.status.stats.scanned'), value: status?.scanned_docs ?? 0 },
            { label: t('page.status.stats.indexed'), value: status?.indexed_docs ?? 0 },
            { label: t('page.status.stats.chunks'), value: status?.indexed_chunks ?? 0 },
            { label: t('page.status.stats.failed'), value: status?.failed_files ?? 0 },
          ]"
          :key="card.label"
          class="rounded-md border border-slate-200 bg-white px-3 py-2"
        >
          <div class="text-[10px] uppercase tracking-wide text-slate-500">{{ card.label }}</div>
          <div class="mt-1 text-xl font-semibold text-slate-950">{{ card.value }}</div>
        </div>
      </div>

      <div class="mt-3 grid gap-3 xl:grid-cols-2">
        <div class="rounded-md border border-slate-200 bg-white px-4 py-3">
          <div class="mb-2 flex items-center justify-between gap-3">
            <div>
              <div class="text-[10px] font-semibold uppercase tracking-[0.16em] text-slate-500">{{ t("page.status.section.incrementalSummary") }}</div>
              <div class="mt-1 text-xs text-slate-500">{{ t("page.status.section.incrementalDesc") }}</div>
            </div>
            <DocMindBadge tone="default">
              {{ status?.last_run ? status.last_run.completed_at : t("status.noRecentRun") }}
            </DocMindBadge>
          </div>
          <div v-if="status?.last_run" class="grid gap-2 md:grid-cols-2 xl:grid-cols-3">
            <div class="rounded-md bg-slate-50 px-3 py-2">
              <div class="text-[10px] uppercase tracking-wide text-slate-500">{{ t("page.status.incremental.updated") }}</div>
              <div class="mt-1 text-sm font-semibold text-slate-900">{{ status.last_run.updated }}</div>
            </div>
            <div class="rounded-md bg-slate-50 px-3 py-2">
              <div class="text-[10px] uppercase tracking-wide text-slate-500">{{ t("page.status.incremental.skipped") }}</div>
              <div class="mt-1 text-sm font-semibold text-slate-900">{{ status.last_run.skipped }}</div>
            </div>
            <div class="rounded-md bg-slate-50 px-3 py-2">
              <div class="text-[10px] uppercase tracking-wide text-slate-500">{{ t("page.status.incremental.deleted") }}</div>
              <div class="mt-1 text-sm font-semibold text-slate-900">{{ status.last_run.deleted }}</div>
            </div>
            <div class="rounded-md bg-slate-50 px-3 py-2">
              <div class="text-[10px] uppercase tracking-wide text-slate-500">{{ t("page.status.incremental.scannedCandidates") }}</div>
              <div class="mt-1 text-sm font-semibold text-slate-900">{{ status.last_run.scanned }}</div>
            </div>
            <div class="rounded-md bg-slate-50 px-3 py-2">
              <div class="text-[10px] uppercase tracking-wide text-slate-500">{{ t("page.status.incremental.successFail") }}</div>
              <div class="mt-1 text-sm font-semibold text-slate-900">{{ status.last_run.succeeded }} / {{ status.last_run.failed }}</div>
            </div>
          </div>
          <div v-else class="rounded-md border border-dashed border-slate-200 bg-white px-4 py-6 text-xs text-slate-400">
            {{ t("page.status.incremental.none") }}
          </div>
        </div>

        <div class="rounded-md border border-slate-200 bg-white px-4 py-3">
          <div class="mb-2 flex items-center justify-between gap-3">
            <div>
              <div class="text-[10px] font-semibold uppercase tracking-[0.16em] text-slate-500">{{ t("page.status.section.parserStatus") }}</div>
              <div class="mt-1 text-xs text-slate-500">{{ t("page.status.section.parserDesc") }}</div>
            </div>
          </div>
          <div v-if="parserRuntime" class="grid gap-2 md:grid-cols-2">
            <div class="rounded-md bg-slate-50 px-3 py-2">
              <div class="text-[10px] uppercase tracking-wide text-slate-500">{{ t("page.status.parser.enabled") }}</div>
              <div class="mt-1 text-sm font-semibold text-slate-900">{{ parserRuntime.enabled ? t("common.yes") : t("common.no") }}</div>
            </div>
            <div class="rounded-md bg-slate-50 px-3 py-2">
              <div class="text-[10px] uppercase tracking-wide text-slate-500">{{ t("page.status.parser.available") }}</div>
              <div class="mt-1 text-sm font-semibold text-slate-900">{{ parserRuntime.available ? t("common.yes") : t("common.no") }}</div>
            </div>
            <div class="rounded-md bg-slate-50 px-3 py-2">
              <div class="text-[10px] uppercase tracking-wide text-slate-500">{{ t("page.status.parser.pythonBin") }}</div>
              <div class="mt-1 truncate text-sm font-medium text-slate-900">{{ parserRuntime.python_bin }}</div>
            </div>
            <div class="rounded-md bg-slate-50 px-3 py-2">
              <div class="text-[10px] uppercase tracking-wide text-slate-500">{{ t("page.status.parser.timeout") }}</div>
              <div class="mt-1 text-sm font-semibold text-slate-900">{{ parserRuntime.timeout_ms }} ms</div>
            </div>
          </div>
          <div v-if="parserRuntime" class="mt-2 text-xs text-slate-500">
            {{ t("page.status.parser.script", { path: parserRuntime.script_path }) }}
          </div>
        </div>
      </div>

      <div class="mt-3 rounded-md border border-slate-200 bg-white px-4 py-3">
        <div class="mb-2 flex items-center justify-between gap-3">
          <div>
            <div class="text-[10px] font-semibold uppercase tracking-[0.16em] text-slate-500">{{ t("page.status.section.indexDirs") }}</div>
            <div class="mt-1 text-xs text-slate-500">{{ t("page.status.section.indexDirsDesc") }}</div>
          </div>
          <DocMindBadge tone="default">
            <Database class="mr-1" :size="13" />
            {{ explicitIndexDirCount }} {{ t("page.status.section.indexDirs") }}
          </DocMindBadge>
        </div>
        <div v-if="dirs.length === 0" class="rounded-md border border-dashed border-slate-200 bg-white px-4 py-6 text-xs text-slate-400">
          {{ t("page.status.emptyDirs") }}
        </div>
        <DocMindIndexTree
          v-else
          :rows="visibleDirRows"
          :selected-path="''"
          :path-tooltip="true"
          :selectable="false"
          :virtual-label="t('common.virtualDir')"
          :expand-title="t('sidebar.expand')"
          :collapse-title="t('sidebar.collapse')"
          density="compact"
        >
          <template #meta="{ row }">
            <span class="truncate">
              {{ t("page.chunks.dirDocs", { docs: row.dir.docs, chunks: row.dir.chunks.toLocaleString() }) }}
            </span>
          </template>
          <template #status="{ row }">
            <DocMindBadge :tone="row.dir.enabled ? 'success' : 'default'">
              {{ row.dir.enabled ? t("common.enabled") : t("common.disabled") }}
            </DocMindBadge>
          </template>
          <template #actions="{ row }">
            <div class="flex items-center gap-1.5">
              <button
                class="rounded-md border border-slate-200 p-2 text-slate-500 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
                :disabled="treeActionTarget === row.dir.path || row.isVirtual"
                :title="t('page.status.action.quickLook')"
                @click.stop="quickLookDir(row.dir.path)"
              >
                <Eye :size="14" />
              </button>
              <button
                class="rounded-md border border-slate-200 p-2 text-slate-500 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
                :disabled="treeActionTarget === row.dir.path"
                :title="t('page.status.action.copyPath')"
                @click.stop="copyDirPath(row.dir.path)"
              >
                <Copy :size="14" />
              </button>
              <button
                class="rounded-md border border-slate-200 p-2 text-slate-500 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
                :disabled="treeActionTarget === row.dir.path"
                :title="t('page.status.action.copyCitation')"
                @click.stop="copyDirCitation(row)"
              >
                <FileText :size="14" />
              </button>
            </div>
          </template>
        </DocMindIndexTree>
      </div>

      <div v-if="infoMessage" class="mt-3 rounded-md border border-emerald-100 bg-emerald-50 px-4 py-2.5 text-xs text-emerald-700">
        {{ infoMessage }}
      </div>

      <div v-if="errorMessage" class="mt-3 rounded-md border border-red-100 bg-red-50 px-4 py-2.5 text-xs text-red-700">
        {{ errorMessage }}
      </div>

      <div v-if="loading" class="mt-3 rounded-md border border-dashed border-slate-200 bg-white px-4 py-5 text-xs text-slate-400">
        {{ t("page.status.subtitle") }}...
      </div>

      <div class="mt-3">
        <DocMindTaskCard
          :task="status?.current_task ?? null"
          :title="t('taskCard.defaultTitle')"
          :description="status?.current_task?.label ?? t('status.noRecentRun')"
          :badge-label="taskDisplayState.label"
          :badge-tone="taskDisplayState.tone"
          :badge-spinning="taskDisplayState.spinning"
        />
      </div>

      <div class="mt-3 rounded-md border border-slate-200 bg-white px-4 py-3">
        <div class="mb-2 flex items-center gap-2 text-[10px] font-semibold uppercase tracking-[0.16em] text-slate-500">
          <AlertCircle :size="16" class="text-amber-500" />
          {{ t("page.status.section.failedFiles") }}
        </div>
        <div v-if="failedGroups.length === 0" class="rounded-md border border-dashed border-slate-200 bg-white px-4 py-6 text-xs text-slate-400">
          {{ t("page.status.failed.noFailed") }}
        </div>
        <div v-else class="space-y-3">
          <div v-for="group in failedGroups" :key="group.code" class="rounded-md border border-slate-200 bg-slate-50 px-3 py-3">
            <div class="mb-2 flex flex-wrap items-center justify-between gap-2">
              <div class="flex items-center gap-2 text-[11px] font-semibold uppercase tracking-wide text-slate-500">
                <FolderOpen :size="13" />
                {{ group.category }}
                <span class="rounded-full bg-white px-2 py-0.5 text-[10px] text-slate-400">{{ group.code }}</span>
                <span class="rounded-full bg-white px-2 py-0.5 text-[10px] text-slate-400">{{ group.items.length }}</span>
              </div>
              <button
                class="inline-flex items-center gap-1 rounded-md border border-slate-200 bg-white px-3 py-1 text-xs font-medium text-slate-600 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
                :disabled="retryingTarget === group.code"
                @click="retryFailedGroup(group.code, group.items)"
              >
                <RefreshCw :size="13" :class="{ 'animate-spin': retryingTarget === group.code }" />
                {{ t("page.status.failed.retryGroup") }}
              </button>
            </div>
            <div class="space-y-2">
              <div
                v-for="file in group.items"
                :key="file.file"
                class="flex items-start justify-between gap-3 rounded-md border border-slate-200 bg-white px-3 py-2"
              >
                <div class="min-w-0">
                  <div class="truncate text-sm font-medium text-slate-950">{{ file.file }}</div>
                  <div class="mt-1 text-xs text-slate-500">{{ file.reason }}</div>
                  <div class="mt-1 flex flex-wrap gap-2 text-[11px] text-slate-400">
                    <span>{{ t("page.status.failed.code", { code: file.code }) }}</span>
                    <span>{{ t("page.status.failed.retryCount", { count: file.retry_count }) }}</span>
                    <span>{{ file.last_failed_at }}</span>
                  </div>
                </div>
                <button
                  class="shrink-0 text-xs font-medium text-slate-600 disabled:cursor-not-allowed disabled:opacity-70"
                  :disabled="retryingTarget === file.file"
                  @click="retryFailedFile(file.file)"
                >
                  {{ retryingTarget === file.file ? t("page.status.failed.retrying") : t("page.status.failed.retryFile") }}
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </main>
  </div>
</template>
