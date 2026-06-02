<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { useI18n } from "vue-i18n";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import {
  AlertCircle,
  Eye,
  Copy,
  FileText,
  RefreshCw,
  ToggleLeft,
  ToggleRight,
  X,
  FolderPlus,
  UploadCloud,
} from "lucide-vue-next";
import SvgIcon from "../components/SvgIcon.vue";
import DocMindContextMenu from "../components/docmind/DocMindContextMenu.vue";
import type { ContextMenuItem } from "../components/docmind/DocMindContextMenu.vue";
import DocMindIndexTree from "../components/docmind/DocMindIndexTree.vue";
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
const nowTs = ref(Date.now());
let pollTimer: number | null = null;
let timeTicker: number | null = null;
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

const ocrLanguagesPreview = computed(() => {
  if (!parserRuntime.value?.tesseract_languages.length) {
    return t("common.none");
  }

  const languages = parserRuntime.value.tesseract_languages;
  const previewCount = 8;
  const preview = languages.slice(0, previewCount).join(", ");
  if (languages.length > previewCount) {
    return `${preview}, …`;
  }

  return preview;
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

const lastUpdateTime = ref(new Date().toLocaleString());

// Teleport 到 body 的菜单使用全局 CSS 变量，无需额外处理

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
  lastUpdateTime.value = new Date().toLocaleString();
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
      spinning: false,
    };
  }

  if (actionState.value === "pausing") {
    return {
      label: t("status.pausing"),
      spinning: true,
    };
  }
  if (actionState.value === "resuming") {
    return {
      label: t("status.resuming"),
      spinning: true,
    };
  }
  if (task.state === "paused") {
    return {
      label: t("status.paused"),
      spinning: false,
    };
  }
  if (task.state === "running") {
    return {
      label: t("status.running"),
      spinning: true,
    };
  }

  return {
    label: task.state || t("status.processing"),
    spinning: true,
  };
});

const formatDuration = (totalSeconds: number) => {
  if (!Number.isFinite(totalSeconds) || totalSeconds < 0) {
    return "--:--:--";
  }

  const seconds = Math.floor(totalSeconds);
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remainder = seconds % 60;
  const clock = [hours, minutes, remainder]
    .map((value) => String(value).padStart(2, "0"))
    .join(":");

  return days > 0 ? `${days}d ${clock}` : clock;
};

const currentTaskStartedAt = computed(() => status.value?.current_task?.started_at ?? 0);

const currentTaskStartTimeText = computed(() => {
  if (!currentTaskStartedAt.value) {
    return "--";
  }

  return new Date(currentTaskStartedAt.value * 1000).toLocaleString();
});

const currentTaskDurationText = computed(() => {
  if (!currentTaskStartedAt.value) {
    return "--:--:--";
  }

  const elapsedSeconds = Math.max(
    0,
    Math.floor(nowTs.value / 1000) - currentTaskStartedAt.value,
  );
  return formatDuration(elapsedSeconds);
});

const indexProgressPercent = computed(() => {
  const scanned = status.value?.scanned_docs ?? 0;
  if (scanned <= 0) {
    return 0;
  }
  return Math.min(100, Math.round(((status.value?.indexed_docs ?? 0) / scanned) * 100));
});

const pendingCount = computed(() => {
  const task = status.value?.current_task;
  if (!task) return 0;
  return Math.max(task.total - task.scanned, 0);
});

const errorTypeList = computed(() => {
  if (!status.value?.failed_items?.length) return [];

  const grouped = new Map<string, number>();
  for (const item of status.value.failed_items) {
    const key = item.category || item.code || t("page.status.exception.unknownType");
    grouped.set(key, (grouped.get(key) ?? 0) + 1);
  }

  const total = status.value.failed_items.length;
  return [...grouped.entries()]
    .map(([name, count]) => ({
      name,
      count,
      percentage: Math.round((count / total) * 1000) / 10,
    }))
    .sort((a, b) => b.count - a.count);
});

const latestExceptions = computed(() => {
  return (status.value?.failed_items ?? []).slice(0, 5).map((item) => ({
    file: item.file,
    type: item.category || item.code || t("page.status.exception.unknownType"),
    time: "--",
    message: item.message,
    traceback: "",
  }));
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
  timeTicker = window.setInterval(() => {
    nowTs.value = Date.now();
  }, 1000);
});

onBeforeUnmount(() => {
  stopPolling();
  if (timeTicker !== null) {
    window.clearInterval(timeTicker);
    timeTicker = null;
  }
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
  <div class="index-status-panel">
    <!-- 头部 -->
    <div class="panel-header">
      <div class="header-left">
        <div class="header-title">
          <span class="title-icon"><SvgIcon icon="icon-database" size="lg" /></span>
          <h1>{{ t("page.status.title") }}</h1>
        </div>
        <p class="header-description">{{ t("page.status.subtitle") }}</p>
      </div>
      <div class="header-right">
        <div
          class="status-badge"
          :class="taskDisplayState.spinning ? 'status-running' : 'status-idle'"
        >
          <span
            class="status-dot"
            :class="{ 'status-dot-active': taskDisplayState.spinning }"
          ></span>
          {{ taskDisplayState.label }}
        </div>
        <div class="last-update">
          <span class="update-icon"><SvgIcon icon="icon-clock" size="sm" /></span>
          {{ t("page.status.lastUpdate") }}：{{ lastUpdateTime }}
        </div>
        <button
          class="refresh-btn"
          :disabled="dashboardRefreshing"
          @click="syncDashboardState"
          title="刷新状态"
        >
          <SvgIcon icon="icon-refresh" size="md" />
        </button>
      </div>
    </div>

    <!-- 修复：header 固定，内容区独立滚动，避免右侧滚动条把头部带走。 -->
    <main class="panel-scroll">
      <!-- Office 提示 -->
      <div v-if="officeNotice" class="office-notice-banner">
        <AlertCircle :size="16" class="shrink-0 office-notice-icon" />
        <div class="min-w-0">
          <div class="office-notice-title">{{ officeNotice.title }}</div>
          <div class="office-notice-desc">{{ officeNotice.desc }}</div>
          <div class="office-notice-hint">{{ officeNotice.hint }}</div>
        </div>
      </div>

      <!-- 主容器 -->
      <div class="panel-content">
        <!-- 左侧面板 -->
        <div class="left-panel">
        <!-- 索引控制 -->
        <div class="card">
          <div class="card-header">
            <span class="card-icon"><SvgIcon icon="icon-settings" size="lg" /></span>
            <h2>{{ t("page.status.section.control") }}</h2>
          </div>
          <p class="card-description">{{ t("page.status.section.controlDesc") }}</p>
          <div class="control-buttons">
            <button
              class="btn btn-secondary"
              :disabled="
                refreshing ||
                loading ||
                !status?.current_task ||
                status.current_task.state === 'paused'
              "
              @click="pauseIndexing"
            >
              <span class="btn-icon"><SvgIcon icon="icon-pause" size="sm" /></span>
              {{
                actionState === "pausing"
                  ? t("page.status.btn.pausing")
                  : t("page.status.btn.pause")
              }}
            </button>
            <button
              class="btn btn-secondary"
              :disabled="
                refreshing ||
                loading ||
                !status?.current_task ||
                status.current_task.state !== 'paused'
              "
              @click="resumeIndexing"
            >
              <span class="btn-icon"><SvgIcon icon="icon-play" size="sm" /></span>
              {{
                actionState === "resuming"
                  ? t("page.status.btn.resuming")
                  : t("page.status.btn.resume")
              }}
            </button>
            <button
              class="btn btn-primary"
              :disabled="refreshing || loading"
              @click="refreshIndex"
            >
              <span class="btn-icon"><SvgIcon icon="icon-refresh" size="sm" /></span>
              {{
                refreshing
                  ? t("page.status.btn.rebuilding")
                  : t("page.status.btn.reindex")
              }}
            </button>
          </div>
        </div>

        <!-- 统计信息 -->
        <div class="card">
          <div class="card-header">
            <span class="card-icon"><SvgIcon icon="icon-chart" size="lg" /></span>
            <h2>{{ t("page.status.section.statistics") }}</h2>
          </div>
          <div class="stats-grid">
            <div class="stat-item">
              <span class="stat-icon indexed"><SvgIcon icon="icon-document" size="md" /></span>
              <div class="stat-content">
                <div class="stat-label">{{ t("page.status.stats.indexedFiles") }}</div>
                <div class="stat-value">{{ status?.indexed_docs ?? 0 }}</div>
                <div class="stat-desc">{{ t("page.status.stats.indexedFilesDesc") }}</div>
              </div>
            </div>
            <div class="stat-item">
              <span class="stat-icon"><SvgIcon icon="icon-folder" size="md" /></span>
              <div class="stat-content">
                <div class="stat-label">{{ t("page.status.stats.indexedCount") }}</div>
                <div class="stat-value">{{ status?.scanned_docs ?? 0 }}</div>
                <div class="stat-desc">{{ t("page.status.stats.indexedCountDesc") }}</div>
              </div>
            </div>
            <div class="stat-item">
              <span class="stat-icon error"><SvgIcon icon="icon-warning" size="md" /></span>
              <div class="stat-content">
                <div class="stat-label">{{ t("page.status.stats.errorCount") }}</div>
                <div class="stat-value error-value">{{ status?.failed_files ?? 0 }}</div>
                <div class="stat-desc">{{ t("page.status.stats.errorCountDesc") }}</div>
              </div>
            </div>
            <div class="stat-item">
              <span class="stat-icon pending"><SvgIcon icon="icon-file" size="md" /></span>
              <div class="stat-content">
                <div class="stat-label">{{ t("page.status.stats.pendingFiles") }}</div>
                <div class="stat-value">{{ pendingCount }}</div>
                <div class="stat-desc">{{ t("page.status.stats.pendingFilesDesc") }}</div>
              </div>
            </div>
          </div>
        </div>

        <!-- 索引进度 -->
        <div class="card">
          <div class="card-header">
            <span class="card-icon"><SvgIcon icon="icon-clock" size="lg" /></span>
            <h2>{{ t("page.status.section.progress") }}</h2>
          </div>

          <div class="progress-section">
            <div class="progress-label">
              <span>{{ t("page.status.progress.overall") }}</span>
              <span class="progress-percentage">{{ indexProgressPercent }}%</span>
            </div>
            <div class="progress-bar">
              <div
                class="progress-fill"
                :style="{ width: indexProgressPercent + '%' }"
              ></div>
            </div>
            <div class="progress-current">
              {{ t("page.status.progress.currentFile") }}：
              <span class="file-name-highlight">{{
                status?.current_task?.current_file ?? "-"
              }}</span>
            </div>
          </div>

          <div class="time-info">
            <div class="time-item">
              <span class="time-icon"><SvgIcon icon="icon-clock" size="sm" /></span>
              <div>
                <div class="time-label">{{ t("page.status.progress.duration") }}</div>
                <div class="time-value">{{ currentTaskDurationText }}</div>
              </div>
            </div>
            <div class="time-item">
              <span class="time-icon"><SvgIcon icon="icon-clock" size="sm" /></span>
              <div>
                <div class="time-label">{{ t("page.status.progress.startTime") }}</div>
                <div class="time-value">{{ currentTaskStartTimeText }}</div>
              </div>
            </div>
          </div>

          <div class="status-stats">
            <div class="status-stat success">
              <span class="stat-icon"><SvgIcon icon="icon-success" size="sm" /></span>
              <div>
                <div class="stat-label">{{ t("page.status.progress.success") }}</div>
                <div class="stat-value">{{ status?.current_task?.succeeded ?? 0 }}</div>
              </div>
            </div>
            <div class="status-stat error">
              <span class="stat-icon"><SvgIcon icon="icon-error" size="sm" /></span>
              <div>
                <div class="stat-label">{{ t("page.status.progress.failed") }}</div>
                <div class="stat-value">{{ status?.current_task?.failed ?? 0 }}</div>
              </div>
            </div>
            <div class="status-stat skipped">
              <span class="stat-icon">⊘</span>
              <div>
                <div class="stat-label">{{ t("page.status.progress.skipped") }}</div>
                <div class="stat-value">{{ status?.current_task?.skipped ?? 0 }}</div>
              </div>
            </div>
          </div>
        </div>

        <!-- 错误摘要 -->
        <div class="card">
          <div class="card-header">
            <!-- 对齐原型：错误摘要使用红色错误语义图标。 -->
            <span class="card-icon error"><SvgIcon icon="icon-error" size="lg" /></span>
            <h2>{{ t("page.status.section.errorSummary") }}</h2>
          </div>

          <div class="error-circle-container">
            <svg class="error-circle" viewBox="0 0 100 100">
              <circle cx="50" cy="50" r="45" class="circle-bg"></circle>
              <circle
                cx="50"
                cy="50"
                r="45"
                class="circle-progress"
                :style="{
                  strokeDashoffset:
                    282.7 -
                    (282.7 *
                      Math.min(errorTypeList.reduce((s, e) => s + e.count, 0), 100)) /
                      100,
                }"
              ></circle>
            </svg>
            <div class="error-text">
              <div class="error-count">{{ status?.failed_files ?? 0 }}</div>
              <div class="error-label">{{ t("page.status.errorSummary.totalErrors") }}</div>
            </div>
          </div>

          <div class="error-types">
            <div class="error-type-header">{{ t("page.status.errorSummary.errorType") }}</div>
            <div
              v-for="err in errorTypeList"
              :key="err.name"
              class="error-type-item"
            >
              <div class="error-type-name" :title="err.name">{{ err.name }}</div>
              <div class="error-type-bar">
                <div
                  class="error-bar-fill"
                  :style="{ width: err.percentage + '%' }"
                ></div>
              </div>
              <div class="error-type-count">{{ err.count }} ({{ err.percentage }}%)</div>
            </div>
            <div v-if="errorTypeList.length === 0" class="error-type-empty">-</div>
          </div>

          <div v-if="parserRuntime" class="environment-info">
            <div class="env-item">
              <span class="env-label">{{ t("page.status.errorSummary.pythonVersion") }}</span>
              <span class="env-value">{{ parserRuntime.python_bin }}</span>
            </div>
            <div class="env-item">
              <span class="env-label">{{ t("page.status.errorSummary.timeout") }}</span>
              <span class="env-value">{{ parserRuntime.timeout_ms }} ms</span>
            </div>
          </div>
        </div>
      </div>

      <!-- 右侧面板 -->
      <div class="right-panel">
        <!-- 索引信息 -->
        <div class="card info-card-large">
          <div class="card-header">
            <span class="card-icon"><SvgIcon icon="icon-info" size="lg" /></span>
            <h2>{{ t("page.status.section.indexInfo") }}</h2>
            <span class="file-count">{{ dirs.length }} 个目录</span>
          </div>

          <div class="dir-tree-toolbar">
            <button
              class="btn btn-secondary btn-sm"
              :disabled="importing || refreshing || !!busyPath"
              @click="chooseAndAddDir"
            >
              <FolderPlus :size="14" />
              {{ t("page.library.btn.addDir") }}
            </button>
            <div
              class="drop-zone"
              :class="dragActive ? 'drop-zone-active' : ''"
            >
              <UploadCloud :size="14" />
              <span>{{ dragActive ? t("page.library.dropActive") : t("page.library.dropHint") }}</span>
            </div>
          </div>

          <div class="dir-tree-container">
            <div
              v-if="dirs.length === 0"
              class="dir-list-empty"
            >
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
              @contextmenu="handleTreeContextMenu"
              @toggle="setDirExpanded"
            />
          </div>
        </div>

        <!-- 最新异常 -->
        <div class="card">
          <div class="card-header">
            <!-- 对齐原型：最新异常使用警告图标，但保留异常红色语义。 -->
            <span class="card-icon error"><SvgIcon icon="icon-warning" size="lg" /></span>
            <h2>{{ t("page.status.section.latestException") }}</h2>
            <span class="exception-count">{{ status?.failed_items?.length ?? 0 }}</span>
          </div>

          <div v-if="latestExceptions.length > 0" class="exception-content">
            <div
              v-for="(item, idx) in latestExceptions.slice(0, 1)"
              :key="idx"
              class="exception-item"
            >
              <div class="exception-header">
                <span class="exception-file">{{ t("page.status.exception.errorFile") }}：{{ item.file }}</span>
                <span class="exception-type-tag">{{ item.type }}</span>
              </div>
              <div class="exception-time">
                {{ t("page.status.exception.exceptionTime") }}：{{ item.time }}
              </div>
              <div v-if="item.message" class="exception-message">
                {{ item.message }}
              </div>
            </div>
          </div>

          <div v-else class="exception-empty">
            <span class="empty-icon"><SvgIcon icon="icon-success" size="lg" /></span>
            <p>{{ t("page.status.exception.noException") }}</p>
          </div>

          <div class="exception-footer">
            <button class="view-all-btn" @click="syncDashboardState">
              {{ t("page.status.exception.viewAll") }}
            </button>
          </div>
        </div>
      </div>
      </div>
    </main>

    <DocMindContextMenu
      v-if="contextMenuVisible"
      :items="contextMenuItems"
      :x="contextMenuPosition.x"
      :y="contextMenuPosition.y"
      @close="contextMenuVisible = false"
    />
  </div>
</template>

<style scoped>
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

.index-status-panel {
  background-color: var(--color-page-bg);
  color: var(--color-text-primary);
  min-height: 0;
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

/* Office 提示 */
.office-notice-banner {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 12px 16px;
  margin-bottom: 16px;
  background-color: rgba(187, 128, 9, 0.15);
  border: 1px solid rgba(187, 128, 9, 0.4);
  border-radius: 6px;
}
.office-notice-icon {
  color: var(--color-warning);
  margin-top: 2px;
}
.office-notice-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--color-warning);
}
.office-notice-desc {
  font-size: 12px;
  color: var(--color-text-secondary);
  margin-top: 4px;
  line-height: 1.5;
}
.office-notice-hint {
  font-size: 12px;
  color: var(--color-text-secondary);
  margin-top: 4px;
}

/* 头部 */
.panel-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
  min-height: 48px;
  padding: 0 20px;
  flex-shrink: 0;
  border-bottom: 1px solid var(--color-border);
  background-color: var(--color-header-bg);
}

.header-left {
  flex: 1;
  min-width: 0;
}

.header-title {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 0;
}

.title-icon {
  display: inline-flex;
  align-items: center;
  color: var(--color-accent);
}

.header-title h1 {
  font-size: 16px;
  line-height: 1.25;
  font-weight: 600;
  letter-spacing: -0.01em;
  color: var(--color-text-primary);
  margin: 0;
}

.header-description {
  font-size: 13px;
  line-height: 1.35;
  color: var(--color-text-secondary);
  margin: 0;
}

.header-right {
  display: flex;
  align-items: center;
  gap: 16px;
  flex-shrink: 0;
}

.status-badge {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  border-radius: 6px;
  font-size: 13px;
  font-weight: 600;
  white-space: nowrap;
}

.status-badge.status-running {
  background-color: var(--color-emerald-soft);
  color: var(--color-success);
  border: 1px solid var(--color-success);
}

.status-badge.status-idle {
  background-color: var(--color-badge-bg);
  color: var(--color-text-secondary);
  border: 1px solid var(--color-border);
}

.status-dot {
  display: inline-block;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background-color: var(--color-success);
}

.status-dot.status-dot-active {
  animation: pulse 2s infinite;
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.3; }
}

.last-update {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--color-text-secondary);
  white-space: nowrap;
}

.update-icon {
  display: inline-flex;
  align-items: center;
}

.refresh-btn {
  width: 36px;
  height: 36px;
  border: 1px solid var(--color-border);
  background-color: var(--color-surface);
  border-radius: 6px;
  color: var(--color-text-secondary);
  cursor: pointer;
  font-size: 16px;
  transition: all 0.2s;
  display: flex;
  align-items: center;
  justify-content: center;
}

.refresh-btn:hover:not(:disabled) {
  background-color: var(--color-surface-active);
  border-color: var(--color-accent);
  color: var(--color-accent);
}

.refresh-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* 内容滚动区 */
.panel-scroll {
  min-height: 0;
  flex: 1;
  overflow-y: auto;
  padding: 20px;
}

/* 主容器 */
.panel-content {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 20px;
  align-items: start;
}

.left-panel,
.right-panel {
  display: flex;
  flex-direction: column;
  gap: 20px;
}

/* 卡片通用 */
.card {
  background-color: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: 8px;
  padding: 20px;
}

.card-header {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 16px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--color-border);
}

.card-icon {
  display: inline-flex;
  align-items: center;
}

.card-header h2 {
  font-size: 15px;
  font-weight: 600;
  color: var(--color-text-primary);
  flex: 1;
  margin: 0;
}

.card-description {
  font-size: 13px;
  color: var(--color-text-secondary);
  margin-bottom: 16px;
  line-height: 1.5;
}

.file-count,
.exception-count {
  font-size: 12px;
  color: var(--color-text-secondary);
  background-color: var(--color-page-bg);
  padding: 3px 8px;
  border-radius: 4px;
  white-space: nowrap;
}

/* 控制按钮 */
.control-buttons {
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
}

.btn {
  padding: 8px 14px;
  border-radius: 6px;
  border: 1px solid var(--color-border);
  background-color: var(--color-page-bg);
  color: var(--color-text-primary);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  transition: all 0.2s;
  white-space: nowrap;
}

.btn:hover:not(:disabled) {
  background-color: var(--color-surface-active);
  border-color: var(--color-accent);
  color: var(--color-accent);
}

.btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.btn-primary {
  background-color: var(--color-accent);
  border-color: var(--color-accent);
  color: white;
}

.btn-primary:hover:not(:disabled) {
  background-color: var(--color-accent-text);
  border-color: var(--color-accent-text);
}

.btn-sm {
  padding: 6px 10px;
  font-size: 12px;
}

.btn-icon {
  display: inline-flex;
  align-items: center;
}

/* 统计信息 */
.stats-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 12px;
}

.stat-item {
  display: flex;
  gap: 12px;
  padding: 12px;
  background-color: var(--color-page-bg);
  border-radius: 6px;
  border: 1px solid var(--color-border);
}

.stat-icon {
  width: 40px;
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 6px;
  background-color: var(--color-surface-active);
  flex-shrink: 0;
}

.stat-icon.indexed {
  background-color: var(--color-accent-soft);
}

.stat-icon.error {
  background-color: var(--color-danger-soft);
}

.stat-icon.pending {
  background-color: var(--color-amber-soft);
}

.stat-content {
  flex: 1;
  min-width: 0;
}

.stat-label {
  font-size: 11px;
  color: var(--color-text-secondary);
  margin-bottom: 4px;
}

.stat-value {
  font-size: 22px;
  font-weight: 600;
  color: var(--color-text-primary);
  margin-bottom: 2px;
  line-height: 1.2;
}

.stat-value.error-value {
  color: var(--color-danger);
}

.stat-desc {
  font-size: 11px;
  color: var(--color-text-dim);
}

/* 进度条 */
.progress-section {
  margin-bottom: 16px;
}

.progress-label {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 8px;
  font-size: 13px;
  color: var(--color-text-primary);
}

.progress-percentage {
  font-weight: 600;
  color: var(--color-accent);
}

.progress-bar {
  width: 100%;
  height: 8px;
  background-color: var(--color-page-bg);
  border-radius: 4px;
  overflow: hidden;
  margin-bottom: 8px;
}

.progress-fill {
  height: 100%;
  background: linear-gradient(90deg, var(--color-accent), var(--color-accent-text));
  border-radius: 4px;
  transition: width 0.3s ease;
}

.progress-current {
  font-size: 12px;
  color: var(--color-text-secondary);
}

.file-name-highlight {
  color: var(--color-accent);
  font-weight: 500;
}

/* 时间信息 */
.time-info {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
  margin-bottom: 16px;
  padding: 12px;
  background-color: var(--color-page-bg);
  border-radius: 6px;
}

.time-item {
  display: flex;
  gap: 8px;
  align-items: flex-start;
}

.time-icon {
  display: inline-flex;
  align-items: center;
  margin-top: 2px;
}

.time-label {
  font-size: 11px;
  color: var(--color-text-secondary);
}

.time-value {
  font-size: 13px;
  color: var(--color-text-primary);
  font-weight: 500;
  margin-top: 2px;
}

/* 状态统计 */
.status-stats {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 10px;
}

.status-stat {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px;
  background-color: var(--color-page-bg);
  border-radius: 6px;
  border-left: 3px solid;
}

.status-stat.success {
  border-left-color: var(--color-success);
}

.status-stat.error {
  border-left-color: var(--color-danger);
}

.status-stat.skipped {
  border-left-color: var(--color-warning);
}

.status-stat .stat-icon {
  width: 24px;
  height: 24px;
  background: none;
  display: flex;
  align-items: center;
  justify-content: center;
}

.status-stat.success .stat-icon {
  color: var(--color-success);
}

.status-stat.error .stat-icon {
  color: var(--color-danger);
}

.status-stat.skipped .stat-icon {
  color: var(--color-warning);
}

.status-stat .stat-label {
  font-size: 11px;
  color: var(--color-text-secondary);
  margin-bottom: 0;
}

.status-stat .stat-value {
  font-size: 16px;
  color: var(--color-text-primary);
  margin: 0;
}

/* 错误圆形图 */
.error-circle-container {
  position: relative;
  width: 120px;
  height: 120px;
  margin: 0 auto 16px;
}

.error-circle {
  width: 100%;
  height: 100%;
}

.circle-bg {
  fill: none;
  stroke: var(--color-surface-active);
  stroke-width: 8;
}

.circle-progress {
  fill: none;
  stroke: var(--color-danger);
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
  color: var(--color-danger);
  line-height: 1.2;
}

.error-label {
  font-size: 11px;
  color: var(--color-text-secondary);
}

/* 错误类型 */
.error-types {
  margin-bottom: 16px;
}

.error-type-header {
  font-size: 12px;
  font-weight: 600;
  color: var(--color-text-primary);
  margin-bottom: 10px;
}

.error-type-item {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 6px;
}

.error-type-name {
  font-size: 12px;
  color: var(--color-text-secondary);
  min-width: 120px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.error-type-bar {
  flex: 1;
  height: 6px;
  background-color: var(--color-page-bg);
  border-radius: 3px;
  overflow: hidden;
}

.error-bar-fill {
  height: 100%;
  background-color: var(--color-danger);
  transition: width 0.3s ease;
}

.error-type-count {
  font-size: 11px;
  color: var(--color-text-secondary);
  min-width: 60px;
  text-align: right;
  white-space: nowrap;
}

.error-type-empty {
  text-align: center;
  color: var(--color-text-secondary);
  font-size: 13px;
  padding: 8px 0;
}

/* 环境信息 */
.environment-info {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
  padding-top: 12px;
  border-top: 1px solid var(--color-border);
}

.env-item {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.env-label {
  font-size: 11px;
  color: var(--color-text-secondary);
}

.env-value {
  font-size: 12px;
  color: var(--color-text-primary);
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 目录树 */
.info-card-large {
  display: flex;
  flex-direction: column;
}

.dir-tree-toolbar {
  display: flex;
  gap: 8px;
  margin-bottom: 12px;
}

.drop-zone {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  border: 1px dashed var(--color-border);
  border-radius: 6px;
  font-size: 12px;
  color: var(--color-text-secondary);
  min-width: 0;
}

.drop-zone.drop-zone-active {
  border-color: var(--color-accent);
  background-color: var(--color-accent-soft);
  color: var(--color-accent);
}

.dir-tree-container {
  flex: 1;
  min-height: 0;
  max-height: 400px;
  overflow: auto;
  border-radius: 6px;
  border: 1px solid var(--color-border);
  background-color: var(--color-page-bg);
}

.dir-list-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100px;
  font-size: 13px;
  color: var(--color-text-secondary);
}

.dir-list-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 10px 12px;
  background-color: var(--color-page-bg);
  border-radius: 6px;
  font-size: 12px;
  color: var(--color-text-secondary);
  margin-top: 12px;
}

.refresh-list-btn {
  padding: 4px 10px;
  background-color: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: 4px;
  color: var(--color-accent);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.2s;
}

.refresh-list-btn:hover {
  background-color: var(--color-surface-active);
  border-color: var(--color-accent);
}

/* 异常信息 */
.exception-content {
  margin-bottom: 12px;
}

.exception-item {
  background-color: var(--color-page-bg);
  border-left: 3px solid var(--color-danger);
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
  color: var(--color-text-primary);
  font-weight: 500;
}

.exception-type-tag {
  font-size: 11px;
  color: var(--color-danger);
  background-color: var(--color-danger-soft);
  padding: 3px 8px;
  border-radius: 4px;
}

.exception-time {
  font-size: 11px;
  color: var(--color-text-secondary);
  margin-bottom: 8px;
}

.exception-message {
  font-size: 12px;
  color: var(--color-text-primary);
  line-height: 1.5;
}

.exception-empty {
  text-align: center;
  padding: 24px 12px;
  color: var(--color-text-secondary);
}

.empty-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  margin-bottom: 6px;
}

.exception-empty p {
  font-size: 13px;
  margin: 0;
}

.exception-footer {
  padding-top: 12px;
  border-top: 1px solid var(--color-border);
}

.view-all-btn {
  width: 100%;
  padding: 8px;
  background-color: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: 6px;
  color: var(--color-accent);
  font-size: 13px;
  cursor: pointer;
  transition: all 0.2s;
}

.view-all-btn:hover {
  background-color: var(--color-surface-active);
  border-color: var(--color-accent);
}

/* 响应式 */
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
    gap: 12px;
    min-height: unset;
    padding: 12px 16px;
  }
  .header-right {
    width: 100%;
    flex-wrap: wrap;
  }
  .panel-scroll {
    padding: 16px;
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
}
</style>
