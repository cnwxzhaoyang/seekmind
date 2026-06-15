<script setup lang="ts">
/**
 * @author MorningSun
 * @CreatedDate 2026/06/02
 * @Description 索引状态页面，展示索引控制、统计信息、进度和异常摘要。
 */
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { useI18n } from "vue-i18n";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import {
  AlertCircle,
  Cpu,
  Database,
  Eye,
  Copy,
  FileCheck,
  FileClock,
  FileText,
  FolderOpenDot,
  RefreshCw,
  ToggleLeft,
  ToggleRight,
  X,
  FolderPlus,
  UploadCloud,
  TriangleAlert,
  ScanText,
} from "lucide-vue-next";
import SvgIcon from "../components/SvgIcon.vue";
import SeekMindContextMenu from "../components/SeekMind/SeekMindContextMenu.vue";
import SeekMindFailedFilesPanel from "../components/SeekMind/SeekMindFailedFilesPanel.vue";
import type { ContextMenuItem } from "../components/SeekMind/SeekMindContextMenu.vue";
import { useIndexDirTree } from "../composables/useIndexDirTree";
import { useIndexDirs } from "../composables/useIndexDirs";
import { seekMindApi, formatSeekMindError } from "../services/seekMindApi";
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
} from "../types/SeekMind";

const { t } = useI18n();

const status = ref<IndexStatusView | null>(null);
const parserRuntime = ref<ParserRuntimeView | null>(null);
const currentIndexParserSource = ref("");
const currentIndexParserWarning = ref("");
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

const { dirs, refreshIndexDirs } = useIndexDirs();
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
    status.value = await seekMindApi.getIndexStatus();
  } catch (error) {
    errorMessage.value = formatSeekMindError(
      error,
      t("page.status.error.loadStatus"),
    );
    console.error("[SeekMind] loadStatus failed", error);
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
    errorMessage.value = formatSeekMindError(error, successMessage);
  }
};

const loadDirs = async () => {
  try {
    await refreshIndexDirs("status:load");
  } catch (error) {
    errorMessage.value = formatSeekMindError(
      error,
      t("page.status.error.loadDirs"),
    );
    console.error("[SeekMind] loadDirs failed", error);
  }
};

const loadParserRuntime = async () => {
  try {
    parserRuntime.value = await seekMindApi.getParserRuntime();
  } catch (error) {
    errorMessage.value = formatSeekMindError(
      error,
      t("page.status.error.loadParser"),
    );
    console.error("[SeekMind] loadParserRuntime failed", error);
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

const pdfOcrNotice = computed(() => {
  if (!parserRuntime.value) {
    return null;
  }

  if (parserRuntime.value.pdf_ocr_available) {
    return {
      title: t("page.status.ocr.title"),
      desc: parserRuntime.value.pdf_ocr_message,
      hint: t("page.status.ocr.availableHint", {
        languages: ocrLanguagesPreview.value,
      }),
      kind: "success",
    };
  }

  return {
    title: t("page.status.ocr.unavailableTitle"),
    desc: parserRuntime.value.pdf_ocr_message,
    hint: t("page.status.ocr.unavailableHint"),
    kind: "warning",
  };
});

const ocrLanguagesPreview = computed(() => {
  if (!parserRuntime.value?.vision_ocr_languages.length) {
    return t("common.none");
  }

  const languages = parserRuntime.value.vision_ocr_languages;
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
    languages: parserRuntime.value.vision_ocr_languages.length
      ? parserRuntime.value.vision_ocr_languages.join(", ")
      : t("common.unknown"),
  };
});

const pythonParserReady = computed(
  () => Boolean(parserRuntime.value?.enabled && parserRuntime.value?.available),
);

const officeConverterReady = computed(
  () => Boolean(parserRuntime.value?.office_available),
);

const imageOcrReady = computed(
  () => Boolean(parserRuntime.value?.vision_ocr_languages?.length),
);

// 修复：阶段 7 的格式覆盖需要在状态页直观看到，否则用户只能读文档，无法判断当前运行时到底支持到哪一步。
const formatSupportItems = computed(() => [
  {
    key: "text",
    title: t("page.status.formatSupport.items.text.title"),
    formats: t("page.status.formatSupport.items.text.formats"),
    hint: t("page.status.formatSupport.items.text.hint"),
    state: "completed",
    stateLabel: t("page.status.formatSupport.states.completed"),
  },
  {
    key: "docx",
    title: t("page.status.formatSupport.items.docx.title"),
    formats: t("page.status.formatSupport.items.docx.formats"),
    hint: t("page.status.formatSupport.items.docx.hint"),
    state: "completed",
    stateLabel: t("page.status.formatSupport.states.completed"),
  },
  {
    key: "office",
    title: t("page.status.formatSupport.items.office.title"),
    formats: t("page.status.formatSupport.items.office.formats"),
    hint: officeConverterReady.value
      ? parserRuntime.value?.office_message || t("page.status.formatSupport.items.office.readyHint")
      : parserRuntime.value?.office_message || t("page.status.formatSupport.items.office.partialHint"),
    state: officeConverterReady.value ? "completed" : "partial",
    stateLabel: officeConverterReady.value
      ? t("page.status.formatSupport.states.completed")
      : t("page.status.formatSupport.states.partial"),
  },
  {
    key: "epub",
    title: t("page.status.formatSupport.items.epub.title"),
    formats: t("page.status.formatSupport.items.epub.formats"),
    hint: t("page.status.formatSupport.items.epub.hint"),
    state: "completed",
    stateLabel: t("page.status.formatSupport.states.completed"),
  },
  {
    key: "pdf",
    title: t("page.status.formatSupport.items.pdf.title"),
    formats: t("page.status.formatSupport.items.pdf.formats"),
    hint: pythonParserReady.value
      ? t("page.status.formatSupport.items.pdf.readyHint")
      : t("page.status.formatSupport.items.pdf.pendingHint"),
    state: pythonParserReady.value ? "completed" : "pending",
    stateLabel: pythonParserReady.value
      ? t("page.status.formatSupport.states.completed")
      : t("page.status.formatSupport.states.pending"),
  },
  {
    key: "pdfOcr",
    title: t("page.status.formatSupport.items.pdfOcr.title"),
    formats: t("page.status.formatSupport.items.pdfOcr.formats"),
    hint: parserRuntime.value?.pdf_ocr_message || t("page.status.formatSupport.items.pdfOcr.pendingHint"),
    state: parserRuntime.value?.pdf_ocr_available ? "completed" : "pending",
    stateLabel: parserRuntime.value?.pdf_ocr_available
      ? t("page.status.formatSupport.states.completed")
      : t("page.status.formatSupport.states.pending"),
  },
  {
    key: "imageOcr",
    title: t("page.status.formatSupport.items.imageOcr.title"),
    formats: t("page.status.formatSupport.items.imageOcr.formats"),
    hint: imageOcrReady.value
      ? t("page.status.formatSupport.items.imageOcr.readyHint", {
          languages: ocrLanguagesPreview.value,
        })
      : t("page.status.formatSupport.items.imageOcr.pendingHint"),
    state: imageOcrReady.value ? "completed" : "pending",
    stateLabel: imageOcrReady.value
      ? t("page.status.formatSupport.states.completed")
      : t("page.status.formatSupport.states.pending"),
  },
]);

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
    "seekmind:index-refresh-progress",
    (event) => {
      const payload = event.payload;
      status.value = payload.status;
      if (payload.parser_source) {
        currentIndexParserSource.value = payload.parser_source;
      }
      if (payload.warning) {
        currentIndexParserWarning.value = payload.warning;
      } else if (payload.parser_source === "python") {
        currentIndexParserWarning.value = "";
      }

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
  if (!status.value?.current_task) {
    currentIndexParserSource.value = "";
    currentIndexParserWarning.value = "";
  } else if (status.value.current_task.warning) {
    currentIndexParserWarning.value = status.value.current_task.warning;
  }
  scheduleNextRefresh();
};

const refreshIndex = async () => {
  refreshing.value = true;
  errorMessage.value = "";

  try {
    const started = await seekMindApi.refreshIndex();
    const finished = await waitForIndexRefreshJob(started.job_id);
    status.value = finished.status;
    if (finished.state === "failed") {
      errorMessage.value = finished.message;
    }
  } catch (error) {
    errorMessage.value = formatSeekMindError(
      error,
      t("page.status.error.reindex"),
    );
    console.error("[SeekMind] refreshIndex failed", error);
  } finally {
    refreshing.value = false;
    await syncDashboardState();
  }
};

const refreshPdfOcrTasks = async () => {
  refreshing.value = true;
  errorMessage.value = "";

  try {
    const started = await seekMindApi.refreshPdfOcrTasks();
    const finished = await waitForIndexRefreshJob(started.job_id);
    status.value = finished.status;
    if (finished.state === "failed") {
      errorMessage.value = finished.message;
    }
  } catch (error) {
    errorMessage.value = formatSeekMindError(
      error,
      t("page.status.error.reindex"),
    );
    console.error("[SeekMind] refreshPdfOcrTasks failed", error);
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
    status.value = await seekMindApi.pauseIndexing();
  } catch (error) {
    errorMessage.value = formatSeekMindError(
      error,
      t("page.status.error.pause"),
    );
    console.error("[SeekMind] pauseIndexing failed", error);
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
    status.value = await seekMindApi.resumeIndexing();
  } catch (error) {
    errorMessage.value = formatSeekMindError(
      error,
      t("page.status.error.resume"),
    );
    console.error("[SeekMind] resumeIndexing failed", error);
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
  const task = status.value?.current_task;
  if (task && Number.isFinite(task.progress)) {
    // 修复：后台已经写入 current_task.progress，前端不再仅依赖 scanned/indexed 比例，避免大分母下始终显示 0%。
    return Math.max(0, Math.min(100, Math.round(task.progress)));
  }

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

const currentIndexParserLabel = computed(() => {
  if (currentIndexParserSource.value === "python") {
    return t("status.parser.python");
  }
  if (currentIndexParserSource.value === "rust") {
    return t("status.parser.pythonFallback");
  }
  return t("common.unknown");
});

const currentIndexParserTone = computed(() => {
  return currentIndexParserSource.value === "python" ? "success" : "warning";
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
    // 修复：FailedFileView 只有 reason，没有 message；这里改为直接展示失败原因。
    reason: item.reason,
    traceback: "",
  }));
});

// 修复：状态页之前只暴露失败数量，失败文件与原因虽然已经在 failed_items 里返回，但没有直接显示；这里按失败时间倒序展开给独立面板使用。
const visibleFailedItems = computed(() =>
  [...(status.value?.failed_items ?? [])].sort((left, right) =>
    (right.last_failed_at || "").localeCompare(left.last_failed_at || ""),
  ),
);

const retryFailedFile = async (path: string) => {
  retryingTarget.value = path;
  errorMessage.value = "";

  try {
    status.value = await seekMindApi.retryFailedFile(path);
  } catch (error) {
    errorMessage.value = formatSeekMindError(
      error,
      t("page.status.error.retryFile"),
    );
    console.error("[SeekMind] retryFailedFile failed", error);
  } finally {
    retryingTarget.value = null;
    await syncDashboardState();
  }
};

const copyFailedReason = async (reason: string) => {
  await copyText(reason, t("page.status.failed.copiedReason"));
};

const retryFailedGroup = async (code: string, items: FailedFileView[]) => {
  retryingTarget.value = code;
  errorMessage.value = "";

  try {
    for (const item of items) {
      await seekMindApi.retryFailedFile(item.file);
    }
  } catch (error) {
    errorMessage.value = formatSeekMindError(
      error,
      t("page.status.error.retryGroup"),
    );
    console.error("[SeekMind] retryFailedGroup failed", error);
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
    await seekMindApi.quickLookFile(path);
    infoMessage.value = t("page.status.action.quickLookOpened");
  } catch (error) {
    errorMessage.value = formatSeekMindError(
      error,
      t("page.status.action.quickLookFailed"),
    );
    console.error("[SeekMind] quickLookDir failed", error);
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
    const started = await seekMindApi.refreshIndexDir(path);
    const finished = await waitForIndexRefreshJob(started.job_id);
    if (finished.state === "failed") {
      throw new Error(finished.message || t("page.library.error.rebuild"));
    }
    infoMessage.value = t("page.library.info.reindexed", { path });
    await refreshDashboard();
  } catch (error) {
    errorMessage.value = formatSeekMindError(
      error,
      t("page.library.error.rebuild"),
    );
    console.error("[SeekMind] refreshIndexDir failed", error);
  } finally {
    busyPath.value = null;
  }
};

const toggleDir = async (dir: IndexDirView) => {
  busyPath.value = dir.path;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    await seekMindApi.setIndexDirEnabled(dir.path, !dir.enabled);
    infoMessage.value = dir.enabled
      ? t("page.library.info.disabled", { path: dir.path })
      : t("page.library.info.enabled", { path: dir.path });
    await refreshDashboard();
  } catch (error) {
    errorMessage.value = formatSeekMindError(
      error,
      t("page.library.error.toggleDir"),
    );
    console.error("[SeekMind] setIndexDirEnabled failed", error);
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
    await seekMindApi.removeIndexDir(path);
    infoMessage.value = t("page.library.info.deleted", { path });
    await refreshDashboard();
  } catch (error) {
    errorMessage.value = formatSeekMindError(
      error,
      t("page.library.error.deleteDir"),
    );
    console.error("[SeekMind] removeIndexDir failed", error);
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
    await seekMindApi.addIndexDir(selected);
    infoMessage.value = t("page.library.info.added", { path: selected });
    await refreshDashboard();
  } catch (error) {
    errorMessage.value = formatSeekMindError(
      error,
      t("page.library.error.addDir"),
    );
    console.error("[SeekMind] addIndexDir failed", error);
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
    "seekmind:document-refresh-progress",
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
      const started = await seekMindApi.refreshDocument(
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
      const started = await seekMindApi.refreshIndexDir(path);
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
    const result: ImportPathsView = await seekMindApi.importPaths(normalized);
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
    errorMessage.value = formatSeekMindError(
      error,
      t("page.library.error.importPaths"),
    );
    console.error("[SeekMind] importPaths failed", error);
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
  // 修复：Windows 下索引目录卡片是页面内手写节点，不是通用树组件，必须在这里显式拦截原生右键菜单。
  event.preventDefault();
  event.stopPropagation();
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
          <span class="title-icon seekmind-page-header-icon"><Database :size="17" /></span>
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

      <!-- 修复：OCR 状态需要在状态页显式可见，避免用户只能从日志判断扫描件是否可识别。 -->
      <div
        v-if="pdfOcrNotice"
        class="ocr-notice-banner"
        :class="pdfOcrNotice.kind === 'warning' ? 'ocr-notice-warning' : 'ocr-notice-success'"
      >
        <ScanText :size="16" class="shrink-0 ocr-notice-icon" />
        <div class="min-w-0">
          <div class="ocr-notice-title">{{ pdfOcrNotice.title }}</div>
          <div class="ocr-notice-desc">{{ pdfOcrNotice.desc }}</div>
          <div class="ocr-notice-hint">{{ pdfOcrNotice.hint }}</div>
        </div>
      </div>

      <div v-if="chineseOcrNotice" class="ocr-notice-banner ocr-notice-warning">
        <AlertCircle :size="16" class="shrink-0 ocr-notice-icon" />
        <div class="min-w-0">
          <div class="ocr-notice-title">{{ chineseOcrNotice.title }}</div>
          <div class="ocr-notice-desc">{{ chineseOcrNotice.desc }}</div>
          <div class="ocr-notice-hint">{{ chineseOcrNotice.hint }}</div>
          <div class="ocr-notice-langs">{{ chineseOcrNotice.languages }}</div>
        </div>
      </div>

      <!-- 主容器 -->
      <div class="panel-content">
        <!-- 左侧面板 -->
        <div class="left-panel">
          <!-- 索引目录 -->
          <div class="card info-card-large">
            <div class="card-header card-header--stacked">
              <div class="card-head-main">
                <span class="card-icon seekmind-page-header-icon"><SvgIcon icon="icon-info" size="lg" /></span>
                <div class="card-head-copy">
                  <h2>{{ t("sidebar.indexDirs") }}</h2>
                  <p>{{ t("page.status.section.indexDirsDesc") }}</p>
                </div>
              </div>
              <span class="file-count">{{ explicitIndexDirCount }} 个目录</span>
            </div>

            <div class="directory-actions">
              <button
                class="btn btn-ghost icon-btn"
                :disabled="
                  refreshing ||
                  loading ||
                  !status?.current_task ||
                  status.current_task.state === 'paused'
                "
                @click="pauseIndexing"
                :title="t('page.status.btn.pause')"
                :aria-label="t('page.status.btn.pause')"
              >
                <span class="btn-icon" aria-hidden="true"><SvgIcon icon="icon-pause" size="sm" /></span>
              </button>
              <button
                class="btn btn-ghost icon-btn"
                :disabled="
                  refreshing ||
                  loading ||
                  !status?.current_task ||
                  status.current_task.state !== 'paused'
                "
                @click="resumeIndexing"
                :title="t('page.status.btn.resume')"
                :aria-label="t('page.status.btn.resume')"
              >
                <span class="btn-icon" aria-hidden="true"><SvgIcon icon="icon-play" size="sm" /></span>
              </button>
              <button
                class="btn btn-primary icon-btn"
                :disabled="refreshing || loading"
                @click="refreshIndex"
                :title="t('page.status.btn.reindex')"
                :aria-label="t('page.status.btn.reindex')"
              >
                <span class="btn-icon" aria-hidden="true"><SvgIcon icon="icon-refresh" size="sm" /></span>
              </button>
              <button
                class="btn btn-primary icon-btn"
                :disabled="importing || refreshing || !!busyPath"
                @click="chooseAndAddDir"
                :title="t('page.library.btn.addDir')"
                :aria-label="t('page.library.btn.addDir')"
              >
                <FolderPlus :size="14" />
              </button>
            </div>

            <div class="dir-tree-toolbar">
              <!-- 目录拖拽提示保留为说明区，按钮已合并到上方一排图标操作。 -->
              <div
                class="drop-zone"
                :class="dragActive ? 'drop-zone-active' : ''"
              >
                <UploadCloud :size="14" />
                <span>{{ dragActive ? t("page.library.dropActive") : t("page.library.dropHint") }}</span>
              </div>
              <div
                v-if="status?.current_task"
                class="mt-3 flex flex-wrap items-center gap-2"
              >
                <span class="seekmind-item-meta">{{ t("page.status.progress.currentParser") }}</span>
                <span
                  class="inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11px]"
                  :class="currentIndexParserTone === 'success'
                    ? 'border-emerald-soft bg-emerald-soft text-success'
                    : 'border-amber-soft bg-amber-soft text-warning'"
                >
                  <Cpu :size="11" />
                  {{ currentIndexParserLabel }}
                </span>
              </div>
              <div
                v-if="currentIndexParserWarning"
                class="mt-2 text-[11px] leading-5 text-warning"
              >
                {{ currentIndexParserWarning }}
              </div>
            </div>

            <div class="dir-list">
              <div
                v-if="visibleDirRows.length === 0"
                class="dir-list-empty"
              >
                {{ t("page.status.emptyDirs") }}
              </div>
              <div
                v-else
                class="dir-card-list"
              >
                <div
                  v-for="row in visibleDirRows"
                  :key="row.fullPath"
                  class="dir-card-row"
                  :class="[
                    row.depth > 0 ? 'dir-card-row--child' : '',
                    row.hasChildren ? 'dir-card-row--branch' : '',
                  ]"
                  :style="{ paddingLeft: `${16 + row.depth * 16}px` }"
                  @contextmenu.prevent="handleTreeContextMenu(row, $event)"
                  @click="row.hasChildren ? setDirExpanded(row.fullPath, !row.expanded) : undefined"
                >
                  <button
                    v-if="row.hasChildren"
                    class="dir-expand-btn"
                    type="button"
                    :title="row.expanded ? t('sidebar.collapse') : t('sidebar.expand')"
                    @click.stop="setDirExpanded(row.fullPath, !row.expanded)"
                  >
                    <span>{{ row.expanded ? "▾" : "▸" }}</span>
                  </button>
                  <span
                    v-else
                    class="dir-expand-spacer"
                  />
                  <span class="dir-folder-icon">
                    <FolderOpenDot :size="18" />
                  </span>
                  <div class="dir-card-main">
                    <strong>{{ row.displayName }}</strong>
                    <span>{{ row.fullPath }}</span>
                  </div>
                  <div class="dir-card-meta">
                    <span>{{ row.dir.docs }} 文件</span>
                    <span>{{ row.dir.chunks }} 片段</span>
                  </div>
                </div>
              </div>
            </div>
          </div>

      </div>

        <!-- 右侧面板 -->
      <div class="right-panel">
          <!-- 统计信息 -->
          <div class="card">
            <div class="card-header">
              <span class="card-icon seekmind-page-header-icon"><SvgIcon icon="icon-chart" size="lg" /></span>
              <div class="card-head-copy">
                <h2>{{ t("page.status.section.statistics") }}</h2>
              </div>
            </div>
            <div class="stats-grid">
              <div class="stat-item">
                <span class="stat-icon indexed" aria-hidden="true"><FileCheck :size="18" /></span>
                <div class="stat-content">
                  <div class="stat-label">{{ t("page.status.stats.indexedFiles") }}</div>
                  <div class="stat-value">{{ status?.indexed_docs ?? 0 }}</div>
                </div>
              </div>
              <div class="stat-item">
                <span class="stat-icon scanned" aria-hidden="true"><FolderOpenDot :size="18" /></span>
                <div class="stat-content">
                  <div class="stat-label">{{ t("page.status.stats.indexedCount") }}</div>
                  <div class="stat-value">{{ status?.scanned_docs ?? 0 }}</div>
                </div>
              </div>
              <div class="stat-item">
                <span class="stat-icon scanned" aria-hidden="true"><FileText :size="18" /></span>
                <div class="stat-content">
                  <div class="stat-label">{{ t("page.status.stats.chunkTotal") }}</div>
                  <div class="stat-value">{{ status?.indexed_chunks ?? 0 }}</div>
                </div>
              </div>
              <div class="stat-item">
                <span class="stat-icon error" aria-hidden="true"><TriangleAlert :size="18" /></span>
                <div class="stat-content">
                  <div class="stat-label">{{ t("page.status.stats.errorCount") }}</div>
                  <div class="stat-value error-value">{{ status?.failed_files ?? 0 }}</div>
                </div>
              </div>
              <div class="stat-item">
                <span class="stat-icon pending" aria-hidden="true"><FileClock :size="18" /></span>
                <div class="stat-content">
                  <div class="stat-label">{{ t("page.status.stats.pendingFiles") }}</div>
                  <div class="stat-value">{{ pendingCount }}</div>
                </div>
              </div>
              <div class="stat-item">
                <span class="stat-icon scanned" aria-hidden="true"><ScanText :size="18" /></span>
                <div class="stat-content">
                  <div class="stat-label">{{ t("page.status.stats.ocrTasks") }}</div>
                  <div class="stat-value">{{ status?.pdf_ocr_tasks ?? 0 }}</div>
                  <button
                    type="button"
                    class="stat-action-button"
                    :disabled="refreshing || loading"
                    @click="refreshPdfOcrTasks"
                  >
                    {{ t("page.status.ocr.retry") }}
                  </button>
                </div>
              </div>
            </div>
          </div>

          <!-- 索引进度 -->
          <div class="card">
            <div class="card-header">
              <span class="card-icon seekmind-page-header-icon"><SvgIcon icon="icon-clock" size="lg" /></span>
              <div class="card-head-copy">
                <h2>{{ t("page.status.section.progress") }}</h2>
              </div>
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

            <div class="progress-metrics">
              <div class="progress-metric">
                <span class="progress-metric-icon"><SvgIcon icon="icon-clock" size="sm" /></span>
                <div class="progress-metric-label">{{ t("page.status.progress.duration") }}</div>
                <div class="progress-metric-value">{{ currentTaskDurationText }}</div>
              </div>
              <div class="progress-metric">
                <span class="progress-metric-icon"><SvgIcon icon="icon-clock" size="sm" /></span>
                <div class="progress-metric-label">{{ t("page.status.progress.startTime") }}</div>
                <div class="progress-metric-value">{{ currentTaskStartTimeText }}</div>
              </div>
              <div class="progress-metric">
                <span class="progress-metric-icon success"><SvgIcon icon="icon-success" size="sm" /></span>
                <div class="progress-metric-label">{{ t("page.status.progress.success") }}</div>
                <div class="progress-metric-value">{{ status?.current_task?.succeeded ?? 0 }}</div>
              </div>
              <div class="progress-metric">
                <span class="progress-metric-icon error"><SvgIcon icon="icon-error" size="sm" /></span>
                <div class="progress-metric-label">{{ t("page.status.progress.failed") }}</div>
                <div class="progress-metric-value">{{ status?.current_task?.failed ?? 0 }}</div>
              </div>
              <div class="progress-metric">
                <span class="progress-metric-icon skipped">⊘</span>
                <div class="progress-metric-label">{{ t("page.status.progress.skipped") }}</div>
                <div class="progress-metric-value">{{ status?.current_task?.skipped ?? 0 }}</div>
              </div>
            </div>
          </div>

          <!-- 健康状态 -->
          <div class="card">
            <div class="card-header">
              <span class="card-icon seekmind-page-header-icon"><SvgIcon icon="icon-error" size="lg" /></span>
              <div class="card-head-copy">
                <h2>{{ t("page.status.section.healthStatus") }}</h2>
              </div>
            </div>

            <!-- 健康区改成和索引统计一致的紧凑小卡片，避免圆环摘要占用大块空白。 -->
            <div class="health-grid health-grid--cards">
              <div class="stat-item health-metric">
                <span class="stat-icon error" aria-hidden="true"><TriangleAlert :size="18" /></span>
                <div class="stat-content">
                  <div class="stat-label">{{ t("page.status.errorSummary.totalErrors") }}</div>
                  <div class="stat-value error-value">{{ status?.failed_files ?? 0 }}</div>
                </div>
              </div>
              <div class="stat-item health-metric">
                <span class="stat-icon scanned" aria-hidden="true"><SvgIcon icon="icon-info" size="sm" /></span>
                <div class="stat-content">
                  <div class="stat-label">{{ t("page.status.errorSummary.errorType") }}</div>
                  <div class="stat-value health-value-truncate" :title="errorTypeList[0]?.name || '-'">
                    {{ errorTypeList[0]?.name || "-" }}
                  </div>
                </div>
              </div>
              <div class="stat-item health-metric">
                <span class="stat-icon scanned" aria-hidden="true"><SvgIcon icon="icon-info" size="sm" /></span>
                <div class="stat-content">
                  <div class="stat-label">{{ t("page.status.errorSummary.pythonVersion") }}</div>
                  <div class="stat-value">{{ parserRuntime?.python_bin || "-" }}</div>
                </div>
              </div>
              <div class="stat-item health-metric">
                <span class="stat-icon pending" aria-hidden="true"><FileClock :size="18" /></span>
                <div class="stat-content">
                  <div class="stat-label">{{ t("page.status.errorSummary.timeout") }}</div>
                  <div class="stat-value">{{ parserRuntime?.timeout_ms ?? 0 }} ms</div>
                </div>
              </div>
              <div class="stat-item health-metric">
                <span class="stat-icon scanned" aria-hidden="true"><Eye :size="18" /></span>
                <div class="stat-content">
                  <div class="stat-label">{{ t("page.status.section.latestException") }}</div>
                  <div
                    class="stat-value health-value-truncate"
                    :title="latestExceptions.length > 0
                      ? `${latestExceptions[0].file} · ${latestExceptions[0].reason || latestExceptions[0].type}`
                      : t('page.status.exception.noException')"
                  >
                    {{ latestExceptions.length > 0 ? latestExceptions[0].file : t("page.status.exception.noException") }}
                  </div>
                  <div
                    v-if="latestExceptions.length > 0"
                    class="stat-desc health-value-truncate"
                    :title="latestExceptions[0].reason || latestExceptions[0].type"
                  >
                    {{ latestExceptions[0].reason || latestExceptions[0].type }}
                  </div>
                </div>
              </div>
            </div>
          </div>

          <div class="card">
            <div class="card-header">
              <span class="card-icon seekmind-page-header-icon"><FileText :size="18" /></span>
              <div class="card-head-copy">
                <h2>{{ t("page.status.section.formatSupport") }}</h2>
              </div>
            </div>

            <div class="format-support-list">
              <div
                v-for="item in formatSupportItems"
                :key="item.key"
                class="format-support-item"
              >
                <div class="format-support-main">
                  <div class="format-support-title-row">
                    <strong>{{ item.title }}</strong>
                    <span class="format-support-formats">{{ item.formats }}</span>
                  </div>
                  <div class="format-support-hint">
                    {{ item.hint }}
                  </div>
                </div>
                <span
                  class="format-support-badge"
                  :class="`format-support-badge--${item.state}`"
                >
                  {{ item.stateLabel }}
                </span>
              </div>
            </div>
          </div>

          <SeekMindFailedFilesPanel
            :items="visibleFailedItems"
            :retrying-target="retryingTarget"
            @retry="retryFailedFile"
            @copy-path="copyDirPath"
            @copy-reason="copyFailedReason"
          />

        </div>
      </div>
    </main>

    <SeekMindContextMenu
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
  margin: 12px;
  background-color: transparent;
  color: var(--color-text-primary);
  min-height: 0;
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.index-status-panel :deep(.seekmind-page-header-icon) {
  width: auto;
  height: auto;
  padding: 0;
  border: 0;
  border-radius: 0;
  background: transparent;
  box-shadow: none;
}

/* Office 提示 */
.office-notice-banner {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 12px 16px;
  margin-bottom: 16px;
  background-color: rgba(187, 128, 9, 0.12);
  border-radius: 16px;
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

.ocr-notice-banner {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 12px 16px;
  margin-bottom: 16px;
  background-color: rgba(37, 99, 235, 0.06);
  border-radius: 16px;
}

.ocr-notice-warning {
  background-color: rgba(187, 128, 9, 0.12);
}

.ocr-notice-icon {
  color: var(--color-accent);
  margin-top: 2px;
}

.ocr-notice-warning .ocr-notice-icon {
  color: var(--color-warning);
}

.ocr-notice-success .ocr-notice-icon {
  color: var(--color-success);
}

.ocr-notice-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--color-text-primary);
}

.ocr-notice-warning .ocr-notice-title {
  color: var(--color-warning);
}

.ocr-notice-desc,
.ocr-notice-hint,
.ocr-notice-langs {
  font-size: 12px;
  color: var(--color-text-secondary);
  margin-top: 4px;
  line-height: 1.5;
  word-break: break-word;
}

/* 头部 */
.panel-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
  min-height: 44px;
  padding: 0 20px;
  flex-shrink: 0;
  background: transparent;
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
  max-width: 64ch;
  font-size: 12px;
  line-height: 1.35;
  color: var(--color-text-secondary);
  margin: 0;
}

.header-right {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-shrink: 0;
}

.status-badge {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 5px 11px;
  border-radius: 999px;
  font-size: 12px;
  font-weight: 600;
  white-space: nowrap;
}

.status-badge.status-running {
  background-color: rgba(34, 197, 94, 0.12);
  color: var(--color-success);
}

.status-badge.status-idle {
  background-color: rgba(148, 163, 184, 0.12);
  color: var(--color-text-secondary);
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
  border: 0;
  background-color: rgba(255, 255, 255, 0.72);
  border-radius: 999px;
  color: var(--color-text-secondary);
  cursor: pointer;
  font-size: 16px;
  transition: all 0.2s;
  display: flex;
  align-items: center;
  justify-content: center;
}

.refresh-btn:hover:not(:disabled) {
  background-color: rgba(47, 129, 255, 0.08);
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
  display: flex;
  flex-direction: column;
  overflow: hidden;
  padding: 12px 16px 16px;
}

/* 主容器 */
.panel-content {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 16px;
  align-items: stretch;
  flex: 1;
  min-height: 0;
  height: auto;
  overflow: hidden;
}

.left-panel,
.right-panel {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-height: 0;
  height: 100%;
}

.left-panel {
  overflow: hidden;
}

.right-panel {
  overflow-y: auto;
  padding-right: 4px;
}

/* 左侧目录卡需要随列高拉伸，目录过多时仅在卡片内部滚动，避免整页高度继续膨胀。 */
.left-panel > .info-card-large {
  flex: 1;
  min-height: 0;
  height: 100%;
  align-self: stretch;
  overflow: hidden;
}

/* 卡片通用 */
.card {
  background-color: rgba(255, 255, 255, 0.92);
  border: 1px solid rgba(148, 163, 184, 0.16);
  border-radius: 18px;
  padding: 16px;
  box-shadow: 0 8px 24px rgba(15, 23, 42, 0.04);
}

.card-header {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  margin-bottom: 12px;
  padding-bottom: 0;
}

.card-header--stacked {
  justify-content: space-between;
  align-items: flex-start;
}

.card-head-main {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  min-width: 0;
}

.card-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  width: 42px;
  height: 42px;
  border-radius: 14px;
  background: rgba(47, 129, 255, 0.12);
  color: var(--color-accent);
}

.card-header h2 {
  font-size: 14px;
  font-weight: 600;
  color: var(--color-text-primary);
  flex: 1;
  margin: 0;
}

.card-head-copy {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.card-head-copy h2 {
  font-size: 14px;
  font-weight: 600;
  color: var(--color-text-primary);
  margin: 0;
}

.card-head-copy p {
  font-size: 12px;
  color: var(--color-text-secondary);
  line-height: 1.45;
  margin: 0;
}

.card-description {
  font-size: 12px;
  color: var(--color-text-secondary);
  margin-bottom: 12px;
  line-height: 1.5;
}

.file-count,
.exception-count {
  font-size: 12px;
  color: var(--color-text-secondary);
  background-color: rgba(148, 163, 184, 0.12);
  padding: 3px 8px;
  border-radius: 999px;
  white-space: nowrap;
}

/* 目录内控制 */
.directory-actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  margin-bottom: 12px;
}

.directory-actions .btn {
  min-width: 36px;
  width: 36px;
  height: 36px;
  padding: 0;
  border-radius: 12px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.directory-actions .btn .btn-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.directory-actions .btn .btn-icon :deep(svg) {
  width: 16px;
  height: 16px;
}

.directory-actions .btn.btn-ghost {
  background-color: rgba(255, 255, 255, 0.84);
  color: var(--color-text-primary);
  box-shadow: 0 1px 2px rgba(15, 23, 42, 0.04);
}

.directory-actions .btn.btn-ghost:hover:not(:disabled) {
  background-color: rgba(47, 129, 255, 0.08);
  color: var(--color-accent);
}

.directory-actions .btn.btn-primary {
  background-color: rgba(47, 129, 255, 0.96);
  color: white;
  box-shadow: 0 1px 2px rgba(15, 23, 42, 0.08);
}

.directory-actions .btn.btn-primary:hover:not(:disabled) {
  background-color: var(--color-accent-hover);
}

.directory-actions .btn:disabled {
  background-color: rgba(148, 163, 184, 0.12);
  color: var(--color-text-secondary);
}

.directory-actions .btn.btn-secondary {
  color: var(--color-text-primary);
}

.btn {
  padding: 7px 12px;
  border-radius: 999px;
  border: 0;
  background-color: rgba(255, 255, 255, 0.82);
  color: var(--color-text-primary);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  transition: all 0.2s;
  white-space: nowrap;
}

.btn:hover:not(:disabled) {
  background-color: rgba(47, 129, 255, 0.08);
  color: var(--color-accent);
}

.btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.btn-primary {
  background-color: var(--color-accent);
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

/* 统计信息：保持为紧凑的仪表盘行，避免卡片下方留下大块空白。 */
.stats-grid {
  display: grid;
  grid-template-columns: repeat(6, minmax(0, 1fr));
  gap: 6px;
}

.stat-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: flex-start;
  gap: 4px;
  padding: 10px 10px 8px;
  background-color: rgba(255, 255, 255, 0.72);
  border-radius: 12px;
  border: 1px solid rgba(148, 163, 184, 0.1);
  min-height: 92px;
  text-align: center;
  position: relative;
}

.stat-icon {
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 10px;
  background-color: var(--color-surface-active);
  flex-shrink: 0;
  border: 1px solid transparent;
}

.stat-icon.indexed {
  background-color: var(--color-accent-soft);
  color: var(--color-accent);
  border-color: rgba(47, 129, 255, 0.14);
}

.stat-icon.scanned {
  background-color: rgba(47, 129, 255, 0.08);
  color: var(--color-accent-text);
  border-color: rgba(47, 129, 255, 0.12);
}

.stat-icon.error {
  background-color: var(--color-danger-soft);
  color: var(--color-danger);
  border-color: rgba(185, 28, 28, 0.14);
}

.stat-icon.pending {
  background-color: var(--color-amber-soft);
  color: #b45309;
  border-color: rgba(180, 83, 9, 0.14);
}

.stat-content {
  flex: 1;
  min-width: 0;
  width: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
}

.stat-label {
  font-size: 10px;
  color: var(--color-text-secondary);
  margin-bottom: 2px;
  line-height: 1.2;
}

.stat-value {
  font-size: 15px;
  font-weight: 600;
  color: var(--color-text-primary);
  margin-bottom: 0;
  line-height: 1.15;
}

.stat-value.error-value {
  color: var(--color-danger);
}

.stat-desc {
  font-size: 11px;
  color: var(--color-text-dim);
}

.stat-action-button {
  position: absolute;
  top: 6px;
  right: 6px;
  border: 1px solid var(--color-border);
  background: var(--color-surface);
  color: var(--color-text-secondary);
  border-radius: 999px;
  padding: 2px 5px;
  font-size: 8px;
  line-height: 1;
  cursor: pointer;
  transition:
    color 0.16s ease,
    border-color 0.16s ease,
    background-color 0.16s ease;
}

.stat-action-button:hover:not(:disabled) {
  color: var(--color-accent);
  border-color: rgba(47, 129, 255, 0.28);
  background: var(--color-accent-soft);
}

.stat-action-button:disabled {
  cursor: not-allowed;
  opacity: 0.6;
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

/* 索引进度：保持为紧凑的卡片行，和索引统计一致。 */
.progress-metrics {
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  gap: 6px;
}

.progress-metric {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: flex-start;
  gap: 3px;
  padding: 8px 8px 7px;
  background-color: rgba(255, 255, 255, 0.72);
  border-radius: 12px;
  border: 1px solid rgba(148, 163, 184, 0.1);
  min-height: 84px;
  text-align: center;
}

.progress-metric-icon {
  width: 26px;
  height: 26px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 9px;
  background-color: var(--color-surface-active);
  color: var(--color-accent);
  border: 1px solid transparent;
  flex-shrink: 0;
}

.progress-metric-icon.success {
  color: var(--color-success);
  background: rgba(34, 197, 94, 0.08);
  border-color: rgba(34, 197, 94, 0.14);
}

.progress-metric-icon.error {
  color: var(--color-danger);
  background: var(--color-danger-soft);
  border-color: rgba(185, 28, 28, 0.14);
}

.progress-metric-icon.skipped {
  color: #64748b;
  background: rgba(148, 163, 184, 0.12);
  border-color: rgba(148, 163, 184, 0.14);
}

.progress-metric-label {
  font-size: 9px;
  color: var(--color-text-secondary);
  line-height: 1.15;
}

.progress-metric-value {
  font-size: 14px;
  font-weight: 600;
  color: var(--color-text-primary);
  line-height: 1.15;
}

/* 健康状态：改成紧凑的小卡片行，和索引统计保持一致的视觉密度。 */
.health-grid--cards {
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  gap: 6px;
}

.health-metric {
  min-height: 92px;
}

.health-value-truncate {
  width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.format-support-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.format-support-item {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 12px;
  border-radius: 14px;
  background-color: rgba(255, 255, 255, 0.72);
  border: 1px solid rgba(148, 163, 184, 0.1);
}

.format-support-main {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.format-support-title-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}

.format-support-title-row strong {
  font-size: 13px;
  line-height: 1.35;
  color: var(--color-text-primary);
  font-weight: 600;
}

.format-support-formats {
  font-size: 11px;
  line-height: 1.35;
  color: var(--color-text-secondary);
}

.format-support-hint {
  font-size: 12px;
  line-height: 1.5;
  color: var(--color-text-secondary);
  word-break: break-word;
}

.format-support-badge {
  flex: none;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 56px;
  padding: 4px 10px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 600;
  white-space: nowrap;
}

.format-support-badge--completed {
  background-color: rgba(34, 197, 94, 0.12);
  color: var(--color-success);
}

.format-support-badge--partial {
  background-color: rgba(234, 179, 8, 0.14);
  color: #b45309;
}

.format-support-badge--pending {
  background-color: rgba(148, 163, 184, 0.16);
  color: var(--color-text-secondary);
}

/* 目录树 */
.info-card-large {
  display: flex;
  flex-direction: column;
  min-height: 0;
  height: 100%;
  overflow: hidden;
  max-height: 100%;
}

.dir-tree-toolbar {
  display: flex;
  gap: 8px;
  margin-bottom: 12px;
  padding: 12px 14px;
  border-radius: 16px;
  background: linear-gradient(135deg, rgba(47, 129, 255, 0.08), rgba(255, 255, 255, 0.96));
  border: 1px solid rgba(47, 129, 255, 0.12);
  box-shadow: 0 8px 20px rgba(15, 23, 42, 0.05);
}

.dir-list {
  flex: 1;
  min-height: 0;
  max-height: none;
  display: flex;
  flex-direction: column;
  gap: 10px;
  overflow-y: auto;
  padding-right: 4px;
}

.drop-zone {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border: 0;
  border-radius: 14px;
  font-size: 12px;
  color: var(--color-text-secondary);
  min-width: 0;
  background-color: rgba(255, 255, 255, 0.92);
  border: 1px solid rgba(148, 163, 184, 0.14);
}

.drop-zone.drop-zone-active {
  background-color: rgba(47, 129, 255, 0.12);
  color: var(--color-accent);
  border-color: rgba(47, 129, 255, 0.2);
}

.dir-list-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100px;
  font-size: 13px;
  color: var(--color-text-secondary);
}

.dir-card-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.dir-card-row {
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: 62px;
  padding: 12px 14px;
  border-radius: 16px;
  background: rgba(255, 255, 255, 0.9);
  border: 1px solid rgba(148, 163, 184, 0.14);
  box-shadow: 0 6px 16px rgba(15, 23, 42, 0.03);
  transition: background-color 0.2s ease, transform 0.2s ease;
  cursor: pointer;
}

.dir-card-row:hover {
  background: rgba(47, 129, 255, 0.04);
}

.dir-card-row--child {
  background: rgba(255, 255, 255, 0.84);
}

.dir-expand-btn,
.dir-expand-spacer {
  flex: none;
  width: 18px;
  height: 18px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--color-text-secondary);
}

.dir-expand-btn {
  border: 0;
  background: transparent;
  border-radius: 999px;
}

.dir-expand-btn:hover {
  color: var(--color-accent);
}

.dir-folder-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  border-radius: 12px;
  background: rgba(47, 129, 255, 0.1);
  color: var(--color-accent);
  flex: none;
}

.dir-card-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.dir-card-main strong {
  font-size: 14px;
  color: var(--color-text-primary);
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dir-card-main span {
  font-size: 12px;
  color: var(--color-text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dir-card-meta {
  flex: none;
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 12px;
  color: var(--color-text-secondary);
  white-space: nowrap;
}

.dir-card-meta span {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.dir-list-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 10px 12px;
  background-color: rgba(255, 255, 255, 0.72);
  border-radius: 14px;
  font-size: 12px;
  color: var(--color-text-secondary);
  margin-top: 12px;
}

.refresh-list-btn {
  padding: 4px 10px;
  background-color: rgba(255, 255, 255, 0.82);
  border: 0;
  border-radius: 999px;
  color: var(--color-accent);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.2s;
}

.refresh-list-btn:hover {
  background-color: var(--color-surface-active);
  border-color: var(--color-accent);
}

.dir-tree-toolbar .btn {
  background-color: rgba(47, 129, 255, 0.96);
  color: white;
  box-shadow: 0 1px 2px rgba(15, 23, 42, 0.08);
}

/* 异常信息 */
.exception-content {
  margin-bottom: 12px;
}

.exception-item {
  background-color: rgba(255, 255, 255, 0.72);
  padding: 12px;
  border-radius: 16px;
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
  background-color: rgba(185, 28, 28, 0.08);
  padding: 3px 8px;
  border-radius: 999px;
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
}

.view-all-btn {
  width: 100%;
  padding: 8px;
  background-color: rgba(255, 255, 255, 0.82);
  border: 0;
  border-radius: 999px;
  color: var(--color-accent);
  font-size: 13px;
  cursor: pointer;
  transition: all 0.2s;
}

.view-all-btn:hover {
  background-color: var(--color-surface-active);
  border-color: var(--color-accent);
}

html.dark .index-status-panel {
  color: var(--color-text-primary);
}

html.dark .index-status-panel .office-notice-banner,
html.dark .index-status-panel .ocr-notice-banner {
  background-color: rgba(47, 129, 255, 0.12);
  border: 1px solid rgba(88, 166, 255, 0.16);
}

html.dark .index-status-panel .ocr-notice-warning,
html.dark .index-status-panel .office-notice-banner {
  background-color: rgba(187, 128, 9, 0.12);
  border-color: rgba(234, 179, 8, 0.16);
}

html.dark .index-status-panel .card {
  background-color: rgba(22, 27, 34, 0.92);
  border-color: rgba(48, 54, 61, 0.95);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.28);
}

html.dark .index-status-panel .card-icon {
  background: rgba(47, 129, 255, 0.18);
}

html.dark .index-status-panel .file-count,
html.dark .index-status-panel .exception-count {
  background-color: rgba(110, 118, 129, 0.16);
}

html.dark .index-status-panel .directory-actions .btn.btn-ghost,
html.dark .index-status-panel .refresh-btn,
html.dark .index-status-panel .drop-zone,
html.dark .index-status-panel .dir-card-row,
html.dark .index-status-panel .stat-item,
html.dark .index-status-panel .progress-metric,
html.dark .index-status-panel .format-support-item,
html.dark .index-status-panel .dir-list-footer,
html.dark .index-status-panel .refresh-list-btn,
html.dark .index-status-panel .exception-item,
html.dark .index-status-panel .view-all-btn {
  background-color: rgba(13, 17, 23, 0.92);
  border-color: rgba(48, 54, 61, 0.92);
  color: var(--color-text-primary);
}

html.dark .index-status-panel .directory-actions .btn.btn-primary {
  background-color: rgba(47, 129, 255, 0.96);
  color: white;
}

html.dark .index-status-panel .directory-actions .btn:disabled {
  background-color: rgba(33, 38, 45, 0.9);
  color: var(--color-text-secondary);
}

html.dark .index-status-panel .stat-item,
html.dark .index-status-panel .progress-metric,
html.dark .index-status-panel .dir-card-row,
html.dark .index-status-panel .exception-item,
html.dark .index-status-panel .view-all-btn {
  box-shadow: none;
}

html.dark .index-status-panel .dir-card-row:hover {
  background: rgba(47, 129, 255, 0.08);
}

html.dark .index-status-panel .dir-card-row--child {
  background: rgba(13, 17, 23, 0.82);
}

html.dark .index-status-panel .dir-folder-icon,
html.dark .index-status-panel .stat-icon,
html.dark .index-status-panel .progress-metric-icon {
  background-color: rgba(47, 129, 255, 0.16);
}

html.dark .index-status-panel .stat-icon.error,
html.dark .index-status-panel .progress-metric-icon.error {
  background-color: rgba(248, 81, 73, 0.16);
}

html.dark .index-status-panel .stat-icon.pending,
html.dark .index-status-panel .progress-metric-icon.skipped {
  background-color: rgba(210, 153, 34, 0.16);
}

html.dark .index-status-panel .progress-bar {
  background-color: rgba(48, 54, 61, 0.92);
}

html.dark .index-status-panel .progress-fill {
  background: linear-gradient(90deg, #58a6ff, #2f81ff);
}

html.dark .index-status-panel .dir-list-empty,
html.dark .index-status-panel .exception-empty,
html.dark .index-status-panel .exception-time,
html.dark .index-status-panel .dir-card-main span,
html.dark .index-status-panel .dir-card-meta,
html.dark .index-status-panel .ocr-notice-desc,
html.dark .index-status-panel .ocr-notice-hint,
html.dark .index-status-panel .ocr-notice-langs,
html.dark .index-status-panel .office-notice-desc,
html.dark .index-status-panel .office-notice-hint,
html.dark .index-status-panel .header-description,
html.dark .index-status-panel .last-update {
  color: var(--color-text-secondary);
}

/* 响应式 */
@media (max-width: 1400px) {
  .panel-scroll {
    overflow-y: auto;
  }
  .panel-content {
    grid-template-columns: 1fr;
    height: auto;
    overflow: visible;
  }
  .left-panel,
  .right-panel {
    height: auto;
    overflow: visible;
  }
  .left-panel > .info-card-large,
  .info-card-large {
    height: auto;
    max-height: none;
  }
  .dir-list {
    max-height: 520px;
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
  .directory-actions {
    flex-direction: column;
  }
  .btn {
    width: 100%;
    justify-content: center;
  }
  .stats-grid {
    grid-template-columns: 1fr;
  }
  .progress-metrics {
    grid-template-columns: 1fr;
  }
  .health-grid--cards {
    grid-template-columns: 1fr;
  }
  .progress-metric {
    min-height: 0;
  }
}
</style>
