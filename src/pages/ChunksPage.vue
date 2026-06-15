/**
 * @author MorningSun
 * @CreatedDate 2026/06/02
 * @Description 切片页面，展示文档切片、预览与局部刷新操作。
 */
<script setup lang="ts">
import { computed, onActivated, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";
import { AlertCircle, ClipboardCopy, Cpu, Eye, FileText, Layers3, RefreshCw, ScanText, SquareArrowOutUpRight, Trash2, X } from "lucide-vue-next";
import { listen } from "@tauri-apps/api/event";
import SeekMindIndexTree from "../components/SeekMind/SeekMindIndexTree.vue";
import SeekMindBadge from "../components/SeekMind/SeekMindBadge.vue";
import SeekMindDetailPanel from "../components/SeekMind/SeekMindDetailPanel.vue";
import SeekMindDetailSection from "../components/SeekMind/SeekMindDetailSection.vue";
import SeekMindContextMenu from "../components/SeekMind/SeekMindContextMenu.vue";
import type { ContextMenuItem } from "../components/SeekMind/SeekMindContextMenu.vue";
import SeekMindFileIcon from "../components/SeekMind/SeekMindFileIcon.vue";
import SeekMindPreviewBlockRenderer from "../components/SeekMind/SeekMindPreviewBlockRenderer.vue";
import SeekMindMarkdownRenderer from "../components/SeekMind/SeekMindMarkdownRenderer.vue";
import SplitPane from "../components/SplitPane.vue";
import { seekMindApi, formatSeekMindError } from "../services/seekMindApi";
import { useIndexDirTree } from "../composables/useIndexDirTree";
import { useIndexDirs } from "../composables/useIndexDirs";
import { buildDocumentLocationParts, formatDocumentCitation, resolveDocumentTitlePath } from "../utils/citation";
import type {
  ChunkView,
  DocumentRefreshProgressView,
  DocumentView,
  ParserRuntimeView,
  PreviewBlockView,
} from "../types/SeekMind";

const { t } = useI18n();

const route = useRoute();
const router = useRouter();

const documents = ref<DocumentView[]>([]);
const chunks = ref<ChunkView[]>([]);
const selectedDirPath = ref("");
const selectedDocPath = ref("");
// 右侧详情面板关闭时直接从 SplitPane 移除，避免只清空内容但右栏仍占位。
const showDetailPanel = ref(false);
const parserRuntime = ref<ParserRuntimeView | null>(null);
const loading = ref(false);
const loadingDocs = ref(false);
const loadingChunks = ref(false);
const refreshingDocPath = ref("");
const refreshQueue = ref<DocumentView[]>([]);
const refreshWorkerRunning = ref(false);
const refreshWarnings = ref<Record<string, string>>({});
const refreshOutcomes = ref<Record<string, "python" | "rust" | "failed">>({});
const refreshStates = ref<Record<string, "idle" | "queued" | "running" | "completed" | "failed">>({});
const refreshActiveSources = ref<Record<string, "python" | "rust">>({});
const refreshErrors = ref<Record<string, string>>({});
const errorMessage = ref("");
const actionMessage = ref("");
const actionErrorMessage = ref("");
const docFilter = ref("");
const routeSyncReady = ref(false);
const hasInitializedSelection = ref(false);
const lastRoutePath = ref("");
const refreshJobResolvers = new Map<string, (payload: DocumentRefreshProgressView) => void>();
const refreshJobBufferedEvents = new Map<string, DocumentRefreshProgressView>();
const refreshJobPaths = new Map<string, string>();
let chunksPageActivatedOnce = false;

const contextMenuDoc = ref<DocumentView | null>(null);
const contextMenuPosition = ref({ x: 0, y: 0 });
const contextMenuVisible = ref(false);

let unlistenRefreshProgress: null | (() => void) = null;

const { dirs, refreshIndexDirs } = useIndexDirs();
const {
  visibleRows: chunkDirRows,
  expandAncestors: expandChunkDirAncestors,
  setExpanded: setChunkDirExpanded,
} = useIndexDirTree(dirs);

const explicitIndexDirCount = computed(() => dirs.value.filter((dir) => dir.is_explicit).length);

const currentDocument = computed(
  () => documents.value.find((item) => item.path === selectedDocPath.value) ?? null,
);

const isImageDocument = computed(() => {
  const ext = currentDocument.value?.ext?.trim().toLowerCase().replace(/^\./, "");
  return Boolean(ext && ["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff", "heic"].includes(ext));
});

const currentDocumentPreviewBlock = computed<PreviewBlockView | null>(() => {
  if (!currentDocument.value || !isImageDocument.value) {
    return null;
  }

  // 图片文档的 chunk 主要承载 OCR 文本，右侧详情需要额外显示原图预览，避免只能看到识别结果看不到素材本身。
  return {
    block_index: 0,
    block_type: "image",
    text: currentDocument.value.file_name,
    heading: currentDocument.value.file_name,
    level: null,
    page: null,
    language: null,
    markdown: "",
    html: "",
    asset_path: currentDocument.value.path,
    alt_text: currentDocument.value.file_name,
    caption: currentDocument.value.path,
    ocr_text: "",
  };
});

const selectedDir = computed(
  () => dirs.value.find((item) => item.path === selectedDirPath.value) ?? null,
);

const formatDirectoryLabel = (path: string, tailSegments = 1) => {
  if (!path.trim()) {
    return path;
  }

  // 长目录名优先暴露末尾的关键路径，避免前缀把有意义的信息挤出可视区。
  const segments = path.split(/[\\/]+/).filter(Boolean);
  if (segments.length <= tailSegments) {
    return segments.join(" / ");
  }

  const tail = segments.slice(-tailSegments).join(" / ");
  return `${tail} · ${path}`;
};

const splitPanels = computed(() => [
  { key: "center", minSize: 320, flex: true },
  ...(showDetailPanel.value ? [{ key: "right", minSize: 380, flex: true }] : []),
]);

const queuedDocPaths = computed(() => new Set(refreshQueue.value.map((doc) => doc.path)));

const refreshTaskCount = computed(
  () => refreshQueue.value.length + (refreshingDocPath.value ? 1 : 0),
);

const hasRefreshOutcome = (path: string) =>
  refreshOutcomes.value[path] === "python" ||
  refreshOutcomes.value[path] === "rust" ||
  refreshOutcomes.value[path] === "failed";

const isDocRefreshing = (path: string) =>
  !hasRefreshOutcome(path) && refreshStates.value[path] === "running";
const isDocRefreshBusy = (path: string) =>
  !hasRefreshOutcome(path) &&
  (refreshStates.value[path] === "running" || refreshStates.value[path] === "queued");

const isTerminalRefreshState = (path: string) =>
  refreshStates.value[path] === "completed" || refreshStates.value[path] === "failed";

const clearRefreshResult = (path: string) => {
  const { [path]: _warning, ...restWarnings } = refreshWarnings.value;
  refreshWarnings.value = restWarnings;

  const { [path]: _outcome, ...restOutcomes } = refreshOutcomes.value;
  refreshOutcomes.value = restOutcomes;

  const { [path]: _error, ...restErrors } = refreshErrors.value;
  refreshErrors.value = restErrors;
};

const clearActiveRefreshSource = (path: string) => {
  const { [path]: _source, ...restSources } = refreshActiveSources.value;
  refreshActiveSources.value = restSources;
};

const applyRefreshTerminalPayload = (path: string, payload: DocumentRefreshProgressView) => {
  const completed = payload.state === "completed";
  refreshStates.value = {
    ...refreshStates.value,
    [path]: completed ? "completed" : "failed",
  };

  if (completed) {
    refreshOutcomes.value = {
      ...refreshOutcomes.value,
      [path]: payload.warning || payload.parser_source === "rust" ? "rust" : "python",
    };
  } else {
    refreshOutcomes.value = {
      ...refreshOutcomes.value,
      [path]: "failed",
    };
  }

  if (payload.warning) {
    refreshWarnings.value = {
      ...refreshWarnings.value,
      [path]: payload.warning,
    };
  } else {
    const { [path]: _removed, ...rest } = refreshWarnings.value;
    refreshWarnings.value = rest;
  }

  clearActiveRefreshSource(path);
};

const refreshAfterTerminalPayload = async (path: string, payload: DocumentRefreshProgressView) => {
  if (payload.state !== "completed") {
    return;
  }

  if (path.startsWith(selectedDirPath.value)) {
    await loadDocuments();
  }

  if (selectedDocPath.value === path) {
    await loadChunks();
  }
};

const currentDocumentCitation = computed(() => {
  if (!currentDocument.value) {
    return "";
  }

  const firstChunk = chunks.value[0];
  return formatDocumentCitation({
    fileName: currentDocument.value.file_name,
    titlePath: resolveDocumentTitlePath({
      fileName: currentDocument.value.file_name,
      titlePath: firstChunk?.title_path,
      heading: firstChunk?.heading,
    }),
    locationParts: firstChunk
      ? buildDocumentLocationParts({
          page: firstChunk.page,
          paragraph: firstChunk.paragraph,
          pageLabel: t("page.chunks.page", { page: firstChunk.page ?? 0 }),
          paragraphLabel: t("page.chunks.paragraph", { para: firstChunk.paragraph ?? 1 }),
        })
      : [t("page.chunks.selectDoc")],
  });
});

const filteredDocuments = computed(() => {
  const keyword = docFilter.value.trim().toLowerCase();
  if (!keyword) {
    return documents.value;
  }

  return documents.value.filter((doc) => {
    return [doc.file_name, doc.path, doc.ext].some((value) =>
      value.toLowerCase().includes(keyword),
    );
  });
});

const resolveDirFromPath = (path?: string | string[]) => {
  if (typeof path !== "string" || !path.trim()) {
    return "";
  }

  const candidate = dirs.value
    .map((dir) => dir.path)
    .filter((dir) => path.startsWith(dir))
    .sort((a, b) => b.length - a.length)[0];

  return candidate ?? "";
};

const refreshDirSelection = async (reason: string) => {
  console.info("[SeekMind] chunks refresh dir selection", {
    reason,
    routePath: getRouteTargetPath(),
  });

  try {
    await refreshIndexDirs(reason);
    await syncSelection(true);
  } catch (error) {
    console.error("[SeekMind] refreshDirSelection failed", {
      reason,
      error,
    });
  }
};

const loadParserRuntime = async () => {
  parserRuntime.value = await seekMindApi.getParserRuntime();
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

const loadDocuments = async () => {
  if (!selectedDirPath.value) {
    documents.value = [];
    return;
  }

  loadingDocs.value = true;
  try {
    documents.value = await seekMindApi.listDocumentsInDir(selectedDirPath.value);
  } catch (error) {
    errorMessage.value = formatSeekMindError(error, t("page.chunks.section.docList"));
    console.error("[SeekMind] listDocumentsInDir failed", error);
    documents.value = [];
  } finally {
    loadingDocs.value = false;
  }
};

const loadChunks = async () => {
  if (!selectedDocPath.value) {
    chunks.value = [];
    return;
  }

  loadingChunks.value = true;
  try {
    chunks.value = await seekMindApi.listDocumentChunks(selectedDocPath.value);
  } catch (error) {
    errorMessage.value = formatSeekMindError(error, t("page.chunks.section.chunkDetail"));
    console.error("[SeekMind] listDocumentChunks failed", error);
    chunks.value = [];
  } finally {
    loadingChunks.value = false;
  }
};

const getRouteTargetPath = () => (typeof route.query.path === "string" ? route.query.path : "");

const syncSelection = async (forceReload = false) => {
  errorMessage.value = "";

  try {
    const targetPath = getRouteTargetPath();
    const routeChanged = targetPath !== lastRoutePath.value;
    const targetDir = resolveDirFromPath(targetPath);
    const fallbackDir = dirs.value.find((dir) => dir.enabled)?.path || dirs.value[0]?.path || "";
    const nextDir = targetDir || (hasInitializedSelection.value ? selectedDirPath.value : fallbackDir);
    const dirChanged = nextDir !== selectedDirPath.value;
    const needsDocsReload = forceReload || dirChanged || documents.value.length === 0;

    if (needsDocsReload) {
      selectedDirPath.value = nextDir;
      if (nextDir) {
        expandChunkDirAncestors(nextDir);
      }
      if (!selectedDirPath.value) {
        documents.value = [];
        selectedDocPath.value = "";
        chunks.value = [];
        return;
      }

      await loadDocuments();
    }

    const matchedDoc = documents.value.find((doc) => doc.path === targetPath);
    const nextDocPath = matchedDoc?.path ?? documents.value[0]?.path ?? "";
    selectedDocPath.value = nextDocPath;
    showDetailPanel.value = Boolean(nextDocPath);

    if (routeChanged || forceReload || dirChanged || chunks.value.length === 0) {
      await loadChunks();
    }

    lastRoutePath.value = targetPath;
    hasInitializedSelection.value = true;
  } catch (error) {
    errorMessage.value = formatSeekMindError(error, t("page.chunks.title"));
    console.error("[SeekMind] chunks syncSelection failed", error);
  }
};

const switchDirectory = async (path: string) => {
  if (!path || path === selectedDirPath.value) {
    return;
  }

  errorMessage.value = "";
  actionMessage.value = "";
  actionErrorMessage.value = "";
  selectedDirPath.value = path;
  expandChunkDirAncestors(path);
  selectedDocPath.value = "";
  showDetailPanel.value = false;
  documents.value = [];
  chunks.value = [];
  hasInitializedSelection.value = true;
  loadingDocs.value = true;
  void router.replace({ query: { ...route.query, path } });
};

const handleDirTreeSelect = (path: string) => {
  void switchDirectory(path);
};

const handleDirTreeToggle = (path: string, expanded: boolean) => {
  setChunkDirExpanded(path, expanded);
};

const setActionMessage = (message: string) => {
  actionErrorMessage.value = "";
  actionMessage.value = message;
};

const setActionError = (message: string) => {
  actionMessage.value = "";
  actionErrorMessage.value = message;
};

const copyText = async (text: string, successMessage: string) => {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
    } else {
      const textarea = document.createElement("textarea");
      textarea.value = text;
      textarea.setAttribute("readonly", "true");
      textarea.style.position = "fixed";
      textarea.style.left = "-9999px";
      document.body.appendChild(textarea);
      textarea.select();
      const copied = document.execCommand("copy");
      document.body.removeChild(textarea);
      if (!copied) {
        throw new Error("copy failed");
      }
    }
    setActionMessage(successMessage);
  } catch (error) {
    console.error("[SeekMind] copyText failed", error);
    setActionError(t("page.chunks.action.copyFailed"));
  }
};

const openCurrentDocument = async () => {
  if (!currentDocument.value) {
    return;
  }

  await seekMindApi.openFile(currentDocument.value.path);
};

const quickLookCurrentDocument = async () => {
  if (!currentDocument.value) {
    return;
  }

  try {
    await seekMindApi.quickLookFile(currentDocument.value.path);
    setActionMessage(t("page.chunks.action.quickLookOpened"));
  } catch (error) {
    setActionError(error instanceof Error ? error.message : t("page.chunks.action.quickLookFailed"));
  }
};

const copyCurrentDocumentPath = async () => {
  if (!currentDocument.value) {
    return;
  }

  await copyText(currentDocument.value.path, t("page.chunks.action.copiedPath"));
};

const closeCurrentDocument = () => {
  console.debug("[SeekMind] chunks detail panel closed", {
    docPath: selectedDocPath.value,
  });
  selectedDocPath.value = "";
  chunks.value = [];
  showDetailPanel.value = false;
};

const copyChunkCitation = async (chunk: ChunkView) => {
  if (!currentDocument.value) {
    return;
  }

  await copyText(
    formatDocumentCitation({
      fileName: currentDocument.value.file_name,
      titlePath: chunk.title_path,
      heading: chunk.heading,
      locationParts: buildDocumentLocationParts({
        page: chunk.page,
        paragraph: chunk.paragraph,
        pageLabel: t("page.chunks.page", { page: chunk.page ?? 0 }),
        paragraphLabel: t("page.chunks.paragraph", { para: chunk.paragraph ?? 1 }),
      }),
    }),
    t("page.chunks.action.copiedCitation"),
  );
};

const waitForRefreshJob = (jobId: string) => {
  const buffered = refreshJobBufferedEvents.get(jobId);
  if (buffered) {
    refreshJobBufferedEvents.delete(jobId);
    return Promise.resolve(buffered);
  }

  return new Promise<DocumentRefreshProgressView>((resolve) => {
    refreshJobResolvers.set(jobId, resolve);
  });
};

const installRefreshProgressListener = async () => {
  if (unlistenRefreshProgress) {
    return;
  }

  unlistenRefreshProgress = await listen<DocumentRefreshProgressView>(
    "seekmind:document-refresh-progress",
    (event) => {
      const payload = event.payload;
      const path = refreshJobPaths.get(payload.job_id) ?? payload.path;

      if (payload.state === "running") {
        if (!isTerminalRefreshState(path)) {
          refreshStates.value = {
            ...refreshStates.value,
            [path]: "running",
          };
          refreshActiveSources.value = {
            ...refreshActiveSources.value,
            [path]: payload.parser_source,
          };
        }
        return;
      }

      applyRefreshTerminalPayload(path, payload);
      refreshJobPaths.delete(payload.job_id);
      if (refreshingDocPath.value === path) {
        refreshingDocPath.value = "";
      }
      void refreshAfterTerminalPayload(path, payload);

      const resolver = refreshJobResolvers.get(payload.job_id);
      if (resolver) {
        refreshJobResolvers.delete(payload.job_id);
        resolver(payload);
      } else {
        refreshJobBufferedEvents.set(payload.job_id, payload);
      }
    },
  );
};

void installRefreshProgressListener();

const processRefreshQueue = async () => {
  if (refreshWorkerRunning.value) {
    return;
  }

  refreshWorkerRunning.value = true;

  try {
    while (refreshQueue.value.length > 0) {
      const doc = refreshQueue.value.shift();
      if (!doc) {
        continue;
      }

      refreshingDocPath.value = doc.path;
      refreshStates.value = {
        ...refreshStates.value,
        [doc.path]: "running",
      };
      errorMessage.value = "";

      try {
        const refreshStart = await seekMindApi.refreshDocument(doc.path, doc.dir_path);
        refreshJobPaths.set(refreshStart.job_id, doc.path);
        const refreshResult = await waitForRefreshJob(refreshStart.job_id);
        refreshJobPaths.delete(refreshStart.job_id);

        applyRefreshTerminalPayload(doc.path, refreshResult);
        if (refreshingDocPath.value === doc.path) {
          refreshingDocPath.value = "";
        }

        if (refreshResult.state === "completed" && doc.dir_path === selectedDirPath.value) {
          await loadDocuments();
          if (selectedDocPath.value === doc.path) {
            await loadChunks();
          }
        }
        if (refreshResult.state === "failed") {
          refreshStates.value = {
            ...refreshStates.value,
            [doc.path]: "failed",
          };
          const errorMsg = formatSeekMindError(
            refreshResult.message,
            `${t("page.chunks.btn.reslice")}：${doc.file_name}`,
          );
          errorMessage.value = errorMsg;
          refreshErrors.value = { ...refreshErrors.value, [doc.path]: errorMsg };
        }
      } catch (error) {
        refreshStates.value = {
          ...refreshStates.value,
          [doc.path]: "failed",
        };
        refreshOutcomes.value = {
          ...refreshOutcomes.value,
          [doc.path]: "failed",
        };
        clearActiveRefreshSource(doc.path);
        const { [doc.path]: _removed, ...rest } = refreshWarnings.value;
        refreshWarnings.value = rest;
        const errorMsg = formatSeekMindError(error, `${t("page.chunks.btn.reslice")}：${doc.file_name}`);
        errorMessage.value = errorMsg;
        refreshErrors.value = { ...refreshErrors.value, [doc.path]: errorMsg };
        console.error("[SeekMind] refreshDocument failed", error);
      }
    }
  } finally {
    refreshingDocPath.value = "";
    refreshWorkerRunning.value = false;
  }
};

const refreshDocument = async (doc: DocumentView) => {
  if (refreshingDocPath.value === doc.path || queuedDocPaths.value.has(doc.path)) {
    return;
  }

  refreshStates.value = {
    ...refreshStates.value,
    [doc.path]: "queued",
  };
  clearRefreshResult(doc.path);
  clearActiveRefreshSource(doc.path);
  refreshQueue.value.push(doc);
  void processRefreshQueue();
};

const refreshPdfOcrDocument = async (doc: DocumentView) => {
  if (refreshingDocPath.value === doc.path || queuedDocPaths.value.has(doc.path)) {
    return;
  }

  refreshingDocPath.value = doc.path;
  errorMessage.value = "";
  actionMessage.value = "";

  try {
    await seekMindApi.refreshPdfOcrDocument(doc.path);
    if (doc.dir_path === selectedDirPath.value) {
      await loadDocuments();
      if (selectedDocPath.value === doc.path) {
        await loadChunks();
      }
    }
    setActionMessage(t("page.chunks.action.ocrRetried", { name: doc.file_name }));
  } catch (error) {
    setActionError(formatSeekMindError(error, t("page.chunks.action.ocrRetryFailed")));
    console.error("[SeekMind] refreshPdfOcrDocument failed", error);
  } finally {
    refreshingDocPath.value = "";
  }
};

const selectDoc = async (path: string) => {
  showDetailPanel.value = Boolean(path);
  void router.replace({ query: { ...route.query, path } });
};

const refreshStateLabel = (path: string) => {
  if (refreshOutcomes.value[path] === "failed") {
    return t("page.chunks.btn.retrySlice");
  }
  if (hasRefreshOutcome(path)) {
    return t("page.chunks.btn.reslice");
  }

  const state = refreshStates.value[path] ?? "idle";
  if (state === "running") {
    const source = refreshActiveSources.value[path];
    return source === "python" ? t("page.chunks.refreshState.pythonSlicing") : source === "rust" ? t("page.chunks.refreshState.rustSlicing") : t("page.chunks.refreshState.slicing");
  }
  if (state === "queued") {
    return t("page.chunks.refreshState.queued");
  }
  if (state === "completed") {
    return t("page.chunks.btn.reslice");
  }
  if (state === "failed") {
    return t("page.chunks.btn.retrySlice");
  }
  return t("page.chunks.btn.reslice");
};

const refreshOutcomeLabel = (path: string) => {
  const outcome = refreshOutcomes.value[path];
  if (outcome === "python") {
    return t("page.chunks.refreshState.pythonDone");
  }
  if (outcome === "rust") {
    return t("page.chunks.refreshState.rustFallback");
  }
  if (outcome === "failed") {
    return t("page.chunks.refreshState.failed");
  }
  return "";
};

const chunkContextMenuItems = computed<ContextMenuItem[]>(() => {
  const doc = contextMenuDoc.value;
  if (!doc) return [];

  const resliceLabel = refreshOutcomes.value[doc.path] === "failed"
    ? t("page.chunks.btn.retrySlice")
    : t("page.chunks.btn.reslice");

  return [
    {
      key: "reslice",
      label: resliceLabel,
      icon: RefreshCw,
      disabled: isDocRefreshBusy(doc.path),
      handler: () => refreshDocument(doc),
    },
    {
      key: "retry-ocr",
      label: t("page.chunks.action.retryOcr"),
      icon: ScanText,
      disabled: isDocRefreshBusy(doc.path) || doc.ext.toLowerCase() !== "pdf",
      handler: () => refreshPdfOcrDocument(doc),
    },
    { key: "divider-actions", label: "", divider: true },
    {
      key: "quickLook",
      label: t("page.chunks.action.quickLook"),
      icon: Eye,
      handler: () => quickLookCurrentDocument(),
    },
    {
      key: "openFile",
      label: t("common.openFile"),
      icon: SquareArrowOutUpRight,
      handler: () => openCurrentDocument(),
    },
    {
      key: "copyPath",
      label: t("page.chunks.action.copyPath"),
      icon: ClipboardCopy,
      handler: () => copyCurrentDocumentPath(),
    },
    { key: "divider-delete", label: "", divider: true },
    {
      key: "delete",
      label: t("common.delete"),
      icon: Trash2,
      danger: true,
      handler: () => deleteCurrentDocument(),
    },
  ];
});

const handleDocContextMenu = (doc: DocumentView, event: MouseEvent) => {
  selectedDocPath.value = doc.path;
  contextMenuDoc.value = doc;
  contextMenuPosition.value = { x: event.clientX, y: event.clientY };
  contextMenuVisible.value = true;
};

const deleteCurrentDocument = async () => {
  const doc = contextMenuDoc.value;
  if (!doc) return;

  try {
    await seekMindApi.deleteDocument(doc.path);
    refreshErrors.value = { ...refreshErrors.value, [doc.path]: "" };
    if (selectedDocPath.value === doc.path) {
      selectedDocPath.value = "";
      chunks.value = [];
      showDetailPanel.value = false;
    }
    await loadDocuments();
    setActionMessage(t("page.chunks.action.deleted"));
  } catch (error) {
    setActionError(formatSeekMindError(error, t("page.chunks.action.deleteFailed")));
  }
};

onMounted(() => {
  loading.value = true;
  void (async () => {
    try {
      await loadParserRuntime();
      await refreshDirSelection("mounted");
      routeSyncReady.value = true;
    } finally {
      loading.value = false;
    }
  })();
  void installRefreshProgressListener();
});

onActivated(() => {
  // 修复：ChunksPage 被 KeepAlive 缓存后，切换回页面不会再次触发 onMounted，
  // 如果前面在状态页新增了目录，这里必须重新拉一次目录并同步当前选择。
  if (!chunksPageActivatedOnce) {
    chunksPageActivatedOnce = true;
    return;
  }

  if (!routeSyncReady.value) {
    return;
  }

  void refreshDirSelection("activated");
});

onBeforeUnmount(() => {
  if (unlistenRefreshProgress) {
    unlistenRefreshProgress();
    unlistenRefreshProgress = null;
  }
  refreshJobResolvers.clear();
  refreshJobBufferedEvents.clear();
});

watch(
  () => route.query.path,
  () => {
    if (!routeSyncReady.value) {
      return;
    }

    void syncSelection();
  },
);
</script>

<template>
  <div class="m-3 flex h-full min-h-0 flex-col overflow-hidden bg-transparent text-primary">
    <!-- Keep the chunks page header compact so the desktop layout stays flat and easy to scan. -->
    <header class="flex items-center justify-between gap-4 px-3 pb-2 pt-1">
      <div class="min-w-0">
        <div class="flex items-center gap-2">
          <span class="inline-flex h-8 w-8 items-center justify-center rounded-[10px] bg-white/72 text-accent" aria-hidden="true">
            <Layers3 :size="17" />
          </span>
          <div class="min-w-0">
            <h1 class="truncate text-[16px] font-semibold leading-6 tracking-[-0.01em] text-primary">
              {{ t("page.chunks.title") }}
            </h1>
            <p class="mt-0.5 truncate text-[12px] leading-5 text-muted">
              {{ t("page.chunks.subtitle") }}
            </p>
          </div>
        </div>
      </div>
      <div class="flex shrink-0 items-center gap-2">
        <div class="hidden sm:block text-xs text-muted">
          {{ t("page.chunks.parserInfo") }}
          <span class="font-medium text-secondary">{{ parserRuntime?.active === "python" ? t("page.chunks.parserPython") : t("page.chunks.parserRust") }}</span>
          {{ t("page.chunks.parserInfo2") }}
        </div>
        <SeekMindBadge :tone="parserRuntime?.active === 'python' ? 'success' : 'warning'">
          <Cpu class="mr-1" :size="13" />
          {{ parserRuntime?.active === 'python' ? t("page.chunks.badgePython") : t("page.chunks.badgeRust") }}
        </SeekMindBadge>
      </div>
    </header>

    <div
      v-if="officeNotice"
      class="mx-3 mb-2 rounded-[18px] bg-amber-soft px-4 py-3"
    >
      <div class="flex items-start gap-3">
        <AlertCircle :size="16" class="mt-0.5 shrink-0 text-warning" />
        <div class="min-w-0">
          <div class="text-sm font-medium text-warning">
            {{ officeNotice.title }}
          </div>
          <div class="seekmind-item-meta mt-1 leading-5 text-secondary">
            {{ officeNotice.desc }}
          </div>
          <div class="seekmind-item-meta mt-1 leading-5">
            {{ officeNotice.hint }}
          </div>
        </div>
      </div>
    </div>

    <div v-if="errorMessage" class="mx-3 mb-2 rounded-[18px] bg-danger-soft px-4 py-3 text-sm text-danger">
      {{ errorMessage }}
    </div>

    <main class="flex min-h-0 flex-1 overflow-hidden">
      <SplitPane :panels="splitPanels">
      <template #center>
        <section class="seekmind-pane-center flex min-h-0 flex-1 flex-col overflow-hidden px-3 py-3">
            <div class="shrink-0 mb-3 rounded-[18px] bg-white/72 px-3 py-2.5">
              <div class="mb-2 flex items-center justify-between gap-2">
                <div class="text-sm font-medium text-primary">{{ t("page.chunks.dirSelector.label") }}</div>
                <SeekMindBadge v-if="selectedDir" tone="default">
                  {{ t("page.chunks.dirSelector.stats", { docs: selectedDir.docs, chunks: selectedDir.chunks }) }}
                </SeekMindBadge>
              </div>
              <div class="max-h-[220px] overflow-y-auto pr-1">
                <SeekMindIndexTree
                  :rows="chunkDirRows"
                  :selected-path="selectedDirPath"
                  density="compact"
                  :empty-text="t('page.chunks.empty.dirs')"
                  @node-select="handleDirTreeSelect"
                  @toggle="handleDirTreeToggle"
                >
                  <template #label="{ row }">
                    <span class="truncate">{{ row.displayName }}</span>
                  </template>
                </SeekMindIndexTree>
              </div>
            </div>

            <div class="shrink-0 mb-3 flex items-start justify-between gap-3">
              <div>
                <div class="text-sm font-medium text-primary">{{ t("page.chunks.section.docList") }}</div>
                <div class="mt-1 text-[12px] text-muted">
                  {{ currentDocument ? currentDocument.file_name : selectedDir ? formatDirectoryLabel(selectedDir.path) : t("page.chunks.selectDir") }}
                </div>
              </div>
              <div class="flex items-center gap-2">
                <SeekMindBadge tone="default">
                  <FileText class="mr-1" :size="13" />
                  {{ filteredDocuments.length }}
                </SeekMindBadge>
                <SeekMindBadge v-if="refreshTaskCount > 0" tone="warning">
                  <RefreshCw class="mr-1" :size="13" />
                  {{ t("page.chunks.btn.queue", { count: refreshTaskCount }) }}
                </SeekMindBadge>
              </div>
            </div>

            <div class="shrink-0 mb-3">
              <input
                v-model="docFilter"
                class="w-full rounded-[14px] bg-input px-3 py-2 text-sm outline-none"
                :placeholder="t('page.chunks.filterPlaceholder')"
              />
            </div>

            <div class="min-h-0 flex-1 overflow-y-auto pr-1">
              <div v-if="loadingDocs" class="text-sm text-dim">{{ t("page.chunks.readingDocs") }}</div>
              <div v-else-if="filteredDocuments.length === 0" class="rounded-[18px] bg-white/72 px-4 py-6 text-sm text-dim">
                {{ t("page.chunks.empty.docs") }}
              </div>
              <div v-else class="space-y-1.5">
                <div
                  v-for="doc in filteredDocuments"
                  :key="doc.id"
                  class="w-full rounded-[10px] px-2 py-1.5 text-left transition"
                  :class="selectedDocPath === doc.path ? 'bg-[rgba(0,122,255,0.10)]' : 'hover:bg-white/50'"
                  role="button"
                  tabindex="0"
                  @click="selectDoc(doc.path)"
                  @contextmenu.prevent="handleDocContextMenu(doc, $event)"
                >
                  <div class="flex items-start gap-2.5">
                    <SeekMindFileIcon :ext="doc.ext" />
                    <div class="min-w-0 flex-1">
                      <div class="truncate text-sm font-medium text-primary">{{ doc.file_name }}</div>
                      <div class="mt-0.5 truncate text-[11px] text-dim">{{ doc.path }}</div>
                      <div class="mt-1.5 flex items-center gap-2 text-[11px] text-dim">
                        <span>{{ t("page.chunks.chunkStats", { count: doc.chunks }) }}</span>
                        <span>·</span>
                        <span>{{ doc.modified }}</span>
                      </div>
                      <div
                        v-if="refreshErrors[doc.path]"
                        class="mt-1.5 rounded-[10px] bg-danger-soft px-2 py-1.5 text-[11px] leading-5 text-danger"
                      >
                        {{ refreshErrors[doc.path] }}
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </section>
        </template>

        <template #right>
        <section class="seekmind-pane-detail flex min-h-0 flex-1 flex-col overflow-hidden">
          <SeekMindDetailPanel v-if="currentDocument">
            <template #header>
              <div class="flex items-center justify-between gap-3">
                <div class="min-w-0">
                  <div class="text-sm font-medium text-primary">{{ t("page.chunks.section.chunkDetail") }}</div>
                  <div class="mt-1 text-[12px] text-muted">
                    {{ currentDocument.file_name }}
                  </div>
                  <div class="mt-1 break-all text-[11px] text-dim">
                    {{ currentDocument.path }}
                  </div>
                </div>
                <div class="flex items-center gap-2">
                  <button
                    class="seekmind-close-button"
                    type="button"
                    :title="t('common.close')"
                    @click="closeCurrentDocument"
                  >
                    <X :size="13" stroke-width="2.25" />
                  </button>
                  <button
                    class="inline-flex items-center gap-2 rounded-full bg-white/72 px-3 py-2 text-xs text-secondary hover:text-primary"
                    :disabled="loading || !selectedDirPath"
                    @click="void syncSelection()"
                  >
                    <RefreshCw :size="14" />
                    {{ t("page.chunks.btn.refresh") }}
                  </button>
                </div>
              </div>
              <div class="mt-2 min-h-[24px]" />
            </template>

            <SeekMindDetailSection :title="t('common.overview')" :subtitle="currentDocument.path">
              <div class="flex flex-wrap gap-2">
                <SeekMindBadge>{{ currentDocument.ext.toUpperCase() }}</SeekMindBadge>
                <SeekMindBadge>{{ t("page.chunks.chunkStats", { count: currentDocument.chunks }) }}</SeekMindBadge>
              </div>
              <div class="grid gap-2 text-xs leading-5 text-muted">
                <div>{{ t("page.chunks.titlePath") }}：{{ currentDocument.file_name }}</div>
                <div>{{ t("page.chunks.detail.imagePreview") }}：{{ isImageDocument ? t("common.available") : t("common.unavailable") }}</div>
              </div>
            </SeekMindDetailSection>

            <SeekMindDetailSection :title="t('common.originalText')">
              <div v-if="loadingChunks" class="text-sm text-dim">{{ t("page.chunks.readingChunks") }}</div>
              <div v-else-if="currentDocumentPreviewBlock" class="rounded-[18px] bg-white/72 p-3">
                <SeekMindPreviewBlockRenderer :block="currentDocumentPreviewBlock" />
              </div>
              <div v-else-if="chunks.length > 0" class="rounded-[18px] bg-white/72 p-3">
                <SeekMindMarkdownRenderer
                  :block="{
                    block_index: 0,
                    block_type: 'paragraph',
                    text: chunks[0].snippet,
                    heading: chunks[0].title_path || chunks[0].heading,
                    level: null,
                    page: chunks[0].page ?? null,
                    language: null,
                    markdown: '',
                    html: '',
                  }"
                />
              </div>
              <div v-else class="rounded-[18px] bg-white/72 px-4 py-6 text-sm text-dim">
                {{ t("page.chunks.empty.selectDocToView") }}
              </div>
            </SeekMindDetailSection>

            <SeekMindDetailSection :title="t('common.context')">
              <div v-if="loadingChunks" class="text-sm text-dim">{{ t("page.chunks.readingChunks") }}</div>
              <div v-else-if="chunks.length === 0" class="rounded-[18px] bg-white/72 px-4 py-6 text-sm text-dim">
                {{ t("page.chunks.empty.chunks") }}
              </div>
              <div v-else class="space-y-3">
                <div v-for="chunk in chunks" :key="chunk.id" class="rounded-[16px] bg-white/72 p-3">
                  <div class="mb-2 flex items-center justify-between gap-2">
                    <div class="min-w-0 flex-1">
                      <div class="text-sm font-medium text-primary">{{ chunk.title_path || chunk.heading }}</div>
                      <div class="mt-1 text-[11px] text-dim">
                        {{ t("page.chunks.titlePath") }}：{{ chunk.title_path || chunk.heading }}
                      </div>
                    </div>
                    <div class="flex shrink-0 items-center gap-2">
                      <SeekMindBadge tone="default">
                        {{ chunk.page ? t("page.chunks.page", { page: chunk.page }) : t("page.chunks.paragraph", { para: chunk.paragraph ?? 0 }) }}
                      </SeekMindBadge>
                      <button
                        class="rounded-full bg-white/72 px-2 py-1 text-[11px] text-secondary hover:text-primary"
                        @click="copyChunkCitation(chunk)"
                      >
                        {{ t("page.chunks.action.copyCitation") }}
                      </button>
                    </div>
                  </div>
                  <div v-if="chunk.preview_blocks && chunk.preview_blocks.length > 0" class="space-y-1">
                    <SeekMindPreviewBlockRenderer
                      v-for="block in chunk.preview_blocks"
                      :key="block.block_index"
                      :block="block"
                    />
                  </div>
                  <SeekMindMarkdownRenderer
                    v-else
                    :block="{
                      block_index: 0,
                      block_type: 'paragraph',
                      text: chunk.snippet,
                      heading: chunk.title_path || chunk.heading,
                      level: null,
                      page: chunk.page ?? null,
                      language: null,
                      markdown: '',
                      html: '',
                    }"
                  />
                </div>
              </div>
            </SeekMindDetailSection>

            <div v-if="actionMessage" class="rounded-md border border-emerald-soft bg-emerald-soft px-3 py-2 text-xs text-success">
              {{ actionMessage }}
            </div>
            <div v-if="actionErrorMessage" class="rounded-md border border-danger-soft bg-danger-soft px-3 py-2 text-xs text-danger">
              {{ actionErrorMessage }}
            </div>
          </SeekMindDetailPanel>
          <div v-else class="flex min-h-0 flex-1 items-center justify-center px-4 text-sm text-dim">
            {{ t("page.chunks.empty.selectDocToView") }}
          </div>
        </section>
        </template>
      </SplitPane>
    </main>

    <SeekMindContextMenu
      v-if="contextMenuVisible"
      :items="chunkContextMenuItems"
      :x="contextMenuPosition.x"
      :y="contextMenuPosition.y"
      @close="contextMenuVisible = false"
    />
  </div>
</template>

<style scoped>
.seekmind-chunks-warning {
  display: -webkit-box;
  overflow: hidden;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
  word-break: break-word;
}
</style>
