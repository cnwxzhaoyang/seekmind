/**
 * @author MorningSun
 * @CreatedDate 2026/06/07
 * @Description SeekMind 底部日志面板，展示索引、切片和语义任务事件。
 */
<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import { ChevronsUpDown, FileText, Loader2, Trash2 } from "lucide-vue-next";
import SeekMindToast from "./SeekMindToast.vue";
import SeekMindBadge from "./SeekMindBadge.vue";
import { useQuickAccessData } from "../../composables/useQuickAccessData";
import { useInfoMessage } from "../../composables/useInfoMessage";
import { seekMindApi } from "../../services/seekMindApi";
import type {
  AppRuntimeInfoView,
  DocumentRefreshProgressView,
  IndexRefreshProgressView,
  IndexSettingsView,
  IndexStatusView,
  SemanticRebuildProgressView,
  SemanticModelStatusView,
} from "../../types/SeekMind";

const HEADER_H = 28;
const DIVIDER_H = 3;
const HEIGHT_COLLAPSED = HEADER_H + DIVIDER_H;
const HEIGHT_MIN = 120;
const HEIGHT_MAX = 500;
const HEIGHT_DEFAULT = 200;
const STORAGE_KEY = "seekmind.logPanel.height";

const { t, te, locale } = useI18n();
const { quickDirs, recentDocuments, favorites } = useQuickAccessData();
const { infoMessage: exportInfoMessage } = useInfoMessage();

type LogScope = "index" | "document" | "semantic";
type LogLevel = "info" | "success" | "warning" | "error";

interface LogEntry {
  id: string;
  scope: LogScope;
  level: LogLevel;
  title: string;
  message: string;
  details: string;
  warning?: string;
  timestamp: string;
}

const expanded = ref(false);
const entries = ref<LogEntry[]>([]);
const panelHeight = ref(loadSavedHeight());
const dragging = ref(false);
const dragStartY = ref(0);
const dragStartHeight = ref(0);
const exporting = ref(false);
const exportTone = ref<"success" | "error">("success");
const indexStatus = ref<IndexStatusView | null>(null);
const indexSettings = ref<IndexSettingsView | null>(null);
const semanticStatus = ref<SemanticModelStatusView | null>(null);
const appRuntime = ref<AppRuntimeInfoView | null>(null);
const maxEntries = 120;
let unlistenIndex: null | (() => void) = null;
let unlistenDocument: null | (() => void) = null;
let unlistenSemantic: null | (() => void) = null;
let unlistenStorageReset: null | (() => void) = null;

function loadSavedHeight(): number {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved) {
      const h = parseInt(saved, 10);
      if (h >= HEIGHT_MIN && h <= HEIGHT_MAX) return h;
    }
  } catch {}
  return HEIGHT_DEFAULT;
}

function saveHeight(h: number) {
  try { localStorage.setItem(STORAGE_KEY, String(h)); } catch {}
}

const scopeMeta: Record<LogScope, { label: string; taskLabel: string; icon: string }> = {
  index: { label: "logPanel.scope.index", taskLabel: "logPanel.scopeLabel.index", icon: "icon-index-status" },
  document: { label: "logPanel.scope.document", taskLabel: "logPanel.scopeLabel.document", icon: "icon-file" },
  semantic: { label: "logPanel.scope.semantic", taskLabel: "logPanel.scopeLabel.semantic", icon: "icon-sync" },
};

const levelTone: Record<LogLevel, "default" | "success" | "warning" | "danger"> = {
  info: "default",
  success: "success",
  warning: "warning",
  error: "danger",
};

const parserSourceLabel = (source?: string | null) => {
  // 修复：日志面板只展示用户能理解的链路名称，不再直接暴露内部实现名。
  if (source === "python") {
    return t("status.parser.python");
  }
  if (source === "rust") {
    return t("status.parser.pythonFallback");
  }
  return t("common.unknown");
};

const leafName = (value?: string | null) => {
  if (!value) return "";
  return value.split(/[\\/]/).filter(Boolean).pop() || value;
};

const indexLogMessage = (scope: string, state: string) => {
  if (scope === "pdf-ocr") {
    if (state === "completed") return t("logPanel.messages.indexPdfOcr.completed");
    if (state === "failed") return t("logPanel.messages.indexPdfOcr.failed");
    return t("logPanel.messages.indexPdfOcr.running");
  }
  if (scope === "dir") {
    if (state === "completed") return t("logPanel.messages.indexDir.completed");
    if (state === "failed") return t("logPanel.messages.indexDir.failed");
    return t("logPanel.messages.indexDir.running");
  }
  if (scope === "fulltext-repair") {
    if (state === "completed") return t("logPanel.messages.fulltextRepair.completed");
    if (state === "failed") return t("logPanel.messages.fulltextRepair.failed");
    return t("logPanel.messages.fulltextRepair.running");
  }
  if (state === "completed") return t("logPanel.messages.indexAll.completed");
  if (state === "failed") return t("logPanel.messages.indexAll.failed");
  return t("logPanel.messages.indexAll.running");
};

const indexEventCode = (scope: string, state: string) => {
  if (scope === "pdf-ocr") return `index.pdfOcr.${state}`;
  if (scope === "dir") return `index.dir.${state}`;
  if (scope === "fulltext-repair") return `index.fulltextRepair.${state}`;
  return `index.all.${state}`;
};

const documentLogMessage = (state: string, warning?: string | null) => {
  if (state === "completed") {
    return warning ? t("logPanel.messages.documentCompletedWithWarning") : t("logPanel.messages.documentCompleted");
  }
  if (state === "failed") {
    return t("logPanel.messages.documentFailed");
  }
  return t("logPanel.messages.documentRunning");
};

const documentEventCode = (state: string, warning?: string | null) => {
  if (state === "completed" && warning) return "document.completedWithWarning";
  return `document.${state}`;
};

const semanticLogMessage = (source: string, state: string) => {
  if (source === "document") {
    if (state === "completed") return t("logPanel.messages.semanticDocumentCompleted");
    if (state === "failed") return t("logPanel.messages.semanticDocumentFailed");
    return t("logPanel.messages.semanticDocumentRunning");
  }
  if (state === "completed") return t("logPanel.messages.semanticRebuildCompleted");
  if (state === "failed") return t("logPanel.messages.semanticRebuildFailed");
  return t("logPanel.messages.semanticRebuildRunning");
};

const semanticEventCode = (source: string, state: string) => {
  if (source === "document") return `semantic.document.${state}`;
  return `semantic.rebuild.${state}`;
};

const translateLogMessage = (code: string, params: Record<string, unknown>, fallback: string) => {
  const key = `logPanel.messages.${code}`;
  // 修复：日志文本统一走前端 i18n，Rust 侧只负责传状态码和参数。
  return te(key) ? t(key, params) : fallback;
};

const taskStateLabel = (state?: string | null) => {
  if (state === "completed") return t("logPanel.state.completed");
  if (state === "failed") return t("logPanel.state.failed");
  if (state === "running") return t("logPanel.state.running");
  return state || t("common.unknown");
};

const indexDetailsWithParser = (details: string, parserSource?: string | null) => {
  const parserLabel = parserSourceLabel(parserSource);
  if (!parserLabel || parserLabel === t("common.unknown")) {
    return details;
  }
  return details ? `${details} · ${parserLabel}` : parserLabel;
};

const indexDetailsLabel = (payload: IndexRefreshProgressView) => {
  const params = payload.params as Record<string, unknown>;
  const fileNameParam = typeof params.file_name === "string" ? params.file_name : "";
  const currentParam = typeof params.current === "string" ? params.current : "";
  const currentPath =
    leafName(payload.path) ||
    leafName(fileNameParam) ||
    leafName(currentParam) ||
    leafName(payload.status.current_task?.current_file || "");
  if (payload.scope === "dir" && payload.path) {
    return t("logPanel.details.dir", { path: payload.path });
  }
  if (payload.scope === "fulltext-repair") {
    return t("logPanel.details.fulltextRepair");
  }
  if (currentPath) {
    return t("logPanel.details.file", { path: currentPath });
  }
  return t("logPanel.details.fullIndex");
};

const semanticDetailsLabel = (payload: SemanticRebuildProgressView) => {
  const params = payload.params as Record<string, unknown>;
  const currentDocument = typeof params.document === "string" ? params.document : payload.current_document;
  const currentChunk = typeof params.current_chunk === "string" ? params.current_chunk : payload.current_chunk;
  const documentLabel = currentDocument ? t("logPanel.details.semanticDocument", { path: leafName(currentDocument) }) : "";
  const chunkLabel = currentChunk ? t("logPanel.details.semanticChunk", { path: currentChunk }) : "";
  if (documentLabel && chunkLabel) {
    return `${documentLabel} · ${chunkLabel}`;
  }
  if (documentLabel) {
    return documentLabel;
  }
  if (chunkLabel) {
    return chunkLabel;
  }
  return t("logPanel.details.rebuilding");
};

const pushLog = (entry: Omit<LogEntry, "id" | "timestamp">) => {
  const now = new Date().toISOString();
  entries.value = [
    { id: `${now}-${Math.random().toString(16).slice(2)}`, timestamp: now, ...entry },
    ...entries.value,
  ].slice(0, maxEntries);
};

const formatEntryTimestamp = (timestamp: string) => {
  const date = new Date(timestamp);
  return date.toLocaleString(locale.value, {
    hour12: false,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
};

const loadMetrics = async () => {
  // 修复：日志导出依赖运行摘要，但面板初始化不能因为单个元数据接口失败而中断。
  const [status, settings, runtime, semantic] = await Promise.allSettled([
    seekMindApi.getIndexStatus(),
    seekMindApi.getIndexSettings(),
    seekMindApi.getAppRuntimeInfo(),
    seekMindApi.getEmbeddingModelStatus(),
  ]);
  if (status.status === "fulfilled") indexStatus.value = status.value;
  if (settings.status === "fulfilled") indexSettings.value = settings.value;
  if (runtime.status === "fulfilled") appRuntime.value = runtime.value;
  if (semantic.status === "fulfilled") semanticStatus.value = semantic.value;
};

const semanticWeightLabel = computed(() => Math.round((indexSettings.value?.semantic_weight ?? 0.25) * 100));
const sqliteLabel = computed(() => t("logPanel.metrics.documents", {
  indexed: indexStatus.value?.indexed_docs ?? 0,
  total: indexStatus.value?.scanned_docs ?? 0,
}));
const tantivyLabel = computed(() => t("logPanel.metrics.chunks", {
  count: indexStatus.value?.indexed_chunks ?? 0,
}));
const semanticVectorProgressLabel = computed(() => t("logPanel.metrics.semanticProgress", {
  embedded: semanticStatus.value?.embedded_chunks ?? 0,
  total: semanticStatus.value?.sqlite_chunks ?? 0,
}));
const semanticVectorLabel = computed(() => {
  if (!semanticStatus.value) {
    return t("logPanel.metrics.semanticUnavailable");
  }

  if (
    (indexStatus.value?.indexed_docs ?? 0) === 0 &&
    semanticStatus.value.sqlite_chunks === 0 &&
    semanticStatus.value.embedded_chunks === 0
  ) {
    return t("logPanel.metrics.semanticEmpty");
  }

  if (semanticStatus.value.model.available && !semanticStatus.value.needs_rebuild) {
    return t("logPanel.metrics.semanticReady");
  }

  if (semanticStatus.value.model.available) {
    return t("logPanel.metrics.semanticRebuild");
  }

  return t("logPanel.metrics.semanticUnavailable");
});
const semanticVectorTone = computed(() => {
  if (!semanticStatus.value) {
    return "warning" as const;
  }
  if (
    (indexStatus.value?.indexed_docs ?? 0) === 0 &&
    semanticStatus.value.sqlite_chunks === 0 &&
    semanticStatus.value.embedded_chunks === 0
  ) {
    return "default" as const;
  }
  if (semanticStatus.value.model.available && !semanticStatus.value.needs_rebuild) {
    return "success" as const;
  }
  if (semanticStatus.value.model.available) {
    return "warning" as const;
  }
  return "danger" as const;
});
const semanticVectorSummaryLabel = computed(() => {
  // 修复：正常态只展示向量化进度，避免底栏出现重复的“正常”状态词。
  if (semanticVectorLabel.value === t("logPanel.metrics.semanticReady")) {
    return semanticVectorProgressLabel.value;
  }
  return `${semanticVectorProgressLabel.value} · ${semanticVectorLabel.value}`;
});
const sidebarStats = computed(() => [
  { label: t("sidebar.statsDirs"), value: quickDirs.value.length },
  { label: t("sidebar.statsRecent"), value: recentDocuments.value.length },
  { label: t("sidebar.statsFavorites"), value: favorites.value.length },
]);
const bottomMetrics = computed(() => [
  // 修复：底栏指标改成用户可理解的业务口径，避免直接暴露 SQLite / Tantivy 实现名。
  { key: "sqlite", label: sqliteLabel.value, tone: "success" as const },
  { key: "tantivy", label: tantivyLabel.value, tone: "default" as const },
  { key: "semantic", label: semanticVectorSummaryLabel.value, tone: semanticVectorTone.value },
  { key: "weight", label: t("page.appSearch.semanticWeight", { weight: semanticWeightLabel.value }), tone: "default" as const },
]);

const installListeners = async () => {
  if (unlistenIndex || unlistenDocument || unlistenSemantic || unlistenStorageReset) return;

  unlistenIndex = await listen<IndexRefreshProgressView>("seekmind:index-refresh-progress", (event) => {
    const payload = event.payload;
    indexStatus.value = payload.status;
    const scope: LogScope = "index";
    const level: LogLevel = payload.state === "failed" ? "error" : payload.state === "completed" ? "success" : "info";
    const message = translateLogMessage(
      payload.code || indexEventCode(payload.scope, payload.state),
      payload.params || {},
      payload.message || indexLogMessage(payload.scope, payload.state),
    );
    const details = indexDetailsLabel(payload);
    pushLog({
      scope, level,
      title: t(scopeMeta[scope].taskLabel),
      message,
      details: indexDetailsWithParser(details, payload.parser_source),
    });
  });

  unlistenDocument = await listen<DocumentRefreshProgressView>("seekmind:document-refresh-progress", (event) => {
    const payload = event.payload;
    const scope: LogScope = "document";
    const level: LogLevel =
      payload.state === "failed" ? "error"
        : payload.warning ? "warning"
          : payload.state === "completed" ? "success" : "info";
    const message = translateLogMessage(
      payload.code || documentEventCode(payload.state, payload.warning),
      payload.params || {},
      payload.message || documentLogMessage(payload.state, payload.warning),
    );
    pushLog({
      scope, level,
      title: t(scopeMeta[scope].taskLabel),
      message,
      // 修复：失败分支不直接透出 Rust 侧原文，只保留可国际化的文件信息和链路信息。
      details: `${payload.file_name} · ${parserSourceLabel(payload.parser_source)}`,
      warning: payload.warning || undefined,
    });
  });

  unlistenSemantic = await listen<SemanticRebuildProgressView>("seekmind:semantic:rebuild-progress", (event) => {
    const payload = event.payload;
    const scope: LogScope = "semantic";
    const level: LogLevel = payload.state === "failed" ? "error" : payload.state === "completed" ? "success" : "info";
    // 修复：语义重建过程中底栏要实时吃进进度事件，不能只在完成时跳到 100%。
    if (payload.source === "rebuild") {
      semanticStatus.value = {
        model: payload.model,
        sqlite_chunks: payload.total_chunks,
        embedded_chunks: payload.embedded_chunks,
        needs_rebuild: payload.state !== "completed",
        last_indexed_at: payload.updated_at,
        last_error: payload.last_error,
        index_status: payload.state,
      };
      if (payload.state === "completed" || payload.state === "failed") {
        void loadMetrics();
      }
    }
    const message = translateLogMessage(
      payload.code || semanticEventCode(payload.source, payload.state),
      payload.params || {},
      payload.message || semanticLogMessage(payload.source, payload.state),
    );
    pushLog({
      scope, level,
      title: t(scopeMeta[scope].taskLabel),
      message,
      details: semanticDetailsLabel(payload),
    });
  });

  unlistenStorageReset = await listen("seekmind:storage-reset", () => {
    // 修复：索引被清空后，底栏指标和语义状态都需要重新读取，不能继续沿用旧缓存。
    void loadMetrics();
  });
};

const clearLogs = () => {
  entries.value = [];
  expanded.value = false;
};

const formatExportTimestamp = (timestamp: string) => {
  const date = new Date(timestamp);
  return date.toISOString().replace(/\.\d{3}Z$/, "Z");
};

const formatExportFilename = () => {
  const now = new Date();
  const stamp = now.toISOString().replace(/[:.]/g, "-");
  return `seekmind-logs-${stamp}.md`;
};

const buildExportContent = () => {
  const generatedAt = new Date().toISOString();
  const runtime = appRuntime.value;
  const status = indexStatus.value;
  const settings = indexSettings.value;

  const lines: string[] = [
    `# ${t("logPanel.exportMarkdown.title")}`,
    "",
    `- ${t("logPanel.exportMarkdown.generatedAt")}: ${generatedAt}`,
  ];

  if (runtime) {
    lines.push(`- ${t("logPanel.exportMarkdown.application")}: ${runtime.app_name} ${runtime.app_version}`);
    lines.push(`- ${t("logPanel.exportMarkdown.buildMode")}: ${runtime.build_mode}`);
    lines.push(`- ${t("logPanel.exportMarkdown.platform")}: ${runtime.target_os} / ${runtime.target_arch}`);
    lines.push(`- ${t("logPanel.exportMarkdown.dataDir")}: ${runtime.data_dir}`);
    lines.push(`- ${t("logPanel.exportMarkdown.cacheDir")}: ${runtime.cache_dir}`);
    lines.push(`- ${t("logPanel.exportMarkdown.sqlite")}: ${runtime.sqlite_path}`);
    lines.push(`- ${t("logPanel.exportMarkdown.fullTextIndex")}: ${runtime.tantivy_dir}`);
  }

  lines.push("");
  lines.push(`## ${t("logPanel.exportMarkdown.summary")}`);
  lines.push(`- ${t("logPanel.exportMarkdown.eventCount")}: ${entries.value.length}`);
  lines.push(`- ${sqliteLabel.value}`);
  lines.push(`- ${tantivyLabel.value}`);
  lines.push(`- ${t("page.appSearch.semanticWeight", { weight: semanticWeightLabel.value })}`);

  if (status) {
    lines.push(`- ${t("logPanel.exportMarkdown.indexedDocs")}: ${status.indexed_docs}`);
    lines.push(`- ${t("logPanel.exportMarkdown.indexedChunks")}: ${status.indexed_chunks}`);
    lines.push(`- ${t("logPanel.exportMarkdown.scannedDocs")}: ${status.scanned_docs}`);
    lines.push(`- ${t("logPanel.exportMarkdown.pdfOcrTasks")}: ${status.pdf_ocr_tasks}`);
    lines.push(`- ${t("logPanel.exportMarkdown.failedFiles")}: ${status.failed_files}`);
    if (status.current_task) {
      lines.push(`- ${t("logPanel.exportMarkdown.currentTask")}: ${status.current_task.label} · ${taskStateLabel(status.current_task.state)}`);
      lines.push(`- ${t("logPanel.exportMarkdown.currentDir")}: ${status.current_task.current_dir || "-"}`);
      lines.push(`- ${t("logPanel.exportMarkdown.currentFile")}: ${status.current_task.current_file || "-"}`);
    }
  }

  if (settings) {
    lines.push(`- ${t("logPanel.exportMarkdown.semanticSearch")}: ${settings.semantic_search_enabled ? t("common.enabled") : t("common.disabled")}`);
    lines.push(`- ${t("logPanel.exportMarkdown.semanticWeight")}: ${Math.round(settings.semantic_weight * 100)}%`);
    lines.push(`- ${t("logPanel.exportMarkdown.semanticThreshold")}: ${Math.round(settings.semantic_threshold * 100)}%`);
  }

  lines.push("");
  lines.push(`## ${t("logPanel.exportMarkdown.events")}`);
  if (entries.value.length === 0) {
    lines.push(`- ${t("logPanel.exportMarkdown.noEvents")}`);
  } else {
    entries.value
      .slice()
      .reverse()
      .forEach((entry) => {
        lines.push(`### [${formatExportTimestamp(entry.timestamp)}] ${t(scopeMeta[entry.scope].label)} / ${entry.title}`);
        lines.push(`- ${t("logPanel.exportMarkdown.level")}: ${entry.level}`);
        lines.push(`- ${t("logPanel.exportMarkdown.message")}: ${entry.message}`);
        lines.push(`- ${t("logPanel.exportMarkdown.details")}: ${entry.details}`);
        if (entry.warning) {
          lines.push(`- ${t("logPanel.exportMarkdown.warning")}: ${entry.warning}`);
        }
        lines.push("");
      });
  }

  return `${lines.join("\n").trim()}\n`;
};

const exportLogs = async () => {
  if (exporting.value) return;
  exporting.value = true;
  try {
    const filePath = await save({
      defaultPath: formatExportFilename(),
      filters: [
        { name: "Markdown", extensions: ["md"] },
        { name: "Text", extensions: ["txt"] },
      ],
    });
    if (!filePath) return;

    const content = buildExportContent();
    // 修复：日志导出改走 Rust 端写盘，避免前端文件插件在部分环境下无效。
    const savedPath = await seekMindApi.exportLogMarkdown(filePath, content);
    console.info(`[SeekMind] log export saved to ${filePath}`);
    exportTone.value = "success";
    exportInfoMessage.value = t("logPanel.exported", { path: savedPath });
  } catch (error) {
    console.error("[SeekMind] log export failed", error);
    exportTone.value = "error";
    exportInfoMessage.value = error instanceof Error ? error.message : t("logPanel.exportFailed");
  } finally {
    exporting.value = false;
  }
};

const toggleExpanded = () => {
  if (entries.value.length === 0) {
    expanded.value = false;
    return;
  }
  expanded.value = !expanded.value;
};

const onResizeStart = (event: MouseEvent) => {
  dragging.value = true;
  dragStartY.value = event.clientY;
  dragStartHeight.value = panelHeight.value;
  if (!expanded.value) {
    expanded.value = true;
    panelHeight.value = HEIGHT_DEFAULT;
    dragStartHeight.value = HEIGHT_DEFAULT;
  }
  document.addEventListener("mousemove", onResizeMove);
  document.addEventListener("mouseup", onResizeEnd);
};

const onResizeMove = (event: MouseEvent) => {
  if (!dragging.value) return;
  const delta = dragStartY.value - event.clientY;
  const newHeight = Math.max(HEIGHT_MIN, Math.min(HEIGHT_MAX, dragStartHeight.value + delta));
  panelHeight.value = newHeight;
};

const onResizeEnd = () => {
  dragging.value = false;
  saveHeight(panelHeight.value);
  document.removeEventListener("mousemove", onResizeMove);
  document.removeEventListener("mouseup", onResizeEnd);
};

const panelStyle = computed(() => {
  // 修复：日志为空时避免撑出大块空白区域，仅保留紧凑状态栏。
  if (!expanded.value) return { height: `${HEIGHT_COLLAPSED}px` };
  if (entries.value.length === 0) {
    return { height: `${HEIGHT_COLLAPSED}px` };
  }
  return { height: `${panelHeight.value}px` };
});

const contentStyle = computed(() => {
  if (!expanded.value || entries.value.length === 0) return {};
  return { height: `calc(100% - ${HEADER_H + DIVIDER_H}px)` };
});

const showContent = computed(() => expanded.value && entries.value.length > 0);

onMounted(() => {
  void installListeners();
  void loadMetrics();
});

onBeforeUnmount(() => {
  unlistenIndex?.();
  unlistenDocument?.();
  unlistenSemantic?.();
  unlistenStorageReset?.();
  unlistenIndex = null;
  unlistenDocument = null;
  unlistenSemantic = null;
  unlistenStorageReset = null;
});
</script>

<template>
  <SeekMindToast v-if="exportInfoMessage" :message="exportInfoMessage" :tone="exportTone" />
  <div
    class="relative shrink-0 bg-panel"
    :class="dragging ? 'select-none' : ''"
    :style="panelStyle"
  >
    <div
      class="h-[3px] w-full cursor-ns-resize shrink-0 transition-colors"
      :class="dragging ? 'bg-accent' : 'bg-border hover:bg-accent active:bg-accent'"
      @mousedown.prevent="onResizeStart"
    />

    <div
      class="group flex h-7 w-full flex-nowrap items-center justify-between gap-3 overflow-x-auto px-3 text-left transition-colors hover:bg-surface-hover"
      role="button"
      tabindex="0"
      @click="toggleExpanded"
      @keydown.enter.prevent="toggleExpanded"
      @keydown.space.prevent="toggleExpanded"
    >
      <div class="flex min-w-0 shrink-0 items-center gap-3 whitespace-nowrap">
        <div class="flex min-w-0 items-center gap-2 whitespace-nowrap">
          <!-- 修复：底栏标题区改成明确的按钮胶囊，并使用跨平台稳定的 lucide 图标，避免 Windows 下自定义 SVG 偶发不显示。 -->
          <span class="inline-flex items-center gap-1.5 rounded-full border border-default bg-surface px-2.5 py-1 text-[11px] font-medium text-secondary shadow-sm transition group-hover:border-accent/40 group-hover:bg-surface-hover group-hover:text-primary">
            <span class="inline-flex h-5 w-5 items-center justify-center rounded-full bg-accent-soft text-accent">
              <FileText :size="13" />
            </span>
            <span class="truncate">{{ t("logPanel.title") }}</span>
            <ChevronsUpDown :size="12" class="text-dim" />
          </span>
        </div>
        <!-- 修复：底 bar 中状态与操作分组展示，避免“信息”和“动作”混在一列。 -->
        <div class="hidden lg:flex items-center gap-2 whitespace-nowrap">
          <span class="inline-flex items-center gap-1 whitespace-nowrap rounded-full border border-emerald-soft bg-emerald-soft px-2 py-0.5 text-[11px] text-success">
            <span class="h-1.5 w-1.5 rounded-full bg-success" />
            {{ t("sidebar.statusRunning") }}
          </span>
          <span
            v-for="stat in sidebarStats"
            :key="stat.label"
            class="inline-flex items-center gap-1 whitespace-nowrap rounded-full border border-default bg-badge px-2 py-0.5 text-[11px] text-secondary"
          >
            {{ stat.value }}
            <span class="text-muted">{{ stat.label }}</span>
          </span>
        </div>
      </div>
      <div class="flex min-w-0 shrink-0 items-center gap-2 whitespace-nowrap text-[11px] text-muted">
        <SeekMindBadge
          v-for="metric in bottomMetrics"
          :key="metric.key"
          :tone="metric.tone"
        >
          {{ metric.label }}
        </SeekMindBadge>
        <!-- 修复：事件数量和“日志”标题语义重复，保留标题即可。 -->
        <button
          class="inline-flex shrink-0 items-center gap-1 whitespace-nowrap rounded-full border border-default bg-surface px-2 py-0.5 text-[11px] text-secondary transition hover:bg-surface-hover hover:text-primary disabled:opacity-50"
          :disabled="exporting"
          :title="t('logPanel.export')"
          @click.stop="exportLogs"
        >
          <SeekMindIcon icon="icon-export" :size="13" />
          <span class="hidden sm:inline">{{ t("logPanel.export") }}</span>
        </button>
      </div>
    </div>

      <div v-if="showContent" class="flex min-h-0 flex-col" :style="contentStyle">
        <div class="flex items-center justify-end border-t border-light px-4 py-2 text-xs text-dim shrink-0">
          <button class="inline-flex items-center gap-1 hover:text-secondary" @click="clearLogs">
            <Trash2 :size="13" /> {{ t("logPanel.clear") }}
          </button>
        </div>
        <div class="flex-1 overflow-y-auto p-2">
          <!-- 修复：日志表格把正文信息集中在左侧，日期独立在右侧。 -->
          <div class="overflow-hidden rounded-2xl border border-light bg-panel">
            <div class="grid grid-cols-[minmax(0,1fr)_170px] border-b border-light bg-surface/60 px-3 py-2 text-[11px] font-medium text-muted">
              <div>{{ t("logPanel.table.content") }}</div>
              <div class="text-right">{{ t("logPanel.table.date") }}</div>
            </div>
            <div
              v-for="entry in entries"
              :key="entry.id"
              class="grid grid-cols-[minmax(0,1fr)_170px] border-b border-light last:border-b-0"
            >
              <div class="min-w-0 px-3 py-2">
                <div class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
                  <SeekMindBadge :tone="levelTone[entry.level]">{{ t(scopeMeta[entry.scope].label) }}</SeekMindBadge>
                  <div class="min-w-0 truncate text-sm font-medium text-primary">{{ entry.title }}</div>
                  <span class="text-[11px] text-dim">·</span>
                  <div class="min-w-0 truncate text-[12px] text-secondary">{{ entry.message }}</div>
                  <span class="text-[11px] text-dim">·</span>
                  <div class="min-w-0 truncate text-[11px] text-muted">{{ entry.details }}</div>
                  <span
                    v-if="entry.warning"
                    class="inline-flex items-center rounded-full border border-amber-soft bg-amber-soft px-2 py-0.5 text-[11px] leading-4 text-warning"
                  >
                    {{ entry.warning }}
                  </span>
                </div>
              </div>
              <div class="flex items-start justify-end px-3 py-2 text-[11px] leading-5 text-muted">
                {{ formatEntryTimestamp(entry.timestamp) }}
              </div>
            </div>
          </div>
        </div>
      </div>
  </div>
</template>
