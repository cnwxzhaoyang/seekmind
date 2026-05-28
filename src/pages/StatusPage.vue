<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { useI18n } from "vue-i18n";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import {
  AlertCircle,
  Loader2,
  RefreshCw,
  FolderOpen,
  FolderPlus,
  UploadCloud,
  Database,
  Cpu,
  Eye,
  Copy,
  FileText,
  ToggleLeft,
  ToggleRight,
  X,
} from "lucide-vue-next";
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
const indexRefreshJobResolvers = new Map<
  string,
  (payload: IndexRefreshProgressView) => void
>();
const indexRefreshJobBufferedEvents = new Map<
  string,
  IndexRefreshProgressView
>();
const documentRefreshResolvers = new Map<
  string,
  (payload: DocumentRefreshProgressView) => void
>();
const documentRefreshBufferedEvents = new Map<
  string,
  DocumentRefreshProgressView
>();
let unlistenIndexRefreshProgress: null | (() => void) = null;
let unlistenDocumentRefreshProgress: null | (() => void) = null;
let unlistenFileDrop: null | (() => void) = null;

const { visibleRows: visibleDirRows, setExpanded: setDirExpanded } =
  useIndexDirTree(dirs);

const explicitIndexDirCount = computed(
  () => dirs.value.filter((dir) => dir.is_explicit).length,
);

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
    errorMessage.value = formatDocmindError(
      error,
      t("page.status.error.loadStatus"),
    );
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
    errorMessage.value = formatDocmindError(
      error,
      t("page.status.error.loadDirs"),
    );
    console.error("[DocMind] loadDirs failed", error);
  }
};

const loadParserRuntime = async () => {
  try {
    parserRuntime.value = await docmindApi.getParserRuntime();
  } catch (error) {
    errorMessage.value = formatDocmindError(
      error,
      t("page.status.error.loadParser"),
    );
    console.error("[DocMind] loadParserRuntime failed", error);
  }
};

const officeNotice = computed(() => {
  if (!parserRuntime.value || parserRuntime.value.office_available) {
    return null;
  }

  return {
    title: t("common.office.warningTitle"),
    desc: t("common.office.warningDesc"),
    hint: t("common.office.warningHint"),
  };
});

const chineseOcrNotice = computed(() => {
  if (!parserRuntime.value?.chinese_ocr_warning) {
    return null;
  }

  return {
    title: t("common.ocr.warningTitle"),
    desc: t("common.ocr.warningDesc"),
    hint: t("common.ocr.warningHint"),
    languages: parserRuntime.value.tesseract_languages.length
      ? parserRuntime.value.tesseract_languages.join(", ")
      : t("common.unknown"),
  };
});

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
      status.value = payload.status;

      if (payload.state === "running") {
        scheduleNextRefresh();
        return;
      }

      const resolver = indexRefreshJobResolvers.get(payload.job_id);
      if (resolver) {
        indexRefreshJobResolvers.delete(payload.job_id);
        resolver(payload);
      } else {
        indexRefreshJobBufferedEvents.set(payload.job_id, payload);
      }

      void refreshDashboard();
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
  if (
    status.value?.current_task &&
    status.value.current_task.state !== "paused"
  ) {
    pollTimer = window.setTimeout(async () => {
      await refreshDashboard();
      if (
        status.value?.current_task &&
        status.value.current_task.state !== "paused"
      ) {
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
    errorMessage.value = formatDocmindError(
      error,
      t("page.status.error.reindex"),
    );
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
    errorMessage.value = formatDocmindError(
      error,
      t("page.status.error.pause"),
    );
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
    errorMessage.value = formatDocmindError(
      error,
      t("page.status.error.resume"),
    );
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
    return {
      label: t("status.idle"),
      tone: "default" as const,
      spinning: false,
    };
  }

  if (actionState.value === "pausing") {
    return {
      label: t("status.pausing"),
      tone: "warning" as const,
      spinning: true,
    };
  }
  if (actionState.value === "resuming") {
    return {
      label: t("status.resuming"),
      tone: "warning" as const,
      spinning: true,
    };
  }
  if (task.state === "paused") {
    return {
      label: t("status.paused"),
      tone: "default" as const,
      spinning: false,
    };
  }
  if (task.state === "running") {
    return {
      label: t("status.running"),
      tone: "warning" as const,
      spinning: true,
    };
  }

  return {
    label: task.state || t("status.processing"),
    tone: "warning" as const,
    spinning: true,
  };
});

const indexProgressPercent = computed(() => {
  const scanned = status.value?.scanned_docs ?? 0;
  if (scanned <= 0) {
    return 0;
  }
  return Math.min(100, Math.round(((status.value?.indexed_docs ?? 0) / scanned) * 100));
});

const retryFailedFile = async (path: string) => {
  retryingTarget.value = path;
  errorMessage.value = "";

  try {
    status.value = await docmindApi.retryFailedFile(path);
  } catch (error) {
    errorMessage.value = formatDocmindError(
      error,
      t("page.status.error.retryFile"),
    );
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
    errorMessage.value = formatDocmindError(
      error,
      t("page.status.error.retryGroup"),
    );
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
    errorMessage.value = formatDocmindError(
      error,
      t("page.status.action.quickLookFailed"),
    );
    console.error("[DocMind] quickLookDir failed", error);
  } finally {
    treeActionTarget.value = null;
  }
};

const copyDirPath = async (path: string) => {
  await copyText(path, t("page.status.action.copiedPath"));
};

const copyDirCitation = async (row: {
  displayName: string;
  fullPath: string;
  dir: IndexDirView;
}) => {
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
    errorMessage.value = formatDocmindError(
      error,
      t("page.library.error.rebuild"),
    );
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
    infoMessage.value = dir.enabled
      ? t("page.library.info.disabled", { path: dir.path })
      : t("page.library.info.enabled", { path: dir.path });
    await refreshDashboard();
  } catch (error) {
    errorMessage.value = formatDocmindError(
      error,
      t("page.library.error.toggleDir"),
    );
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
    errorMessage.value = formatDocmindError(
      error,
      t("page.library.error.deleteDir"),
    );
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
    errorMessage.value = formatDocmindError(
      error,
      t("page.library.error.addDir"),
    );
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
      const started = await docmindApi.refreshDocument(
        file.path,
        file.dir_path,
      );
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
  const normalized = paths
    .map((path) => path.trim())
    .filter((path) => path.length > 0);
  if (normalized.length === 0) {
    return;
  }

  importing.value = true;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    const result: ImportPathsView = await docmindApi.importPaths(normalized);
    const dirsToRefresh = result.added_dirs.filter(
      (path) => path !== result.virtual_dir,
    );
    if (dirsToRefresh.length > 0) {
      infoMessage.value = t("page.library.info.importing", {
        count: normalized.length,
      });
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
      t("page.library.info.importedFiles", {
        count: result.imported_files.length,
      }),
    ];
    if (result.virtual_dir) {
      summaryParts.push(
        t("page.library.info.virtualDir", { path: result.virtual_dir }),
      );
    }
    if (result.unsupported.length > 0) {
      summaryParts.push(
        t("page.library.info.unsupported", {
          count: result.unsupported.length,
        }),
      );
    }
    if (result.skipped.length > 0) {
      summaryParts.push(
        t("page.library.info.skipped", { count: result.skipped.length }),
      );
    }
    infoMessage.value = summaryParts.join(" · ");
    await refreshDashboard();
  } catch (error) {
    errorMessage.value = formatDocmindError(
      error,
      t("page.library.error.importPaths"),
    );
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
  <div class="flex h-full min-h-0 flex-col bg-[#f3f6fb] text-primary">
    <header
      class="flex h-12 shrink-0 items-center justify-between gap-4 border-b border-default bg-white/85 px-5 backdrop-blur"
    >
      <div class="min-w-0">
        <h1 class="text-base font-semibold tracking-tight text-primary">
          {{ t("page.status.title") }}
        </h1>
        <p class="mt-0.5 text-xs text-dim">{{ t("page.status.subtitle") }}</p>
      </div>

      <DocMindBadge
        :tone="parserRuntime?.active === 'python' ? 'success' : 'warning'"
      >
        <Cpu class="mr-1" :size="13" />
        {{
          parserRuntime?.active === "python"
            ? t("status.parser.python")
            : t("status.parser.pythonFallback")
        }}
      </DocMindBadge>
    </header>

    <div
      v-if="officeNotice"
      class="border-b border-amber-soft bg-amber-soft px-5 py-3"
    >
      <div class="mx-auto flex max-w-400 items-start gap-3">
        <AlertCircle :size="16" class="mt-0.5 shrink-0 text-warning" />
        <div class="min-w-0">
          <div class="text-sm font-medium text-warning">
            {{ officeNotice.title }}
          </div>
          <div class="mt-1 text-xs leading-5 text-secondary">
            {{ officeNotice.desc }}
          </div>
          <div class="mt-1 text-xs leading-5 text-dim">
            {{ officeNotice.hint }}
          </div>
        </div>
      </div>
    </div>

    <main class="flex min-h-0 flex-1 overflow-hidden p-5">
      <div class="mx-auto grid h-full min-h-0 w-full max-w-400 gap-4 xl:grid-cols-[420px_minmax(0,1fr)]">
        <!-- 左侧：索引服务控制中心 -->
        <section class="flex h-full min-h-0 flex-col gap-4 overflow-hidden pr-1">
          <div class="flex min-h-0 flex-1 flex-col rounded-xl border border-default bg-white p-4 shadow-sm">
            <div class="mb-4 flex items-start justify-between gap-3">
              <div>
                <div class="flex items-center gap-2">
                  <span
                    class="h-2.5 w-2.5 rounded-full"
                    :class="
                      status?.current_task &&
                      status.current_task.state !== 'paused'
                        ? 'bg-emerald-500'
                        : 'bg-slate-400'
                    "
                  />
                  <h2 class="text-sm font-semibold text-primary">
                    {{
                      status?.current_task
                        ? status.current_task.label
                        : t("status.idle")
                    }}
                  </h2>
                </div>
                <p class="mt-1 text-xs text-dim">
                  {{
                    status?.last_run
                      ? status.last_run.completed_at
                      : t("status.noRecentRun")
                  }}
                </p>
              </div>

              <DocMindBadge :tone="taskDisplayState.tone">
                <Loader2
                  v-if="taskDisplayState.spinning"
                  class="mr-1 animate-spin"
                  :size="13"
                />
                {{ taskDisplayState.label }}
              </DocMindBadge>
            </div>

            <div class="min-h-0 flex-1">
              <DocMindTaskCard
                v-if="status?.current_task"
                :task="status.current_task"
                :title="t('taskCard.defaultTitle')"
                :description="status.current_task.label"
                :badge-label="taskDisplayState.label"
                :badge-tone="taskDisplayState.tone"
                :badge-spinning="taskDisplayState.spinning"
              />
              <div v-else class="flex h-full min-h-[210px] flex-col rounded-lg border border-light bg-[#f7f9fc] p-4">
                <div class="flex items-center justify-between gap-3">
                  <div>
                    <div class="text-[10px] font-semibold uppercase tracking-[0.16em] text-dim">
                      {{ t("taskCard.defaultTitle") }}
                    </div>
                    <div class="mt-1 text-xs text-dim">{{ t("status.noRecentRun") }}</div>
                  </div>
                  <div class="text-2xl font-semibold text-primary">{{ indexProgressPercent }}%</div>
                </div>

                <div class="mt-4 h-1.5 rounded-full bg-badge">
                  <div
                    class="h-1.5 rounded-full bg-accent transition-[width] duration-500"
                    :style="{ width: `${Math.max(indexProgressPercent, status?.scanned_docs ? 6 : 0)}%` }"
                  />
                </div>

                <div class="mt-4 grid grid-cols-2 gap-2">
                  <div class="rounded-lg bg-white px-3 py-2">
                    <div class="text-[10px] uppercase tracking-wide text-dim">{{ t("page.status.stats.scanned") }}</div>
                    <div class="mt-1 text-sm font-semibold text-primary">{{ status?.scanned_docs ?? 0 }}</div>
                  </div>
                  <div class="rounded-lg bg-white px-3 py-2">
                    <div class="text-[10px] uppercase tracking-wide text-dim">{{ t("page.status.stats.indexed") }}</div>
                    <div class="mt-1 text-sm font-semibold text-primary">{{ status?.indexed_docs ?? 0 }}</div>
                  </div>
                  <div class="rounded-lg bg-white px-3 py-2">
                    <div class="text-[10px] uppercase tracking-wide text-dim">{{ t("page.status.stats.chunks") }}</div>
                    <div class="mt-1 text-sm font-semibold text-primary">{{ status?.indexed_chunks ?? 0 }}</div>
                  </div>
                  <div class="rounded-lg bg-white px-3 py-2">
                    <div class="text-[10px] uppercase tracking-wide text-dim">{{ t("page.status.stats.failed") }}</div>
                    <div class="mt-1 text-sm font-semibold text-primary">{{ status?.failed_files ?? 0 }}</div>
                  </div>
                </div>

                <div class="mt-auto pt-4 text-xs text-dim">
                  {{
                    status?.last_run
                      ? status.last_run.completed_at
                      : t("status.noRecentRun")
                  }}
                </div>
              </div>
            </div>

            <div class="mt-4 grid grid-cols-3 gap-2">
              <button
                class="inline-flex items-center justify-center gap-2 rounded-lg border border-default bg-white px-3 py-2 text-xs font-medium text-secondary shadow-sm hover:bg-panel disabled:cursor-not-allowed disabled:opacity-60"
                :disabled="
                  refreshing ||
                  loading ||
                  !status?.current_task ||
                  status.current_task.state === 'paused'
                "
                @click="pauseIndexing"
              >
                <Loader2
                  v-if="actionState === 'pausing'"
                  :size="14"
                  class="animate-spin"
                />
                {{
                  actionState === "pausing"
                    ? t("page.status.btn.pausing")
                    : t("page.status.btn.pause")
                }}
              </button>

              <button
                class="inline-flex items-center justify-center gap-2 rounded-lg border border-default bg-white px-3 py-2 text-xs font-medium text-secondary shadow-sm hover:bg-panel disabled:cursor-not-allowed disabled:opacity-60"
                :disabled="
                  refreshing ||
                  loading ||
                  !status?.current_task ||
                  status.current_task.state !== 'paused'
                "
                @click="resumeIndexing"
              >
                <Loader2
                  v-if="actionState === 'resuming'"
                  :size="14"
                  class="animate-spin"
                />
                <RefreshCw v-else :size="14" />
                {{
                  actionState === "resuming"
                    ? t("page.status.btn.resuming")
                    : t("page.status.btn.resume")
                }}
              </button>

              <button
                class="inline-flex items-center justify-center gap-2 rounded-lg bg-accent px-3 py-2 text-xs font-medium text-white shadow-sm disabled:cursor-not-allowed disabled:opacity-60"
                :disabled="refreshing || loading"
                @click="refreshIndex"
              >
                <RefreshCw :size="14" :class="{ 'animate-spin': refreshing }" />
                {{
                  refreshing
                    ? t("page.status.btn.rebuilding")
                    : t("page.status.btn.reindex")
                }}
              </button>
            </div>
          </div>

          <div class="rounded-xl border border-default bg-white p-4 shadow-sm">
            <div class="mb-3 flex items-center justify-between">
              <div>
                <div
                  class="text-[10px] font-semibold uppercase tracking-[0.16em] text-dim"
                >
                  {{ t("page.status.section.incrementalSummary") }}
                </div>
                <div class="mt-1 text-xs text-dim">
                  {{ t("page.status.section.incrementalDesc") }}
                </div>
              </div>
              <DocMindBadge tone="default">
                {{
                  status?.last_run
                    ? status.last_run.completed_at
                    : t("status.noRecentRun")
                }}
              </DocMindBadge>
            </div>

            <div v-if="status?.last_run" class="grid grid-cols-2 gap-2">
              <div class="rounded-lg bg-[#f7f9fc] px-3 py-2">
                <div class="text-[10px] uppercase tracking-wide text-dim">
                  {{ t("page.status.incremental.updated") }}
                </div>
                <div class="mt-1 text-sm font-semibold">
                  {{ status.last_run.updated }}
                </div>
              </div>
              <div class="rounded-lg bg-[#f7f9fc] px-3 py-2">
                <div class="text-[10px] uppercase tracking-wide text-dim">
                  {{ t("page.status.incremental.skipped") }}
                </div>
                <div class="mt-1 text-sm font-semibold">
                  {{ status.last_run.skipped }}
                </div>
              </div>
              <div class="rounded-lg bg-[#f7f9fc] px-3 py-2">
                <div class="text-[10px] uppercase tracking-wide text-dim">
                  {{ t("page.status.incremental.deleted") }}
                </div>
                <div class="mt-1 text-sm font-semibold">
                  {{ status.last_run.deleted }}
                </div>
              </div>
              <div class="rounded-lg bg-[#f7f9fc] px-3 py-2">
                <div class="text-[10px] uppercase tracking-wide text-dim">
                  {{ t("page.status.incremental.successFail") }}
                </div>
                <div class="mt-1 text-sm font-semibold">
                  {{ status.last_run.succeeded }} / {{ status.last_run.failed }}
                </div>
              </div>
            </div>

            <div
              v-else
              class="rounded-lg border border-dashed border-default bg-[#f7f9fc] px-4 py-6 text-xs text-muted"
            >
              {{ t("page.status.incremental.none") }}
            </div>
          </div>

          <div class="rounded-xl border border-default bg-white p-4 shadow-sm">
            <div class="mb-3">
              <div
                class="text-[10px] font-semibold uppercase tracking-[0.16em] text-dim"
              >
                {{ t("page.status.section.parserStatus") }}
              </div>
              <div class="mt-1 text-xs text-dim">
                {{ t("page.status.section.parserDesc") }}
              </div>
            </div>

            <div v-if="parserRuntime" class="grid grid-cols-2 gap-2">
              <div class="rounded-lg bg-[#f7f9fc] px-3 py-2">
                <div class="text-[10px] uppercase tracking-wide text-dim">
                  {{ t("page.status.parser.enabled") }}
                </div>
                <div class="mt-1 text-sm font-semibold">
                  {{ parserRuntime.enabled ? t("common.yes") : t("common.no") }}
                </div>
              </div>
              <div class="rounded-lg bg-[#f7f9fc] px-3 py-2">
                <div class="text-[10px] uppercase tracking-wide text-dim">
                  {{ t("page.status.parser.available") }}
                </div>
                <div class="mt-1 text-sm font-semibold">
                  {{
                    parserRuntime.available ? t("common.yes") : t("common.no")
                  }}
                </div>
              </div>
              <div class="rounded-lg bg-[#f7f9fc] px-3 py-2">
                <div class="text-[10px] uppercase tracking-wide text-dim">
                  {{ t("page.status.parser.pythonBin") }}
                </div>
                <div class="mt-1 truncate text-sm font-medium">
                  {{ parserRuntime.python_bin }}
                </div>
              </div>
              <div class="rounded-lg bg-[#f7f9fc] px-3 py-2">
                <div class="text-[10px] uppercase tracking-wide text-dim">
                  {{ t("page.status.parser.timeout") }}
                </div>
                <div class="mt-1 text-sm font-semibold">
                  {{ parserRuntime.timeout_ms }} ms
                </div>
              </div>
              <div class="rounded-lg bg-[#f7f9fc] px-3 py-2">
                <div class="text-[10px] uppercase tracking-wide text-dim">
                  {{ t("page.status.parser.systemLanguage") }}
                </div>
                <div class="mt-1 text-sm font-semibold">
                  {{ parserRuntime.system_locale || t("common.unknown") }}
                </div>
                <div class="mt-1 text-xs text-dim">
                  {{ t("page.status.parser.systemLanguageHint", { language: parserRuntime.system_language }) }}
                </div>
              </div>
              <div class="rounded-lg bg-[#f7f9fc] px-3 py-2">
                <div class="text-[10px] uppercase tracking-wide text-dim">
                  {{ t("page.status.parser.ocrLanguages") }}
                </div>
                <div class="mt-1 text-sm font-semibold">
                  {{
                    parserRuntime.tesseract_languages.length
                      ? parserRuntime.tesseract_languages.join(", ")
                      : t("common.none")
                  }}
                </div>
                <div class="mt-1 text-xs text-dim">
                  {{ t("page.status.parser.ocrAvailability", { status: parserRuntime.chinese_ocr_available ? t("common.available") : t("common.unavailable") }) }}
                </div>
              </div>
              <div class="col-span-2 rounded-lg border border-default bg-[#f7f9fc] px-3 py-2">
                <div class="text-[10px] uppercase tracking-wide text-dim">
                  {{ t("common.office.runtimeLabel") }}
                </div>
                <div class="mt-1 text-sm font-semibold">
                  {{
                    parserRuntime.office_available
                      ? t("common.office.runtimeAvailable", {
                          bin: parserRuntime.office_bin || t("common.available"),
                        })
                      : t("common.office.runtimeUnavailable")
                  }}
                </div>
                <div class="mt-1 text-xs leading-5 text-dim">
                  {{ t("common.office.runtimePlatform", { platform: parserRuntime.office_platform || t("common.unknown") }) }}
                </div>
                <div v-if="!parserRuntime.office_available" class="mt-1 text-xs leading-5 text-warning">
                  {{ parserRuntime.office_message || t("common.office.warningHint") }}
                </div>
              </div>
              <div
                v-if="chineseOcrNotice"
                class="col-span-2 rounded-lg border border-amber-200 bg-amber-50 px-3 py-2"
              >
                <div class="text-[10px] uppercase tracking-wide text-amber-700">
                  {{ chineseOcrNotice.title }}
                </div>
                <div class="mt-1 text-sm font-semibold text-amber-900">
                  {{ chineseOcrNotice.desc }}
                </div>
                <div class="mt-1 text-xs leading-5 text-amber-800">
                  {{ t("page.status.parser.ocrInstalled", { languages: chineseOcrNotice.languages }) }}
                </div>
                <div class="mt-1 text-xs leading-5 text-amber-700">
                  {{ chineseOcrNotice.hint }}
                </div>
              </div>
            </div>

            <div v-if="parserRuntime" class="mt-3 truncate text-xs text-dim">
              {{
                t("page.status.parser.script", {
                  path: parserRuntime.script_path,
                })
              }}
            </div>
          </div>
        </section>

        <!-- 右侧：目录 / 导入 / 失败处理 -->
        <section class="grid h-full min-h-0 grid-rows-[2fr_1fr] gap-4 overflow-hidden">
          <!-- 索引目录 -->
          <div class="flex min-h-0 flex-col rounded-xl border border-default bg-white p-4 shadow-sm">
            <div class="mb-3 flex shrink-0 items-center justify-between gap-3">
              <div>
                <div class="text-[10px] font-semibold uppercase tracking-[0.16em] text-dim">
                  {{ t("page.status.section.indexDirs") }}
                </div>
                <div class="mt-1 text-xs text-dim">
                  {{ t("page.status.section.indexDirsDesc") }}
                </div>
              </div>

              <DocMindBadge tone="default">
                <Database class="mr-1" :size="13" />
                {{ explicitIndexDirCount }}
                {{ t("page.status.section.indexDirs") }}
              </DocMindBadge>
            </div>

            <div class="mb-3 flex shrink-0 flex-col gap-2 lg:flex-row lg:items-stretch">
              <button
                  class="inline-flex shrink-0 items-center gap-2 rounded-lg border border-default bg-white px-3 py-2 text-sm font-medium text-secondary shadow-sm hover:bg-panel disabled:cursor-not-allowed disabled:opacity-60"
                  :disabled="importing || refreshing || !!busyPath"
                  @click="chooseAndAddDir"
              >
                <FolderPlus :size="15" />
                {{ t("page.library.btn.addDir") }}
              </button>

              <div
                  class="flex min-w-0 flex-1 items-center gap-3 rounded-lg border border-dashed px-4 py-3 text-sm transition"
                  :class="
          dragActive
            ? 'border-accent bg-accent-soft text-accent-text'
            : 'border-default bg-[#f7f9fc] text-dim'
        "
              >
                <UploadCloud :size="16" />
                <div class="min-w-0">
                  <div class="font-medium">
                    {{ dragActive ? t("page.library.dropActive") : t("page.library.dropHint") }}
                  </div>
                  <div v-if="importing" class="mt-0.5 text-xs opacity-80">
                    {{ t("page.library.importing") }}
                  </div>
                </div>
              </div>
            </div>

            <div class="min-h-0 flex-1 overflow-hidden rounded-lg border border-default bg-[#f7f9fc]">
              <div
                  v-if="dirs.length === 0"
                  class="flex h-full items-center px-4 py-8 text-xs text-muted"
              >
                {{ t("page.status.emptyDirs") }}
              </div>

              <div v-else class="h-full overflow-auto p-2">
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
          </div>

          <!-- 解析失败 -->
          <div class="flex min-h-0 flex-col rounded-xl border border-default bg-white p-4 shadow-sm">
            <div class="mb-3 flex shrink-0 items-center gap-2 text-[10px] font-semibold uppercase tracking-[0.16em] text-dim">
              <AlertCircle :size="16" class="text-warning" />
              {{ t("page.status.section.failedFiles") }}
            </div>

            <div
                v-if="failedGroups.length === 0"
                class="flex min-h-0 flex-1 items-center rounded-lg border border-dashed border-default bg-[#f7f9fc] px-4 py-8 text-xs text-muted"
            >
              {{ t("page.status.failed.noFailed") }}
            </div>

            <div v-else class="min-h-0 flex-1 space-y-3 overflow-auto pr-1">
              <!-- 你原来的 failedGroups 内容保持不变 -->
            </div>
          </div>

          <DocMindContextMenu
              v-if="contextMenuVisible"
              :items="contextMenuItems"
              :x="contextMenuPosition.x"
              :y="contextMenuPosition.y"
              @close="contextMenuVisible = false"
          />
        </section>
      </div>
    </main>
  </div>
</template>
