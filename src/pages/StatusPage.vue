<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { useI18n } from "vue-i18n";
import { listen } from "@tauri-apps/api/event";
import { AlertCircle, Loader2, RefreshCw, FolderOpen, FolderPlus, UploadCloud, Database, Cpu, Eye, Copy, FileText, ToggleLeft, ToggleRight, X } from "lucide-vue-next";
import DocMindBadge from "../components/docmind/DocMindBadge.vue";
import DocMindContextMenu from "../components/docmind/DocMindContextMenu.vue";
import type { ContextMenuItem } from "../components/docmind/DocMindContextMenu.vue";
import DocMindIndexTree from "../components/docmind/DocMindIndexTree.vue";
import DocMindTaskCard from "../components/docmind/DocMindTaskCard.vue";
import { useIndexDirTree } from "../composables/useIndexDirTree";
import { docmindApi, formatDocmindError } from "../services/docmindApi";
import { formatDirectoryCitation } from "../utils/citation";
import type { VisibleIndexDirRow } from "../composables/useIndexDirTree";
import type {
  FailedFileView,
  IndexDirView,
  DocumentRefreshProgressView,
  IndexRefreshProgressView,
  IndexStatusView,
  ImportedPathView,
  ImportPathsView,
  ParserRuntimeView,
} from "../types/docmind";

const { t } = useI18n();

const status = ref<IndexStatusView | null>(null);
const dirs = ref<IndexDirView[]>([]);
const parserRuntime = ref<ParserRuntimeView | null>(null);
const loading = ref(false);
const refreshing = ref(false);
const importing = ref(false);
const dragActive = ref(false);
const retryingTarget = ref<string | null>(null);
const busyPath = ref<string | null>(null);
const treeActionTarget = ref<string | null>(null);
const errorMessage = ref("");
const infoMessage = ref("");
const dashboardRefreshing = ref(false);
const actionState = ref<"pausing" | "resuming" | null>(null);
let pollTimer: number | null = null;
const indexRefreshJobResolvers = new Map<string, (payload: IndexRefreshProgressView) => void>();
const indexRefreshJobBufferedEvents = new Map<string, IndexRefreshProgressView>();
const documentRefreshResolvers = new Map<string, (payload: DocumentRefreshProgressView) => void>();
const documentRefreshBufferedEvents = new Map<string, DocumentRefreshProgressView>();
let unlistenIndexRefreshProgress: null | (() => void) = null;
let unlistenDocumentRefreshProgress: null | (() => void) = null;
let unlistenFileDrop: null | (() => void) = null;

const {
  visibleRows: visibleDirRows,
  setExpanded: setDirExpanded,
} = useIndexDirTree(dirs);

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

const refreshSingleDir = async (path: string) => {
  busyPath.value = path;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    const started = await docmindApi.refreshIndexDir(path);
    const finished = await waitForIndexRefreshJob(started.job_id);
    if (finished.state === "failed") {
      throw new Error(finished.message || t("page.library.error.rebuild"));
    }
    infoMessage.value = t("page.library.info.reindexed", { path });
    await refreshDashboard();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.library.error.rebuild"));
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
    infoMessage.value = dir.enabled ? t("page.library.info.disabled", { path: dir.path }) : t("page.library.info.enabled", { path: dir.path });
    await refreshDashboard();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.library.error.toggleDir"));
    console.error("[DocMind] setIndexDirEnabled failed", error);
  } finally {
    busyPath.value = null;
  }
};

const removeDir = async (path: string) => {
  if (!window.confirm(t("page.library.confirmRemove", { path }))) {
    return;
  }

  busyPath.value = path;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    await docmindApi.removeIndexDir(path);
    infoMessage.value = t("page.library.info.deleted", { path });
    await refreshDashboard();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.library.error.deleteDir"));
    console.error("[DocMind] removeIndexDir failed", error);
  } finally {
    busyPath.value = null;
  }
};

const chooseAndAddDir = async () => {
  errorMessage.value = "";
  infoMessage.value = "";

  const selected = await open({
    directory: true,
    multiple: false,
    title: t("page.library.dialogTitle"),
  });

  if (typeof selected !== "string" || selected.trim().length === 0) {
    return;
  }

  busyPath.value = selected;
  try {
    await docmindApi.addIndexDir(selected);
    infoMessage.value = t("page.library.info.added", { path: selected });
    await refreshDashboard();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.library.error.addDir"));
    console.error("[DocMind] addIndexDir failed", error);
  } finally {
    busyPath.value = null;
  }
};

const waitForDocumentRefreshJob = (jobId: string) => {
  const buffered = documentRefreshBufferedEvents.get(jobId);
  if (buffered) {
    documentRefreshBufferedEvents.delete(jobId);
    return Promise.resolve(buffered);
  }

  return new Promise<DocumentRefreshProgressView>((resolve) => {
    documentRefreshResolvers.set(jobId, resolve);
  });
};

const installDocumentRefreshListener = async () => {
  if (unlistenDocumentRefreshProgress) {
    return;
  }

  unlistenDocumentRefreshProgress = await listen<DocumentRefreshProgressView>(
    "docmind:document-refresh-progress",
    (event) => {
      const payload = event.payload;
      if (payload.state === "running") {
        return;
      }

      const resolver = documentRefreshResolvers.get(payload.job_id);
      if (resolver) {
        documentRefreshResolvers.delete(payload.job_id);
        resolver(payload);
      } else {
        documentRefreshBufferedEvents.set(payload.job_id, payload);
      }
    },
  );
};

const installFileDropListener = async () => {
  if (unlistenFileDrop) {
    return;
  }

  if (typeof window === "undefined") {
    return;
  }

  const webview = getCurrentWebview();
  const unlisten = await webview.onDragDropEvent((event) => {
    const payload = event.payload;

    if (payload.type === "enter") {
      dragActive.value = payload.paths.length > 0;
      return;
    }

    if (payload.type === "over") {
      dragActive.value = true;
      return;
    }

    if (payload.type === "leave") {
      dragActive.value = false;
      return;
    }

    dragActive.value = false;
    void importDroppedPaths(payload.paths);
  });

  unlistenFileDrop = unlisten;
};

const processImportedFiles = async (importedFiles: ImportedPathView[]) => {
  const queued = importedFiles.filter((item) => item.dir_path !== "");
  for (const file of queued) {
    busyPath.value = file.dir_path;
    try {
      const started = await docmindApi.refreshDocument(file.path, file.dir_path);
      const finished = await waitForDocumentRefreshJob(started.job_id);
      if (finished.state === "failed") {
        throw new Error(finished.message || t("page.library.error.rebuild"));
      }
    } finally {
      busyPath.value = null;
    }
  }
};

const processImportedDirs = async (dirsToRefresh: string[]) => {
  for (const path of dirsToRefresh) {
    busyPath.value = path;
    try {
      const started = await docmindApi.refreshIndexDir(path);
      const finished = await waitForIndexRefreshJob(started.job_id);
      if (finished.state === "failed") {
        throw new Error(finished.message || t("page.library.error.rebuild"));
      }
    } finally {
      busyPath.value = null;
    }
  }
};

const importDroppedPaths = async (paths: string[]) => {
  const normalized = paths.map((path) => path.trim()).filter((path) => path.length > 0);
  if (normalized.length === 0) {
    return;
  }

  importing.value = true;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    const result: ImportPathsView = await docmindApi.importPaths(normalized);
    const dirsToRefresh = result.added_dirs.filter((path) => path !== result.virtual_dir);
    if (dirsToRefresh.length > 0) {
      infoMessage.value = t("page.library.info.importing", { count: normalized.length });
      await processImportedDirs(dirsToRefresh);
    }

    const filesToRefresh = result.imported_files.filter(
      (file) => file.is_virtual || !dirsToRefresh.includes(file.dir_path),
    );
    if (filesToRefresh.length > 0) {
      await processImportedFiles(filesToRefresh);
    }

    const summaryParts = [
      t("page.library.info.importedDirs", { count: result.added_dirs.length }),
      t("page.library.info.importedFiles", { count: result.imported_files.length }),
    ];
    if (result.virtual_dir) {
      summaryParts.push(t("page.library.info.virtualDir", { path: result.virtual_dir }));
    }
    if (result.unsupported.length > 0) {
      summaryParts.push(t("page.library.info.unsupported", { count: result.unsupported.length }));
    }
    if (result.skipped.length > 0) {
      summaryParts.push(t("page.library.info.skipped", { count: result.skipped.length }));
    }
    infoMessage.value = summaryParts.join(" · ");
    await refreshDashboard();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.library.error.importPaths"));
    console.error("[DocMind] importPaths failed", error);
  } finally {
    importing.value = false;
    dragActive.value = false;
    busyPath.value = null;
  }
};


const contextMenuRow = ref<VisibleIndexDirRow | null>(null);
const contextMenuPosition = ref({ x: 0, y: 0 });
const contextMenuVisible = ref(false);

const contextMenuItems = computed<ContextMenuItem[]>(() => {
  const row = contextMenuRow.value;
  if (!row) return [];
  return [
    {
      key: "quickLook",
      label: t("page.status.action.quickLook"),
      icon: Eye,
      disabled: treeActionTarget.value === row.dir.path || row.isVirtual,
      handler: () => quickLookDir(row.dir.path),
    },
    {
      key: "copyPath",
      label: t("page.status.action.copyPath"),
      icon: Copy,
      disabled: treeActionTarget.value === row.dir.path,
      handler: () => copyDirPath(row.dir.path),
    },
    {
      key: "copyCitation",
      label: t("page.status.action.copyCitation"),
      icon: FileText,
      disabled: treeActionTarget.value === row.dir.path,
      handler: () => copyDirCitation(row),
    },
    { key: "divider1", label: "", divider: true, handler: () => {} },
    {
      key: "refresh",
      label: t("common.reindex"),
      icon: RefreshCw,
      disabled: busyPath.value === row.dir.path || !row.dir.is_explicit,
      handler: () => refreshSingleDir(row.dir.path),
    },
    {
      key: "toggle",
      label: row.dir.enabled ? t("common.disabled") : t("common.enabled"),
      icon: row.dir.enabled ? ToggleRight : ToggleLeft,
      disabled: busyPath.value === row.dir.path || !row.dir.is_explicit,
      handler: () => toggleDir(row.dir),
    },
    {
      key: "remove",
      label: t("page.library.action.removeDir"),
      icon: X,
      disabled: busyPath.value === row.dir.path || !row.dir.is_explicit,
      danger: true,
      handler: () => removeDir(row.dir.path),
    },
  ];
});

const handleTreeContextMenu = (row: VisibleIndexDirRow, event: MouseEvent) => {
  contextMenuRow.value = row;
  contextMenuPosition.value = { x: event.clientX, y: event.clientY };
  contextMenuVisible.value = true;
};

onMounted(() => {
  void installIndexRefreshListener();
  void installDocumentRefreshListener();
  void installFileDropListener();
  void syncDashboardState();
});

onBeforeUnmount(() => {
  stopPolling();
  if (unlistenIndexRefreshProgress) {
    unlistenIndexRefreshProgress();
    unlistenIndexRefreshProgress = null;
  }
  if (unlistenDocumentRefreshProgress) {
    unlistenDocumentRefreshProgress();
    unlistenDocumentRefreshProgress = null;
  }
  if (unlistenFileDrop) {
    unlistenFileDrop();
    unlistenFileDrop = null;
  }
  indexRefreshJobResolvers.clear();
  indexRefreshJobBufferedEvents.clear();
  documentRefreshResolvers.clear();
  documentRefreshBufferedEvents.clear();
});
</script>

<template>
  <div class="flex h-full min-h-0 flex-col bg-page text-primary">
    <header class="flex h-12 items-center justify-between gap-4 border-b border-default bg-header px-5">
      <div class="min-w-0">
        <h1 class="text-base font-semibold tracking-tight text-primary">{{ t("page.status.title") }}</h1>
        <p class="mt-0.5 text-xs text-dim">{{ t("page.status.subtitle") }}</p>
      </div>
      <DocMindBadge :tone="parserRuntime?.active === 'python' ? 'success' : 'warning'">
        <Cpu class="mr-1" :size="13" />
        {{ parserRuntime?.active === 'python' ? t("status.parser.python") : t("status.parser.pythonFallback") }}
      </DocMindBadge>
    </header>

    <main class="min-h-0 flex-1 overflow-y-auto p-4">
      <div class="mb-3 flex items-center justify-between border-b border-default bg-surface px-4 py-2">
        <div>
          <div class="text-[10px] font-semibold uppercase tracking-[0.16em] text-dim">{{ t("page.status.section.taskOps") }}</div>
          <div class="mt-1 text-xs text-dim">{{ t("page.status.section.taskOpsDesc") }}</div>
        </div>
        <div class="flex flex-wrap items-center gap-2">
          <button
            class="inline-flex items-center gap-2 rounded-md border border-default bg-surface px-3 py-1.5 text-sm font-medium text-secondary hover:bg-panel disabled:cursor-not-allowed disabled:opacity-70"
            :disabled="refreshing || loading || !status?.current_task || status.current_task.state === 'paused'"
            @click="pauseIndexing"
          >
            <Loader2 v-if="actionState === 'pausing'" :size="15" class="animate-spin" />
            {{ actionState === 'pausing' ? t("page.status.btn.pausing") : t("page.status.btn.pause") }}
          </button>
          <button
            class="inline-flex items-center gap-2 rounded-md border border-default bg-surface px-3 py-1.5 text-sm font-medium text-secondary hover:bg-panel disabled:cursor-not-allowed disabled:opacity-70"
            :disabled="refreshing || loading || !status?.current_task || status.current_task.state !== 'paused'"
            @click="resumeIndexing"
          >
            <Loader2 v-if="actionState === 'resuming'" :size="15" class="animate-spin" />
            <RefreshCw v-else :size="15" />
            {{ actionState === 'resuming' ? t("page.status.btn.resuming") : t("page.status.btn.resume") }}
          </button>
          <button
            class="inline-flex items-center gap-2 rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-70"
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
          class="rounded-md border border-default bg-panel px-3 py-2"
        >
          <div class="text-[10px] uppercase tracking-wide text-dim">{{ card.label }}</div>
          <div class="mt-1 text-xl font-semibold text-primary">{{ card.value }}</div>
        </div>
      </div>

      <div class="mt-3 grid gap-3 xl:grid-cols-2">
        <div class="rounded-md border border-default bg-surface px-4 py-3">
          <div class="mb-2 flex items-center justify-between gap-3">
            <div>
              <div class="text-[10px] font-semibold uppercase tracking-[0.16em] text-dim">{{ t("page.status.section.incrementalSummary") }}</div>
              <div class="mt-1 text-xs text-dim">{{ t("page.status.section.incrementalDesc") }}</div>
            </div>
            <DocMindBadge tone="default">
              {{ status?.last_run ? status.last_run.completed_at : t("status.noRecentRun") }}
            </DocMindBadge>
          </div>
          <div v-if="status?.last_run" class="grid gap-2 md:grid-cols-2 xl:grid-cols-3">
            <div class="rounded-md bg-panel px-3 py-2">
              <div class="text-[10px] uppercase tracking-wide text-dim">{{ t("page.status.incremental.updated") }}</div>
              <div class="mt-1 text-sm font-semibold text-primary">{{ status.last_run.updated }}</div>
            </div>
            <div class="rounded-md bg-panel px-3 py-2">
              <div class="text-[10px] uppercase tracking-wide text-dim">{{ t("page.status.incremental.skipped") }}</div>
              <div class="mt-1 text-sm font-semibold text-primary">{{ status.last_run.skipped }}</div>
            </div>
            <div class="rounded-md bg-panel px-3 py-2">
              <div class="text-[10px] uppercase tracking-wide text-dim">{{ t("page.status.incremental.deleted") }}</div>
              <div class="mt-1 text-sm font-semibold text-primary">{{ status.last_run.deleted }}</div>
            </div>
            <div class="rounded-md bg-panel px-3 py-2">
              <div class="text-[10px] uppercase tracking-wide text-dim">{{ t("page.status.incremental.scannedCandidates") }}</div>
              <div class="mt-1 text-sm font-semibold text-primary">{{ status.last_run.scanned }}</div>
            </div>
            <div class="rounded-md bg-panel px-3 py-2">
              <div class="text-[10px] uppercase tracking-wide text-dim">{{ t("page.status.incremental.successFail") }}</div>
              <div class="mt-1 text-sm font-semibold text-primary">{{ status.last_run.succeeded }} / {{ status.last_run.failed }}</div>
            </div>
          </div>
          <div v-else class="rounded-md border border-dashed border-default bg-surface px-4 py-6 text-xs text-muted">
            {{ t("page.status.incremental.none") }}
          </div>
        </div>

        <div class="rounded-md border border-default bg-surface px-4 py-3">
          <div class="mb-2 flex items-center justify-between gap-3">
            <div>
              <div class="text-[10px] font-semibold uppercase tracking-[0.16em] text-dim">{{ t("page.status.section.parserStatus") }}</div>
              <div class="mt-1 text-xs text-dim">{{ t("page.status.section.parserDesc") }}</div>
            </div>
          </div>
          <div v-if="parserRuntime" class="grid gap-2 md:grid-cols-2">
            <div class="rounded-md bg-panel px-3 py-2">
              <div class="text-[10px] uppercase tracking-wide text-dim">{{ t("page.status.parser.enabled") }}</div>
              <div class="mt-1 text-sm font-semibold text-primary">{{ parserRuntime.enabled ? t("common.yes") : t("common.no") }}</div>
            </div>
            <div class="rounded-md bg-panel px-3 py-2">
              <div class="text-[10px] uppercase tracking-wide text-dim">{{ t("page.status.parser.available") }}</div>
              <div class="mt-1 text-sm font-semibold text-primary">{{ parserRuntime.available ? t("common.yes") : t("common.no") }}</div>
            </div>
            <div class="rounded-md bg-panel px-3 py-2">
              <div class="text-[10px] uppercase tracking-wide text-dim">{{ t("page.status.parser.pythonBin") }}</div>
              <div class="mt-1 truncate text-sm font-medium text-primary">{{ parserRuntime.python_bin }}</div>
            </div>
            <div class="rounded-md bg-panel px-3 py-2">
              <div class="text-[10px] uppercase tracking-wide text-dim">{{ t("page.status.parser.timeout") }}</div>
              <div class="mt-1 text-sm font-semibold text-primary">{{ parserRuntime.timeout_ms }} ms</div>
            </div>
          </div>
          <div v-if="parserRuntime" class="mt-2 text-xs text-dim">
            {{ t("page.status.parser.script", { path: parserRuntime.script_path }) }}
          </div>
        </div>
      </div>

      <div class="mt-3 rounded-md border border-default bg-surface px-4 py-3">
        <div class="mb-2 flex items-center justify-between gap-3">
          <div>
            <div class="text-[10px] font-semibold uppercase tracking-[0.16em] text-dim">{{ t("page.status.section.indexDirs") }}</div>
            <div class="mt-1 text-xs text-dim">{{ t("page.status.section.indexDirsDesc") }}</div>
          </div>
          <DocMindBadge tone="default">
            <Database class="mr-1" :size="13" />
            {{ explicitIndexDirCount }} {{ t("page.status.section.indexDirs") }}
          </DocMindBadge>
        </div>
        <div class="mt-3 flex flex-col gap-2 lg:flex-row lg:items-stretch">
          <button
            class="inline-flex shrink-0 items-center gap-2 rounded-md border border-default bg-surface px-3 py-2 text-sm font-medium text-secondary hover:bg-panel disabled:cursor-not-allowed disabled:opacity-70"
            :disabled="importing || refreshing || !!busyPath"
            @click="chooseAndAddDir"
          >
            <FolderPlus :size="15" />
            {{ t("page.library.btn.addDir") }}
          </button>
          <div
            class="flex min-w-0 flex-1 items-center gap-3 rounded-md border border-dashed px-4 py-3 text-sm transition"
            :class="dragActive ? 'border-accent bg-accent-soft text-accent-text' : 'border-default bg-panel text-dim'"
          >
            <UploadCloud :size="16" />
            <div class="min-w-0">
              <div class="font-medium">{{ dragActive ? t("page.library.dropActive") : t("page.library.dropHint") }}</div>
              <div v-if="importing" class="mt-0.5 text-xs opacity-80">{{ t("page.library.importing") }}</div>
            </div>
          </div>
        </div>

        <div class="mt-3 rounded-md border border-default bg-surface">
          <div v-if="dirs.length === 0" class="px-4 py-6 text-xs text-muted">
            {{ t("page.status.emptyDirs") }}
          </div>
          <div v-else class="max-h-[420px] overflow-auto p-2">
            <DocMindIndexTree
              :rows="visibleDirRows"
              :selected-path="''"
              :path-tooltip="true"
              :selectable="false"
              :virtual-label="t('common.virtualDir')"
              :expand-title="t('sidebar.expand')"
              :collapse-title="t('sidebar.collapse')"
              density="compact"
              @contextmenu="handleTreeContextMenu"
              @toggle="setDirExpanded"
            />
          </div>
        </div>
        <DocMindContextMenu
          v-if="contextMenuVisible"
          :items="contextMenuItems"
          :x="contextMenuPosition.x"
          :y="contextMenuPosition.y"
          @close="contextMenuVisible = false"
        />
      </div>

      <div v-if="infoMessage" class="mt-3 rounded-md border border-emerald-soft bg-emerald-soft px-4 py-2.5 text-xs text-success">
        {{ infoMessage }}
      </div>

      <div v-if="errorMessage" class="mt-3 rounded-md border border-danger-soft bg-danger-soft px-4 py-2.5 text-xs text-danger">
        {{ errorMessage }}
      </div>

      <div v-if="loading" class="mt-3 rounded-md border border-dashed border-default bg-surface px-4 py-5 text-xs text-muted">
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

      <div class="mt-3 rounded-md border border-default bg-surface px-4 py-3">
        <div class="mb-2 flex items-center gap-2 text-[10px] font-semibold uppercase tracking-[0.16em] text-dim">
          <AlertCircle :size="16" class="text-warning" />
          {{ t("page.status.section.failedFiles") }}
        </div>
        <div v-if="failedGroups.length === 0" class="rounded-md border border-dashed border-default bg-surface px-4 py-6 text-xs text-muted">
          {{ t("page.status.failed.noFailed") }}
        </div>
        <div v-else class="space-y-3">
          <div v-for="group in failedGroups" :key="group.code" class="rounded-md border border-default bg-panel px-3 py-3">
            <div class="mb-2 flex flex-wrap items-center justify-between gap-2">
              <div class="flex items-center gap-2 text-[11px] font-semibold uppercase tracking-wide text-dim">
                <FolderOpen :size="13" />
                {{ group.category }}
                <span class="rounded-full bg-surface px-2 py-0.5 text-[10px] text-muted">{{ group.code }}</span>
                <span class="rounded-full bg-surface px-2 py-0.5 text-[10px] text-muted">{{ group.items.length }}</span>
              </div>
              <button
                class="inline-flex items-center gap-1 rounded-md border border-default bg-surface px-3 py-1 text-xs font-medium text-secondary hover:bg-panel disabled:cursor-not-allowed disabled:opacity-70"
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
                class="flex items-start justify-between gap-3 rounded-md border border-default bg-surface px-3 py-2"
              >
                <div class="min-w-0">
                  <div class="truncate text-sm font-medium text-primary">{{ file.file }}</div>
                  <div class="mt-1 text-xs text-dim">{{ file.reason }}</div>
                  <div class="mt-1 flex flex-wrap gap-2 text-[11px] text-muted">
                    <span>{{ t("page.status.failed.code", { code: file.code }) }}</span>
                    <span>{{ t("page.status.failed.retryCount", { count: file.retry_count }) }}</span>
                    <span>{{ file.last_failed_at }}</span>
                  </div>
                </div>
                <button
                  class="shrink-0 text-xs font-medium text-secondary disabled:cursor-not-allowed disabled:opacity-70"
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
