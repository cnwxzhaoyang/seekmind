/**
 * @author MorningSun
 * @CreatedDate 2026/06/03
 * @Description 搜索面板，展示搜索结果、命中详情与快捷问答。
 */
<script setup lang="ts">
defineOptions({
  name: "AppSearchPage",
});

import { computed, onActivated, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { listen } from "@tauri-apps/api/event";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";
import { AlertCircle, BookMarked, ClipboardCopy, Clock, Eye, FileText, Files, Filter, FolderPlus, Link2, MessageSquareText, Search, SquareArrowOutUpRight, X } from "lucide-vue-next";
import SeekMindBadge from "../components/SeekMind/SeekMindBadge.vue";
import SeekMindDetailPanel from "../components/SeekMind/SeekMindDetailPanel.vue";
import SeekMindDetailSection from "../components/SeekMind/SeekMindDetailSection.vue";
import SeekMindCollectionPicker from "../components/SeekMind/SeekMindCollectionPicker.vue";
import SeekMindContextMenu from "../components/SeekMind/SeekMindContextMenu.vue";
import type { ContextMenuItem } from "../components/SeekMind/SeekMindContextMenu.vue";
import SeekMindHighlightedText from "../components/SeekMind/SeekMindHighlightedText.vue";
import SeekMindMarkdownRenderer from "../components/SeekMind/SeekMindMarkdownRenderer.vue";
import SeekMindPreviewBlockRenderer from "../components/SeekMind/SeekMindPreviewBlockRenderer.vue";
import SeekMindSearchResultGroupCard from "../components/SeekMind/SeekMindSearchResultGroupCard.vue";
import SeekMindToast from "../components/SeekMind/SeekMindToast.vue";
import SplitPane from "../components/SplitPane.vue";
import { useQuickAccessData } from "../composables/useQuickAccessData";
import { seekMindApi, formatSeekMindError } from "../services/seekMindApi";
import { listenQaConfigUpdated } from "../utils/qaConfigEvents";
import { emitQuickAccessUpdated } from "../utils/quickAccessEvents";
import { buildDocumentLocationParts, formatDocumentCitation, resolveDocumentTitlePath } from "../utils/citation";

const { t } = useI18n();
import type {
  ChunkView,
  ParserRuntimeView,
  CollectionView,
  CollectionItemInput,
  QaAnswerView,
  QaAnswerProgressView,
  QaAskStartView,
  QaSettingsView,
  SearchDebugReportEventView,
  SearchDebugView,
  SearchResultView,
  PreviewBlockView,
} from "../types/SeekMind";

const route = useRoute();
const router = useRouter();
const routeSearchQuery = computed(() => (typeof route.query.q === "string" ? route.query.q : ""));
const query = ref(routeSearchQuery.value);
const qaQuestion = ref("");
const qaAnswer = ref<QaAnswerView | null>(null);
const qaMessages = ref<QaAnswerView[]>([]);
const qaSessionId = ref("");
const qaSessionTitle = ref("");
const qaSettings = ref<QaSettingsView | null>(null);
const qaSelectedSourceId = ref("");
const qaSelectedSourceClosed = ref(false);
const qaLoading = ref(false);
const qaCancelling = ref(false);
const qaErrorMessage = ref("");
const qaInfoMessage = ref("");
const qaMode = ref(false);
const qaActiveJobId = ref("");
const searchInputRef = ref<HTMLInputElement | null>(null);
const selectedId = ref<string>("");
const results = ref<SearchResultView[]>([]);
const debugReport = ref<SearchDebugView | null>(null);
const showDebugPanel = ref(false);
const debugReportLoading = ref(false);
const debugReportError = ref("");
const activeDebugRequestId = ref("");
const parserRuntime = ref<ParserRuntimeView | null>(null);
const selectedChunkCount = ref<number | null>(null);
const selectedDocumentChunks = ref<ChunkView[]>([]);
const selectedChunkIndex = ref<number>(-1);
const actionMessage = ref("");
const actionErrorMessage = ref("");
const loading = ref(false);
const errorMessage = ref("");
const expandedGroups = ref<Record<string, boolean>>({});
const showResultFilterPanel = ref(false);
const resultFilterMode = ref<"all" | "favorites">("all");
const selectedResultClosed = ref(false);
const routeSyncReady = ref(false);
let selectedContextRequestId = 0;
let unlistenSearchDebugReport: null | (() => void) = null;
let unlistenQaProgress: null | (() => void) = null;
let unlistenQaConfigUpdated: null | (() => void) = null;
const { favorites, loadQuickAccessData } = useQuickAccessData();
const collections = ref<CollectionView[]>([]);
const collectionPickerVisible = ref(false);
const collectionPickerTarget = ref<SearchResultView | null>(null);
const collectionPickerMode = ref<"chunk" | "document">("chunk");
const collectionPickerLoading = ref(false);

interface SearchResultGroup {
  path: string;
  file_name: string;
  ext: string;
  topResult: SearchResultView;
  results: SearchResultView[];
  count: number;
  totalScore: number;
}

const selected = computed(
  () => results.value.find((item) => item.id === selectedId.value) ?? null,
);

const qaStateLabel = computed(() => {
  if (!qaAnswer.value) {
    return t("page.appSearch.qa.idle");
  }

  return t(`page.appSearch.qa.state.${qaAnswer.value.state}`);
});

const qaStateTone = computed(() => {
  if (!qaAnswer.value) {
    return "default";
  }

  if (qaAnswer.value.state === "answered") {
    return "success";
  }

  if (qaAnswer.value.state === "insufficient_evidence") {
    return "warning";
  }

  if (qaAnswer.value.state === "cancelled") {
    return "danger";
  }

  return "default";
});

const isQaConfigured = (settings: QaSettingsView | null) =>
  Boolean(settings?.enabled && settings.base_url.trim() && settings.model.trim());

const qaSelectedSource = computed(
  () => {
    if (qaSelectedSourceClosed.value) {
      return null;
    }

    return qaAnswer.value?.sources.find((item) => item.source_id === qaSelectedSourceId.value) ?? qaAnswer.value?.sources[0] ?? null;
  },
);

const selectedTitlePath = computed(() =>
  resolveDocumentTitlePath({
    fileName: selected.value?.file_name,
    titlePath: selected.value?.title_path,
    heading: selected.value?.heading,
  }),
);

const qaSelectedTitlePath = computed(() =>
  resolveDocumentTitlePath({
    fileName: qaSelectedSource.value?.file_name,
    titlePath: qaSelectedSource.value?.title_path,
    heading: qaSelectedSource.value?.heading,
  }),
);

const selectedCitation = computed(() => {
  if (!selected.value) {
    return "";
  }

  return formatDocumentCitation({
    fileName: selected.value.file_name,
    titlePath: selected.value.title_path,
    heading: selected.value.heading,
    locationParts: buildDocumentLocationParts({
      page: selected.value.page,
      paragraph: selected.value.paragraph,
      pageLabel: t("page.appSearch.detail.pdfPage", { page: selected.value.page ?? 0 }),
      paragraphLabel: t("searchResultCard.paragraph", { para: selected.value.paragraph ?? 0 }),
    }),
  });
});

const qaSelectedCitation = computed(() => {
  if (!qaSelectedSource.value) {
    return "";
  }

  return formatDocumentCitation({
    fileName: qaSelectedSource.value.file_name,
    titlePath: qaSelectedTitlePath.value,
    locationParts: buildDocumentLocationParts({
      page: qaSelectedSource.value.page,
      paragraph: qaSelectedSource.value.paragraph,
      pageLabel: t("page.appSearch.detail.pdfPage", { page: qaSelectedSource.value.page ?? 0 }),
      paragraphLabel: t("searchResultCard.paragraph", { para: qaSelectedSource.value.paragraph ?? 0 }),
    }),
  });
});

const selectedChunk = computed(() => {
  if (selectedChunkIndex.value < 0) {
    return null;
  }

  return selectedDocumentChunks.value[selectedChunkIndex.value] ?? null;
});

const isImageExtension = (ext?: string | null) => {
  const normalized = ext?.trim().toLowerCase().replace(/^\./, "") ?? "";
  return Boolean(normalized && ["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff", "heic"].includes(normalized));
};

const selectedImagePreviewBlock = computed<PreviewBlockView | null>(() => {
  if (!selected.value || !isImageExtension(selected.value.ext)) {
    return null;
  }

  // 修复：图片文件的搜索命中多为 OCR 文本，详情里额外补充原图，避免用户只能看到识别文本。
  return {
    block_index: -1,
    block_type: "image",
    text: selected.value.file_name,
    heading: selected.value.file_name,
    level: null,
    page: null,
    language: null,
    markdown: "",
    html: "",
    asset_path: selected.value.path,
    alt_text: selected.value.file_name,
    caption: selected.value.path,
    ocr_text: "",
  };
});

const selectedPreviewBlocks = computed(() => {
  const chunkBlocks = selectedChunk.value?.preview_blocks ?? [];
  const resultBlocks = selected.value?.preview_blocks ?? [];
  if (selectedImagePreviewBlock.value) {
    return [selectedImagePreviewBlock.value, ...(chunkBlocks.length > 0 ? chunkBlocks : resultBlocks)];
  }
  if (chunkBlocks.length > 0) {
    return chunkBlocks;
  }
  return resultBlocks;
});

const qaSelectedPreviewBlocks = computed(() => qaSelectedSource.value?.preview_blocks ?? []);

const selectedChunkPositionLabel = computed(() => {
  if (!selectedDocumentChunks.value.length || selectedChunkIndex.value < 0) {
    return "—";
  }

  return t("page.appSearch.detail.chunkPosition", {
    current: selectedChunkIndex.value + 1,
    total: selectedDocumentChunks.value.length,
  });
});

const selectedContextChunks = computed(() => {
  if (!selectedDocumentChunks.value.length || selectedChunkIndex.value < 0) {
    return [];
  }

  return [
    {
      key: "previous",
      label: t("page.appSearch.detail.previousChunk"),
      chunk: selectedDocumentChunks.value[selectedChunkIndex.value - 1] ?? null,
    },
    {
      key: "current",
      label: t("page.appSearch.detail.currentChunk"),
      chunk: selectedDocumentChunks.value[selectedChunkIndex.value] ?? null,
    },
    {
      key: "next",
      label: t("page.appSearch.detail.nextChunk"),
      chunk: selectedDocumentChunks.value[selectedChunkIndex.value + 1] ?? null,
    },
  ].filter((item) => item.chunk !== null);
});

const filteredResults = computed(() => {
  if (resultFilterMode.value === "favorites") {
    return results.value.filter((item) => isResultFavorited(item.path, item.heading, item.paragraph, item.page));
  }
  return results.value;
});

const groupedResults = computed<SearchResultGroup[]>(() => {
  const groups = new Map<string, SearchResultView[]>();

  for (const item of filteredResults.value) {
    const items = groups.get(item.path) ?? [];
    items.push(item);
    groups.set(item.path, items);
  }

  return [...groups.entries()]
    .map(([path, items]) => {
      const sorted = [...items].sort((a, b) => b.score - a.score);
      return {
        path,
        file_name: sorted[0]?.file_name ?? path,
        ext: sorted[0]?.ext ?? "",
        topResult: sorted[0],
        results: sorted,
        count: sorted.length,
        totalScore: sorted.reduce((sum, item) => sum + item.score, 0),
      };
    })
    .sort((a, b) => b.topResult.score - a.topResult.score);
});

const splitPanels = computed(() => {
  const items: { key: string; initialSize?: number; minSize: number; flex?: boolean }[] = [
    { key: "center", minSize: 300, flex: true },
  ];
  if (qaMode.value ? Boolean(qaSelectedSource.value) : Boolean(selected.value)) {
    items.push({ key: "right", initialSize: 320, minSize: 240 });
  }
  return items;
});

const matchedFieldLabel = computed(() => selected.value?.match_origin || "");

const favoriteTargetSet = computed(() => {
  const set = new Set<string>();
  for (const favorite of favorites.value) {
    if (favorite.favorite_type === "result") {
      set.add(favorite.target);
    }
  }
  return set;
});

const favoriteResults = computed(() => favorites.value.filter((favorite) => favorite.favorite_type === "result"));

const focusSearchInput = () => {
  searchInputRef.value?.focus();
  searchInputRef.value?.select?.();
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
    setActionError(t("page.appSearch.detail.copyFailed"));
  }
};

const handleGlobalShortcut = (event: KeyboardEvent) => {
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
    event.preventDefault();
    focusSearchInput();
  }
};

const isSearchClickDebugEnabled = () => globalThis.localStorage?.getItem("SEEKMIND_DEBUG_SEARCH_CLICK") === "1";

const selectResult = (id: string) => {
  if (isSearchClickDebugEnabled()) {
    const item = results.value.find((result) => result.id === id);
    console.debug("[AppSearch] selectResult received", {
      id,
      found: Boolean(item),
      previousSelectedId: selectedId.value,
      path: item?.path ?? null,
      results: results.value.length,
    });
  }

  selectedResultClosed.value = false;
  selectedId.value = id;
};

const toggleGroup = (path: string) => {
  expandedGroups.value = {
    ...expandedGroups.value,
    [path]: !expandedGroups.value[path],
  };
};

const toggleResultFilterPanel = () => {
  showResultFilterPanel.value = !showResultFilterPanel.value;
};

const setResultFilterMode = (mode: "all" | "favorites") => {
  resultFilterMode.value = mode;
  showResultFilterPanel.value = false;
};

const loadParserRuntime = async () => {
  parserRuntime.value = await seekMindApi.getParserRuntime();
};

const loadQaSettings = async () => {
  try {
    qaSettings.value = await seekMindApi.getQaSettings();
  } catch (error) {
    console.error("[SeekMind] getQaSettings failed", error);
  }
};

const refreshQaConfig = async () => {
  // 修复：搜索页问答模式同样处于 KeepAlive 缓存中，返回页面时要主动同步最新问答配置。
  await loadQaSettings();
  console.info("[SeekMind] search QA config refreshed");
};

const loadLatestQaSession = async () => {
  try {
    const sessions = await seekMindApi.listQaSessions(1);
    const latest = sessions[0];
    if (!latest) {
      return;
    }

    qaSessionId.value = latest.id;
    qaSessionTitle.value = latest.title;
    const messages = await seekMindApi.listQaMessages(latest.id, 50);
    qaMessages.value = messages;
    qaAnswer.value = messages[messages.length - 1] ?? null;
    qaSelectedSourceClosed.value = false;
    qaSelectedSourceId.value = qaAnswer.value?.sources[0]?.source_id ?? "";
  } catch (error) {
    console.error("[SeekMind] loadLatestQaSession failed", error);
  }
};

const ensureQaSession = async (title: string) => {
  if (qaSessionId.value) {
    return qaSessionId.value;
  }

  const session = await seekMindApi.createQaSession(title);
  qaSessionId.value = session.id;
  qaSessionTitle.value = session.title;
  return session.id;
};

const installQaProgressListener = async () => {
  if (unlistenQaProgress) {
    return;
  }

  unlistenQaProgress = await listen<QaAnswerProgressView>(
    "seekmind:qa:answer-progress",
    (event) => {
      const payload = event.payload;
      if (payload.job_id !== qaActiveJobId.value) {
        return;
      }

      qaAnswer.value = {
        id: payload.job_id,
        question: payload.question,
        answer: payload.answer,
        state: payload.state,
        sources: payload.sources,
        retrieval: payload.retrieval,
        model: payload.model,
        created_at: payload.updated_at,
        error: payload.error ?? null,
      };
      const messageIndex = qaMessages.value.findIndex((item) => item.id === payload.job_id);
      if (messageIndex >= 0 && qaAnswer.value) {
        qaMessages.value.splice(messageIndex, 1, qaAnswer.value);
      } else if (qaAnswer.value) {
        qaMessages.value.push(qaAnswer.value);
      }
      if (!qaSelectedSourceClosed.value) {
        qaSelectedSourceId.value = qaSelectedSourceId.value || payload.sources[0]?.source_id || "";
      }

      if (payload.state === "searching") {
        qaLoading.value = true;
        qaInfoMessage.value = t("page.appSearch.qa.searching");
        qaErrorMessage.value = "";
        return;
      }

      if (payload.state === "generating" || payload.state === "streaming") {
        qaLoading.value = true;
        qaInfoMessage.value = payload.state === "generating"
          ? t("page.appSearch.qa.generating")
          : t("page.appSearch.qa.streaming");
        qaErrorMessage.value = "";
        return;
      }

      qaLoading.value = false;
      if (payload.state === "answered") {
        qaInfoMessage.value = t("page.appSearch.qa.answered");
        qaErrorMessage.value = "";
        return;
      }

      if (payload.state === "insufficient_evidence") {
        qaInfoMessage.value = t("page.appSearch.qa.insufficient");
        qaErrorMessage.value = "";
        return;
      }

      if (payload.state === "cancelled") {
        qaLoading.value = false;
        qaCancelling.value = false;
        qaInfoMessage.value = t("page.appSearch.qa.stopped");
        qaErrorMessage.value = "";
        qaActiveJobId.value = "";
        return;
      }

      qaInfoMessage.value = "";
      qaErrorMessage.value = payload.error || t("page.appSearch.qa.askFailed");
    },
  );
};

const runQa = async () => {
  if (!qaQuestion.value.trim()) {
    qaAnswer.value = null;
    qaMessages.value = [];
    qaSessionId.value = "";
    qaSessionTitle.value = "";
    qaSelectedSourceId.value = "";
    qaErrorMessage.value = "";
    qaInfoMessage.value = "";
    return;
  }

  if (qaLoading.value || qaCancelling.value) {
    return;
  }

  qaLoading.value = true;
  qaErrorMessage.value = "";
  qaInfoMessage.value = "";

  try {
    await refreshQaConfig();
    if (!isQaConfigured(qaSettings.value)) {
      qaAnswer.value = null;
      qaSelectedSourceId.value = "";
      qaInfoMessage.value = t("page.appSearch.qa.notConfigured");
      qaLoading.value = false;
      return;
    }

    const questionText = qaQuestion.value.trim();
    const sessionId = await ensureQaSession(questionText);
    const started: QaAskStartView = await seekMindApi.askQuestion(questionText, [], 6, sessionId);
    qaActiveJobId.value = started.job_id;
    qaAnswer.value = started.status;
    qaMessages.value.push(started.status);
    qaSelectedSourceClosed.value = false;
    qaSelectedSourceId.value = started.status.sources[0]?.source_id ?? "";
    qaQuestion.value = "";

    if (started.status.state === "model_not_configured") {
      qaInfoMessage.value = t("page.appSearch.qa.notConfigured");
      qaLoading.value = false;
      qaErrorMessage.value = started.status.error || "";
    } else if (started.status.state === "insufficient_evidence") {
      qaInfoMessage.value = t("page.appSearch.qa.insufficient");
      qaLoading.value = false;
    } else {
      qaInfoMessage.value = t("page.appSearch.qa.searching");
    }
  } catch (error) {
    qaAnswer.value = null;
    qaSelectedSourceClosed.value = false;
    qaSelectedSourceId.value = "";
    qaErrorMessage.value = formatSeekMindError(error, t("page.appSearch.qa.askFailed"));
    qaLoading.value = false;
    qaActiveJobId.value = "";
  } finally {
    qaCancelling.value = false;
    if (qaAnswer.value?.state === "model_not_configured" || qaAnswer.value?.state === "insufficient_evidence") {
      qaActiveJobId.value = "";
    }
  }
};

const newQaSession = () => {
  if (qaLoading.value || qaCancelling.value) {
    return;
  }

  qaMessages.value = [];
  qaAnswer.value = null;
  qaSessionId.value = "";
  qaSessionTitle.value = "";
  qaQuestion.value = "";
  qaSelectedSourceClosed.value = false;
  qaSelectedSourceId.value = "";
  qaInfoMessage.value = "";
  qaErrorMessage.value = "";
  qaActiveJobId.value = "";
};

const selectQaMessage = (message: QaAnswerView) => {
  qaAnswer.value = message;
  qaSelectedSourceClosed.value = false;
  qaSelectedSourceId.value = message.sources[0]?.source_id ?? "";
};

const stopQa = async () => {
  if (!qaActiveJobId.value || qaCancelling.value) {
    return;
  }

  const jobId = qaActiveJobId.value;
  qaCancelling.value = true;
  qaLoading.value = false;
  qaInfoMessage.value = t("page.appSearch.qa.stopping");
  qaErrorMessage.value = "";

  try {
    await seekMindApi.cancelQaQuestion(jobId);
    if (qaAnswer.value && qaAnswer.value.id === jobId) {
      qaAnswer.value = {
        ...qaAnswer.value,
        state: "cancelled",
        error: null,
      };
    }
    qaInfoMessage.value = t("page.appSearch.qa.stopped");
  } catch (error) {
    qaErrorMessage.value = error instanceof Error ? error.message : t("page.appSearch.qa.askFailed");
  } finally {
    qaActiveJobId.value = "";
    qaCancelling.value = false;
  }
};

const selectQaSource = (sourceId: string) => {
  qaSelectedSourceClosed.value = false;
  qaSelectedSourceId.value = sourceId;
};

const closeSelectedQaSource = () => {
  qaSelectedSourceClosed.value = true;
  qaSelectedSourceId.value = "";
};

const closeSelectedResult = () => {
  selectedResultClosed.value = true;
  selectedId.value = "";
};

const openSelectedQaFile = async () => {
  if (!qaSelectedSource.value) return;
  await seekMindApi.openFile(qaSelectedSource.value.path);
  await loadQuickAccessData();
};

const quickLookSelectedQaFile = async () => {
  if (!qaSelectedSource.value) return;

  try {
    await seekMindApi.quickLookFile(qaSelectedSource.value.path);
    setActionMessage(t("page.appSearch.detail.quickLookOpened"));
  } catch (error) {
    setActionError(error instanceof Error ? error.message : t("page.appSearch.detail.quickLookFailed"));
  }
};

const copySelectedQaPath = async () => {
  if (!qaSelectedSource.value) return;
  await copyText(qaSelectedSource.value.path, t("page.appSearch.detail.copiedPath"));
};

const copySelectedQaCitation = async () => {
  if (!qaSelectedSource.value) return;
  await copyText(qaSelectedCitation.value, t("page.appSearch.detail.copiedCitation"));
};

const viewQaChunks = async () => {
  if (!qaSelectedSource.value) return;
  await router.push({ path: "/chunks", query: { path: qaSelectedSource.value.path } });
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

const createSearchDebugRequestId = () => {
  if (globalThis.crypto?.randomUUID) {
    return globalThis.crypto.randomUUID();
  }

  return `search-debug-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
};

const clearSearchDebugReport = () => {
  activeDebugRequestId.value = "";
  debugReport.value = null;
  debugReportError.value = "";
  debugReportLoading.value = false;
};

const installSearchDebugReportListener = async () => {
  if (unlistenSearchDebugReport) {
    return;
  }

  unlistenSearchDebugReport = await listen<SearchDebugReportEventView>(
    "seekmind:search-debug-report",
    (event) => {
      const payload = event.payload;
      if (payload.request_id !== activeDebugRequestId.value) {
        return;
      }

      if (payload.state === "running") {
        debugReportLoading.value = true;
        debugReportError.value = "";
        return;
      }

      debugReportLoading.value = false;
      if (payload.state === "completed") {
        debugReport.value = payload.report;
        debugReportError.value = "";
        return;
      }

      debugReport.value = null;
      debugReportError.value = payload.error || t("page.appSearch.searchFailed");
    },
  );
};

const requestSearchDebugReport = async () => {
  if (!showDebugPanel.value || !query.value.trim()) {
    clearSearchDebugReport();
    return;
  }

  const requestId = createSearchDebugRequestId();
  activeDebugRequestId.value = requestId;
  debugReportLoading.value = true;
  debugReportError.value = "";

  try {
    await seekMindApi.requestSearchDebugReport(requestId, query.value, 20);
  } catch (error) {
    if (activeDebugRequestId.value !== requestId) {
      return;
    }

    debugReportLoading.value = false;
    debugReport.value = null;
    debugReportError.value = error instanceof Error ? error.message : t("page.appSearch.searchFailed");
  }
};

const normalizeMatchText = (value: string) => value.replace(/\s+/g, "").trim().toLowerCase();

const resolveSelectedChunkIndex = (chunks: ChunkView[], current: SearchResultView) => {
  const resultParagraph = current.paragraph ?? null;
  const resultPage = current.page ?? null;
  const resultHeading = normalizeMatchText(current.title_path ?? current.heading ?? "");
  const resultSnippet = normalizeMatchText(current.snippet ?? "");

  const exactIndex = chunks.findIndex((chunk) => {
    if (resultParagraph !== null && chunk.paragraph !== null && chunk.paragraph === resultParagraph) {
      return true;
    }
    if (resultPage !== null && chunk.page !== null && chunk.page === resultPage) {
      return true;
    }

    const chunkHeading = normalizeMatchText(chunk.title_path ?? chunk.heading ?? "");
    const chunkSnippet = normalizeMatchText(chunk.snippet ?? "");

    if (resultHeading && chunkHeading && resultHeading === chunkHeading) {
      return true;
    }
    if (resultSnippet && chunkSnippet && resultSnippet === chunkSnippet) {
      return true;
    }
    if (resultSnippet && chunkSnippet && (chunkSnippet.includes(resultSnippet) || resultSnippet.includes(chunkSnippet))) {
      return true;
    }

    return false;
  });

  return exactIndex;
};

const loadSelectedContext = async (current: SearchResultView | null | undefined) => {
  const requestId = ++selectedContextRequestId;

  if (!current) {
    if (isSearchClickDebugEnabled()) {
      console.debug("[AppSearch] loadSelectedContext skipped", {
        reason: "empty selection",
        requestId,
      });
    }

    selectedChunkCount.value = null;
    selectedDocumentChunks.value = [];
    selectedChunkIndex.value = -1;
    return;
  }

  try {
    const chunks = await seekMindApi.listDocumentChunks(current.path);
    if (requestId !== selectedContextRequestId) {
      if (isSearchClickDebugEnabled()) {
        console.debug("[AppSearch] loadSelectedContext ignored stale response", {
          requestId,
          activeRequestId: selectedContextRequestId,
          path: current.path,
        });
      }

      return;
    }

    selectedDocumentChunks.value = chunks;
    selectedChunkCount.value = chunks.length;
    selectedChunkIndex.value = resolveSelectedChunkIndex(chunks, current);

    if (isSearchClickDebugEnabled()) {
      console.debug("[AppSearch] loadSelectedContext resolved", {
        requestId,
        id: current.id,
        path: current.path,
        chunks: chunks.length,
        selectedChunkIndex: selectedChunkIndex.value,
      });
    }
  } catch (error) {
    if (requestId !== selectedContextRequestId) {
      if (isSearchClickDebugEnabled()) {
        console.debug("[AppSearch] loadSelectedContext ignored stale error", {
          requestId,
          activeRequestId: selectedContextRequestId,
          path: current.path,
        });
      }

      return;
    }

    selectedChunkCount.value = null;
    selectedDocumentChunks.value = [];
    selectedChunkIndex.value = -1;
    console.error("[SeekMind] loadSelectedContext failed", error);
  }
};

const runSearch = async () => {
  if (!query.value.trim()) {
    results.value = [];
    selectedResultClosed.value = false;
    selectedId.value = "";
    expandedGroups.value = {};
    clearSearchDebugReport();
    selectedChunkCount.value = null;
    selectedDocumentChunks.value = [];
    selectedChunkIndex.value = -1;
    return;
  }

  if (loading.value) {
    return;
  }

  loading.value = true;
  errorMessage.value = "";

  try {
    results.value = await seekMindApi.searchDocuments(query.value, 20);
    selectedResultClosed.value = false;
    selectedId.value = "";
    expandedGroups.value = {};
    if (showDebugPanel.value) {
      await requestSearchDebugReport();
    } else {
      clearSearchDebugReport();
    }
    await loadQuickAccessData();
    emitQuickAccessUpdated("search");
  } catch (error) {
    results.value = [];
    selectedResultClosed.value = false;
    selectedId.value = "";
    expandedGroups.value = {};
    errorMessage.value = error instanceof Error ? error.message : t("page.appSearch.searchFailed");
    if (showDebugPanel.value) {
      await requestSearchDebugReport();
    } else {
      clearSearchDebugReport();
    }
    await loadQuickAccessData();
    emitQuickAccessUpdated("search-error");
  } finally {
    loading.value = false;
  }
};

const openSelectedFile = async () => {
  if (!selected.value) return;
  await seekMindApi.openFile(selected.value.path);
  await loadQuickAccessData();
};

const quickLookSelectedFile = async () => {
  if (!selected.value) return;

  try {
    await seekMindApi.quickLookFile(selected.value.path);
    setActionMessage(t("page.appSearch.detail.quickLookOpened"));
  } catch (error) {
    setActionError(error instanceof Error ? error.message : t("page.appSearch.detail.quickLookFailed"));
  }
};

const copySelectedPath = async () => {
  if (!selected.value) return;
  await copyText(selected.value.path, t("page.appSearch.detail.copiedPath"));
};

const copySelectedTitlePath = async () => {
  if (!selected.value) return;
  await copyText(selectedTitlePath.value || selected.value.heading, t("page.appSearch.detail.copiedTitlePath"));
};

const copySelectedCitation = async () => {
  if (!selected.value) return;
  await copyText(selectedCitation.value, t("page.appSearch.detail.copiedCitation"));
};

const viewChunks = async () => {
  if (!selected.value) return;
  await router.push({ path: "/chunks", query: { path: selected.value.path } });
};

const toggleFavoriteResult = async (item: SearchResultView) => {
  await seekMindApi.toggleResultFavorite(
    item.path,
    item.heading,
    item.paragraph ?? null,
    item.page ?? null,
    item.file_name,
  );
  await loadQuickAccessData();
};

const loadCollections = async () => {
  collectionPickerLoading.value = true;
  try {
    collections.value = await seekMindApi.listCollections();
  } catch (error) {
    console.error("[SeekMind] listCollections failed", error);
  } finally {
    collectionPickerLoading.value = false;
  }
};

const openResultCollectionPicker = async (item: SearchResultView, mode: "chunk" | "document" = "chunk") => {
  collectionPickerTarget.value = item;
  collectionPickerMode.value = mode;
  actionErrorMessage.value = "";
  actionMessage.value = t("page.collections.pickerOpening");
  collectionPickerVisible.value = true;
  if (collections.value.length === 0) {
    await loadCollections();
  }
};

const addSelectedResultToCollection = async (collectionId: string) => {
  if (!collectionPickerTarget.value) {
    return;
  }

  const item = collectionPickerTarget.value;
  const isDocumentCollection = collectionPickerMode.value === "document";
  const itemTitle = item.file_name
    || item.path
    || item.heading
    || t("common.none");
  const collection = collections.value.find((entry) => entry.id === collectionId);
  const input: CollectionItemInput = {
    collection_id: collectionId,
    item_type: collectionPickerMode.value,
    document_id: isDocumentCollection ? null : "",
    chunk_id: isDocumentCollection ? null : item.id,
    title: itemTitle,
    path: item.path,
    title_path: isDocumentCollection ? item.file_name : (item.title_path || item.heading),
    snippet: item.snippet,
    note: "",
    source_meta_json: JSON.stringify({
      mode: collectionPickerMode.value,
      file_name: item.file_name,
      ext: item.ext,
      paragraph: item.paragraph ?? null,
      page: item.page ?? null,
      score: item.score,
      rank_reason: item.rank_reason.summary,
    }),
  };

  await seekMindApi.addCollectionItem(input);
  actionMessage.value = t("page.collections.itemAddedToCollection", { name: collection?.name ?? t("common.none") });
  collectionPickerVisible.value = false;
  await loadCollections();
};

const createCollectionAndAddResult = async (name: string) => {
  const created = await seekMindApi.createCollection(name, "");
  collections.value = [created, ...collections.value.filter((item) => item.id !== created.id)];
  await addSelectedResultToCollection(created.id);
};

const handleCollectionPickerSelect = async (collectionId: string) => {
  if (!collectionPickerTarget.value) {
    return;
  }
  await addSelectedResultToCollection(collectionId);
};

const handleCollectionPickerCreate = async (name: string) => {
  if (!collectionPickerTarget.value) {
    return;
  }
  await createCollectionAndAddResult(name);
};

const isResultFavorited = (
  path: string,
  heading: string,
  paragraph?: number | null,
  page?: number | null,
) => {
  const key = `result|${path}|${heading.trim()}|${paragraph ?? ""}|${page ?? ""}`;
  return favoriteTargetSet.value.has(key);
};

const resultMenuPosition = ref({ x: 0, y: 0 });
const resultMenuVisible = ref(false);

const resultContextMenuItems = computed<ContextMenuItem[]>(() => {
  if (!selected.value) {
    return [];
  }

  return [
    {
      key: "addCollection",
      label: t("page.collections.addToCollection"),
      icon: FolderPlus,
      handler: () => {
        if (selected.value) {
          void openResultCollectionPicker(selected.value);
        }
      },
    },
    {
      key: "collectDocument",
      label: t("page.collections.collectDocument"),
      icon: BookMarked,
      handler: () => {
        if (selected.value) {
          void openResultCollectionPicker(selected.value, "document");
        }
      },
    },
    { key: "divider-collection", label: "", divider: true },
    {
      key: "openFile",
      label: t("page.appSearch.detail.openFile"),
      icon: SquareArrowOutUpRight,
      handler: () => openSelectedFile(),
    },
    {
      key: "viewChunks",
      label: t("page.appSearch.detail.viewChunks"),
      icon: Files,
      handler: () => viewChunks(),
    },
    {
      key: "quickLook",
      label: t("page.appSearch.detail.quickLook"),
      icon: Eye,
      handler: () => quickLookSelectedFile(),
    },
    { key: "divider-copy", label: "", divider: true },
    {
      key: "copyPath",
      label: t("page.appSearch.detail.copyPath"),
      icon: ClipboardCopy,
      handler: () => copySelectedPath(),
    },
    {
      key: "copyTitlePath",
      label: t("page.appSearch.detail.copyTitlePath"),
      icon: Link2,
      handler: () => copySelectedTitlePath(),
    },
    {
      key: "copyCitation",
      label: t("page.appSearch.detail.copyCitation"),
      icon: FileText,
      handler: () => copySelectedCitation(),
    },
  ];
});

const handleResultContextMenu = (event: MouseEvent) => {
  if (!selected.value) {
    return;
  }

  resultMenuPosition.value = { x: event.clientX, y: event.clientY };
  resultMenuVisible.value = true;
};

onMounted(async () => {
  window.addEventListener("keydown", handleGlobalShortcut);
  unlistenQaConfigUpdated = listenQaConfigUpdated(() => {
    void refreshQaConfig();
  });
  await installSearchDebugReportListener();
  await Promise.all([loadParserRuntime(), loadQuickAccessData(), loadCollections()]);
  query.value = routeSearchQuery.value;
  if (query.value.trim()) {
    await runSearch();
  } else {
    results.value = [];
    selectedResultClosed.value = false;
    selectedId.value = "";
    expandedGroups.value = {};
    clearSearchDebugReport();
  }
  routeSyncReady.value = true;
});

onActivated(async () => {
  await refreshQaConfig();
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", handleGlobalShortcut);
  unlistenSearchDebugReport?.();
  unlistenSearchDebugReport = null;
  unlistenQaProgress?.();
  unlistenQaProgress = null;
  unlistenQaConfigUpdated?.();
  unlistenQaConfigUpdated = null;
});

watch(query, () => {
  if (!query.value.trim()) {
    results.value = [];
    selectedResultClosed.value = false;
    selectedId.value = "";
    expandedGroups.value = {};
    clearSearchDebugReport();
    selectedChunkCount.value = null;
    selectedDocumentChunks.value = [];
    selectedChunkIndex.value = -1;
  }
});

watch(routeSearchQuery, async (next) => {
  if (!routeSyncReady.value) {
    return;
  }

  if (next === query.value) {
    return;
  }

  query.value = next;
  if (query.value.trim()) {
    await runSearch();
    return;
  }

  results.value = [];
  selectedResultClosed.value = false;
  selectedId.value = "";
  expandedGroups.value = {};
  clearSearchDebugReport();
  selectedChunkCount.value = null;
  selectedDocumentChunks.value = [];
  selectedChunkIndex.value = -1;
});

watch(
  () => selected.value,
  async (current) => {
    if (isSearchClickDebugEnabled()) {
      console.debug("[AppSearch] selected watcher", {
        selectedId: selectedId.value,
        hasCurrent: Boolean(current),
        path: current?.path ?? null,
      });
    }

    await loadSelectedContext(current);
  },
  { immediate: true },
);

watch(
  filteredResults,
  (items) => {
  if (!items.length) {
    if (selectedId.value) {
      selectedId.value = "";
    }
    return;
  }

  if (selectedResultClosed.value) {
    return;
  }

  if (selected.value && items.some((item) => item.id === selected.value?.id)) {
    return;
  }

    selectedId.value = items[0].id;
  },
  { immediate: true },
);

watch(showDebugPanel, async (visible) => {
  if (!visible) {
    clearSearchDebugReport();
    return;
  }

  if (query.value.trim()) {
    await requestSearchDebugReport();
  }
});
</script>

<template>
  <div class="flex h-full min-h-0 flex-col bg-transparent text-primary">
    <main class="flex min-h-0 flex-1 overflow-hidden px-3 pb-3 pt-2">
      <SplitPane :panels="splitPanels">
        <template #center>
          <!-- 修复：搜索面板改为扁平化结构，减少卡片边框和分割线层级。 -->
          <section class="seekmind-pane-center flex min-h-0 flex-1 flex-col overflow-hidden">
            <div class="space-y-3 px-4 pt-2 pb-3">
              <form
                v-if="!qaMode"
                class="mx-auto flex h-10 w-full max-w-none items-center gap-2 rounded-full bg-[rgba(118,118,128,0.08)] px-4 transition focus-within:bg-white/70 focus-within:ring-2 focus-within:ring-accent/20"
                @submit.prevent="runSearch"
              >
                <Search :size="15" class="shrink-0 text-muted" />
                <input
                  ref="searchInputRef"
                  v-model="query"
                  :placeholder="t('page.appSearch.placeholder')"
                  class="min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-muted"
                />
                <span class="hidden rounded-full border border-default bg-white/70 px-2 py-0.5 text-[11px] text-muted sm:inline">Cmd+K</span>
              </form>

              <form v-else class="space-y-2" @submit.prevent="runQa">
                <textarea
                  v-model="qaQuestion"
                  rows="3"
                  class="w-full rounded-[18px] bg-[rgba(118,118,128,0.08)] px-4 py-2.5 text-sm text-primary outline-none transition focus:bg-white/70 focus:ring-2 focus:ring-accent/20"
                  :placeholder="t('page.appSearch.qa.placeholder')"
                />
                <div class="flex items-center justify-end gap-2">
                  <div v-if="qaSessionTitle" class="mr-auto text-xs text-dim">
                    {{ isQaConfigured(qaSettings) ? t("page.appSearch.qa.ready") : t("page.appSearch.qa.notConfigured") }}
                    <span class="ml-2 text-muted">· {{ qaSessionTitle }}</span>
                  </div>
                  <div class="flex items-center gap-2">
                    <button
                      v-if="qaMessages.length"
                      class="inline-flex items-center gap-2 rounded-full bg-white/80 px-3 py-1.5 text-sm font-medium text-secondary hover:bg-surface-hover disabled:cursor-not-allowed disabled:opacity-70"
                      type="button"
                      :disabled="qaLoading || qaCancelling"
                      @click="newQaSession"
                    >
                      {{ t("page.appSearch.qa.newSession") }}
                    </button>
                    <button
                      v-if="qaLoading || qaCancelling"
                      class="inline-flex items-center gap-2 rounded-full bg-white/80 px-3 py-1.5 text-sm font-medium text-secondary hover:bg-surface-hover"
                      type="button"
                      @click="stopQa"
                    >
                      {{ qaCancelling ? t("page.appSearch.qa.stopping") : t("page.appSearch.qa.stop") }}
                    </button>
                    <button
                      class="inline-flex items-center gap-2 rounded-full bg-accent px-3 py-1.5 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-70"
                      :disabled="qaLoading || qaCancelling"
                      type="submit"
                    >
                      <MessageSquareText :size="15" />
                      {{ qaCancelling ? t("page.appSearch.qa.stopping") : qaLoading ? t("page.appSearch.qa.asking") : t("page.appSearch.qa.ask") }}
                    </button>
                  </div>
                </div>
              </form>
            </div>

            <div
              v-if="officeNotice"
              class="border-b border-amber-soft bg-amber-soft px-4 py-3"
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

            <div v-if="!qaMode" class="flex items-center justify-between gap-3 px-4 py-1.5">
              <div class="text-xs font-medium text-dim">
                {{ t("page.appSearch.stats.foundDocs", { count: groupedResults.length, total: results.length }) }}
              </div>
              <button
                class="flex items-center gap-1 rounded-full px-2 py-1 text-xs text-accent-text hover:bg-accent-soft"
                :class="showResultFilterPanel ? 'bg-accent-soft' : ''"
                @click="toggleResultFilterPanel"
              >
                <Filter :size="14" />
                {{ t("page.appSearch.filter") }}
                <SeekMindBadge v-if="resultFilterMode !== 'all'" tone="success">
                  {{ t(`page.appSearch.filterMode.${resultFilterMode}`) }}
                </SeekMindBadge>
              </button>
            </div>
            <div v-if="!qaMode && showResultFilterPanel" class="px-4 pb-2">
              <div class="flex flex-wrap items-center gap-2">
                <button
                  class="rounded-full border px-2.5 py-1.5 text-xs transition"
                  :class="resultFilterMode === 'all' ? 'border-accent bg-accent-soft text-primary' : 'border-default bg-surface text-secondary hover:bg-surface-hover hover:text-primary'"
                  @click="setResultFilterMode('all')"
                >
                  {{ t("page.appSearch.filterMode.all") }}
                </button>
                <button
                  class="rounded-full border px-2.5 py-1.5 text-xs transition"
                  :class="resultFilterMode === 'favorites' ? 'border-accent bg-accent-soft text-primary' : 'border-default bg-surface text-secondary hover:bg-surface-hover hover:text-primary'"
                  @click="setResultFilterMode('favorites')"
                >
                  {{ t("page.appSearch.filterMode.favorites") }}
                </button>
                <div class="ml-auto text-[11px] text-dim">
                  {{ t("page.appSearch.filterHint") }}
                </div>
              </div>
            </div>
            <div class="min-h-0 flex-1 overflow-y-auto">
            <div v-if="!qaMode && showDebugPanel" class="px-4 py-3 text-xs text-secondary">
                <div class="flex items-center justify-between gap-3">
                  <div>
                  <div class="seekmind-section-label">{{ t("page.appSearch.debug.title") }}</div>
                    <div class="seekmind-item-meta mt-1">{{ t("page.appSearch.debug.desc") }}</div>
                  </div>
                  <button class="rounded-md border border-default bg-surface px-2 py-1 text-[11px] text-secondary hover:bg-surface-hover" @click="requestSearchDebugReport">
                    {{ t("common.refresh") }}
                  </button>
                </div>
                <div v-if="debugReportLoading" class="mt-3 rounded-md border border-dashed border-default bg-surface px-3 py-3 text-muted">
                  {{ t("page.appSearch.debug.loading") }}
                </div>
                <SeekMindToast v-else-if="debugReportError" :message="debugReportError" tone="error" />
                <div v-else-if="debugReport" class="mt-3 space-y-3">
                  <div class="grid grid-cols-2 gap-2 lg:grid-cols-4">
                    <div class="rounded-md border border-default bg-surface px-3 py-2">
                      <div class="seekmind-section-label">{{ t("page.appSearch.debug.hits") }}</div>
                      <div class="mt-1 seekmind-metric-value text-primary">{{ debugReport.hit_count }}</div>
                    </div>
                    <div class="rounded-md border border-default bg-surface px-3 py-2">
                      <div class="seekmind-section-label">{{ t("page.appSearch.debug.keywordHits") }}</div>
                      <div class="mt-1 seekmind-metric-value text-primary">{{ debugReport.keyword_hit_count }}</div>
                    </div>
                    <div class="rounded-md border border-default bg-surface px-3 py-2">
                      <div class="seekmind-section-label">{{ t("page.appSearch.debug.semanticHits") }}</div>
                      <div class="mt-1 seekmind-metric-value text-primary">{{ debugReport.semantic_hit_count }}</div>
                    </div>
                    <div class="rounded-md border border-default bg-surface px-3 py-2">
                      <div class="seekmind-section-label">{{ t("page.appSearch.debug.mode") }}</div>
                      <div class="mt-1 text-sm font-medium text-primary">{{ debugReport.search_mode }}</div>
                    </div>
                  </div>
                  <div class="rounded-md border border-default bg-surface px-3 py-2 text-sm text-secondary">
                    <div>{{ t("page.appSearch.debug.normalized", { query: debugReport.normalized_search_text || t("common.none") }) }}</div>
                    <div class="mt-1 break-all">{{ t("page.appSearch.debug.rewritten", { query: debugReport.rewritten_query || t("common.none") }) }}</div>
                    <div class="mt-1 break-all">{{ t("page.appSearch.debug.expanded", { query: debugReport.expanded_query || t("common.none") }) }}</div>
                  </div>
                </div>
              </div>

              <SeekMindToast v-if="!qaMode && errorMessage" :message="errorMessage" tone="error" />

              <template v-if="!qaMode">
                <div v-if="!results.length && !loading" class="m-4 rounded-md border border-dashed border-transparent bg-transparent px-4 py-6 text-center text-xs text-muted">
                  {{ t("page.appSearch.noResults") }}
                </div>

                <div class="space-y-2 pr-2 pb-4">
                  <SeekMindSearchResultGroupCard
                    v-for="group in groupedResults"
                    :key="group.path"
                    :group="group"
                    :query="query"
                    :selected-id="selectedId"
                    :expanded="Boolean(expandedGroups[group.path])"
                    :is-favorited="isResultFavorited"
                    @select="selectResult"
                    @toggle="toggleGroup"
                    @toggle-favorite="toggleFavoriteResult"
                    @contextmenu="handleResultContextMenu"
                  />
                </div>
              </template>

              <template v-else>
                <SeekMindToast v-if="qaErrorMessage" :message="qaErrorMessage" tone="error" />
                <SeekMindToast v-if="qaInfoMessage" :message="qaInfoMessage" tone="success" />
                <div v-if="qaLoading && qaMessages.length === 0" class="m-4 rounded-md border border-transparent bg-transparent px-4 py-6 text-center text-xs text-muted">
                  {{ t("page.appSearch.qa.loading") }}
                </div>
                <div v-else-if="qaMessages.length" class="space-y-3 p-4">
                  <div
                    v-for="message in qaMessages"
                    :key="message.id"
                    class="rounded-lg border border-transparent bg-surface/70 p-4"
                    :class="qaAnswer?.id === message.id ? 'ring-1 ring-accent-soft' : ''"
                    @click="selectQaMessage(message)"
                  >
                    <div class="flex items-center justify-between gap-3">
                      <div class="min-w-0">
                        <div class="seekmind-section-label">{{ t("page.appSearch.qa.answerTitle") }}</div>
                        <div class="mt-1 text-sm font-medium text-primary">{{ message.question }}</div>
                      </div>
                      <div class="flex items-center gap-2">
                        <SeekMindBadge tone="default">
                          {{ qaLoading && qaAnswer?.id === message.id ? qaInfoMessage || t("page.appSearch.qa.streaming") : t(`page.appSearch.qa.state.${message.state}`) }}
                        </SeekMindBadge>
                        <SeekMindBadge v-if="message.state === 'cancelled'" tone="danger">
                          {{ t("page.appSearch.qa.cancelledByUser") }}
                        </SeekMindBadge>
                      </div>
                    </div>
                    <div class="mt-2 whitespace-pre-wrap text-sm leading-7 text-primary">
                      {{ message.answer || t("page.appSearch.qa.noAnswer") }}
                    </div>
                    <div class="mt-3 flex flex-wrap gap-2 text-xs text-dim">
                      <SeekMindBadge tone="default">{{ message.model || t("common.none") }}</SeekMindBadge>
                      <SeekMindBadge tone="default">{{ message.created_at }}</SeekMindBadge>
                      <SeekMindBadge tone="default">{{ t("page.appSearch.qa.sourceCount", { count: message.sources.length }) }}</SeekMindBadge>
                    </div>
                  </div>

                  <div v-if="qaAnswer" class="space-y-2">
                    <div class="seekmind-section-label px-1">{{ t("page.appSearch.qa.sourcesTitle") }}</div>
                    <div
                      v-for="source in qaAnswer.sources"
                      :key="source.source_id"
                      class="cursor-pointer rounded-lg border border-transparent px-3 py-3 transition"
                      :class="qaSelectedSourceId === source.source_id ? 'bg-accent-soft ring-1 ring-accent/20' : 'bg-surface/70 hover:bg-surface-hover'"
                      @click="selectQaSource(source.source_id)"
                    >
                      <div class="flex items-start justify-between gap-3">
                        <div class="min-w-0">
                          <div class="flex flex-wrap items-center gap-2">
                            <SeekMindBadge tone="default">{{ source.source_id }}</SeekMindBadge>
                            <span class="text-sm font-medium text-primary">{{ source.file_name }}</span>
                          </div>
                          <div class="mt-1 text-xs text-muted">{{ source.path }}</div>
                          <div class="mt-1 text-[11px] text-dim">
                            {{ source.title_path || source.heading }}
                          </div>
                        </div>
                        <div class="text-right text-xs text-dim">
                          <div>{{ Math.round(source.score * 100) }}%</div>
                          <div class="mt-1">{{ source.rank_reason }}</div>
                        </div>
                      </div>
                      <div class="mt-2 text-sm leading-6 text-secondary">
                        {{ source.snippet }}
                      </div>
                    </div>
                  </div>
                </div>
                <div v-else class="m-4 rounded-md border border-transparent bg-transparent px-4 py-6 text-center text-xs text-muted">
                  {{ t("page.appSearch.qa.enterQuestion") }}
                </div>
              </template>
            </div>
          </section>
        </template>

        <template #right>
          <aside class="seekmind-pane-detail flex h-full min-h-0 flex-1 flex-col overflow-hidden">
            <SeekMindDetailPanel v-if="!qaMode && selected">
              <template #header>
                <div class="flex items-start justify-between gap-3">
                  <div class="min-w-0">
                    <div class="truncate text-base font-semibold leading-6 text-primary" :title="selected.file_name">
                      {{ selected.file_name }}
                    </div>
                    <div class="mt-0.5 truncate text-xs leading-5 text-muted" :title="selected.path">
                      {{ selected.path }}
                    </div>
                  </div>
                  <div class="flex items-center gap-2">
                    <div class="seekmind-file-icon flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-surface text-[10px] font-semibold text-secondary">
                      {{ selected.ext.toUpperCase() }}
                    </div>
                    <button
                      class="seekmind-close-button"
                      type="button"
                      :title="t('common.close')"
                      @click="closeSelectedResult"
                    >
                      <X :size="13" stroke-width="2.25" />
                    </button>
                  </div>
                </div>
                <div v-if="selectedTitlePath" class="mt-2" :title="selectedTitlePath">
                  <div class="text-[11px] font-semibold uppercase tracking-[0.16em] text-dim">
                    {{ t("page.appSearch.detail.titlePath") }}
                  </div>
                  <div class="mt-1 text-sm leading-6 text-primary">
                    {{ selectedTitlePath }}
                  </div>
                </div>
              </template>

              <SeekMindDetailSection :title="t('common.overview')" :subtitle="selectedCitation">
                <div class="flex flex-wrap gap-1.5">
                  <SeekMindBadge>{{ selected.ext.toUpperCase() }}</SeekMindBadge>
                  <SeekMindBadge>{{ selected.page ? t("searchResultCard.page", { page: selected.page }) : t("searchResultCard.paragraph", { para: selected.paragraph }) }}</SeekMindBadge>
                  <SeekMindBadge v-if="selected.page" tone="default">{{ t("page.appSearch.detail.pdfPage", { page: selected.page }) }}</SeekMindBadge>
                  <SeekMindBadge tone="success">{{ t("searchResultCard.matchField", { field: matchedFieldLabel }) }}</SeekMindBadge>
                  <SeekMindBadge tone="default">{{ selected.snippet_window_start }}-{{ selected.snippet_window_end }} / {{ selected.snippet_source_len }}</SeekMindBadge>
                  <SeekMindBadge tone="default">{{ selectedChunkPositionLabel }}</SeekMindBadge>
                  <SeekMindBadge tone="default">{{ t("page.appSearch.detail.chunkCount", { count: selectedChunkCount ?? '...' }) }}</SeekMindBadge>
                  <SeekMindBadge tone="default"><Clock class="mr-1 inline" :size="12" />{{ selected.modified }}</SeekMindBadge>
                </div>
                <div class="rounded-[14px] bg-white/70 px-3 py-3 text-sm leading-6 text-secondary">
                  {{ selected.rank_reason.summary || selected.rank_reason.match_origin || selected.match_origin || t('common.none') }}
                </div>
                <!-- 修复：命中原因摘要已经包含 boosts 的信息，这里不再重复展开同一组标签，避免视觉上像重复命中。 -->
              </SeekMindDetailSection>

              <SeekMindDetailSection :title="t('common.originalText')">
                <div v-if="selectedPreviewBlocks.length > 0" class="space-y-2">
                  <SeekMindPreviewBlockRenderer
                    v-for="block in selectedPreviewBlocks"
                    :key="block.block_index"
                    :block="block"
                  />
                </div>
                <SeekMindMarkdownRenderer
                  v-else
                  :block="{
                    block_index: 0,
                    block_type: 'paragraph',
                    text: selected.snippet,
                    heading: selected.title_path || selected.heading,
                    level: null,
                    page: selected.page ?? null,
                    language: null,
                    markdown: '',
                    html: '',
                  }"
                  :query="query"
                  :highlight-text="selected.snippet"
                  :highlight-spans="selected.highlight_spans"
                />
              </SeekMindDetailSection>

              <SeekMindDetailSection :title="t('common.context')">
                <div v-if="selectedChunk" class="space-y-3">
                  <div
                    v-for="item in selectedContextChunks"
                    :key="item.key"
                    class="seekmind-context-row"
                    :class="item.key === 'current' ? 'seekmind-context-row--active' : ''"
                  >
                    <div class="seekmind-section-label" :class="item.key === 'current' ? 'text-accent-text' : 'text-dim'">
                      {{ item.label }}
                    </div>
                    <div v-if="item.chunk?.title_path || item.chunk?.heading" class="mt-1 text-[11px] text-dim">
                      {{ t("page.appSearch.detail.titlePath") }}：{{ item.chunk?.title_path || item.chunk?.heading }}
                    </div>
                    <div class="mt-1 text-sm leading-7" :class="item.key === 'current' ? 'text-primary' : 'text-secondary'">
                      <SeekMindHighlightedText
                        v-if="item.key === 'current' && !(item.chunk?.preview_blocks?.length)"
                        :text="item.chunk?.snippet ?? ''"
                        :query="query"
                        :spans="selected.highlight_spans"
                      />
                      <div v-else-if="item.chunk?.preview_blocks?.length" class="space-y-2">
                        <SeekMindPreviewBlockRenderer
                          v-for="block in item.chunk.preview_blocks"
                          :key="block.block_index"
                          :block="block"
                        />
                      </div>
                      <SeekMindMarkdownRenderer
                        v-else
                        :block="{
                          block_index: 0,
                          block_type: 'paragraph',
                          text: item.chunk?.snippet ?? '',
                          heading: item.chunk?.title_path || item.chunk?.heading || '',
                          level: null,
                          page: item.chunk?.page ?? null,
                          language: null,
                          markdown: '',
                          html: '',
                        }"
                      />
                    </div>
                    <div class="seekmind-item-meta mt-1">
                      {{ item.chunk?.page ? t("page.appSearch.detail.pdfPage", { page: item.chunk.page }) : t("searchResultCard.paragraph", { para: item.chunk?.paragraph ?? '-' }) }}
                    </div>
                  </div>
                </div>
                <div v-else class="rounded-md bg-surface px-3 py-3 text-sm text-muted">
                  {{ t("page.appSearch.detail.noContext") }}
                </div>
                <p class="seekmind-item-meta mt-3">{{ t("page.appSearch.detail.snippetSource", { start: selected.snippet_window_start, end: selected.snippet_window_end, length: selected.snippet_source_len }) }}</p>
              </SeekMindDetailSection>

              <SeekMindToast v-if="actionMessage" :message="actionMessage" tone="success" />
              <SeekMindToast v-if="actionErrorMessage" :message="actionErrorMessage" tone="error" />
            </SeekMindDetailPanel>

            <SeekMindDetailPanel v-else-if="qaMode && qaSelectedSource">
              <template #header>
                <div class="flex items-start justify-between gap-3">
                  <div class="min-w-0">
                    <div class="truncate text-base font-semibold leading-6 text-primary" :title="qaSelectedSource.file_name">
                      {{ qaSelectedSource.file_name }}
                    </div>
                    <div class="mt-0.5 truncate text-xs leading-5 text-muted" :title="qaSelectedSource.path">
                      {{ qaSelectedSource.path }}
                    </div>
                  </div>
                  <div class="flex items-center gap-2">
                    <div class="seekmind-file-icon flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-surface text-[10px] font-semibold text-secondary">
                      {{ qaSelectedSource.ext.toUpperCase() }}
                    </div>
                    <button
                      class="seekmind-close-button"
                      type="button"
                      :title="t('common.close')"
                      @click="closeSelectedQaSource"
                    >
                      <X :size="13" stroke-width="2.25" />
                    </button>
                  </div>
                </div>
                <div v-if="qaSelectedTitlePath" class="mt-2" :title="qaSelectedTitlePath">
                  <div class="text-[11px] font-semibold uppercase tracking-[0.16em] text-dim">
                    {{ t("page.appSearch.detail.titlePath") }}
                  </div>
                  <div class="mt-1 text-sm leading-6 text-primary">
                    {{ qaSelectedTitlePath }}
                  </div>
                </div>
              </template>

              <SeekMindDetailSection :title="t('common.overview')" :subtitle="qaSelectedCitation">
                <div class="flex flex-wrap gap-1.5">
                  <SeekMindBadge>{{ qaSelectedSource.ext.toUpperCase() }}</SeekMindBadge>
                  <SeekMindBadge>{{ qaSelectedSource.page ? t("searchResultCard.page", { page: qaSelectedSource.page }) : t("searchResultCard.paragraph", { para: qaSelectedSource.paragraph }) }}</SeekMindBadge>
                  <SeekMindBadge v-if="qaSelectedSource.page" tone="default">{{ t("page.appSearch.detail.pdfPage", { page: qaSelectedSource.page }) }}</SeekMindBadge>
                  <SeekMindBadge tone="success">{{ t("page.appSearch.qa.sourceId", { id: qaSelectedSource.source_id }) }}</SeekMindBadge>
                  <SeekMindBadge tone="default">{{ qaAnswer?.retrieval.search_mode || t("common.none") }}</SeekMindBadge>
                  <SeekMindBadge tone="default">{{ qaAnswer?.retrieval.selected_count ?? 0 }}/{{ qaAnswer?.retrieval.candidate_count ?? 0 }}</SeekMindBadge>
                </div>
                <div class="rounded-[14px] bg-white/70 px-3 py-3 text-sm leading-6 text-secondary">
                  {{ qaSelectedSource.rank_reason || t('common.none') }}
                </div>
              </SeekMindDetailSection>

              <SeekMindDetailSection :title="t('common.originalText')">
                <div v-if="qaSelectedPreviewBlocks.length > 0" class="space-y-2">
                  <SeekMindPreviewBlockRenderer
                    v-for="block in qaSelectedPreviewBlocks"
                    :key="block.block_index"
                    :block="block"
                  />
                </div>
                <div v-else class="whitespace-pre-wrap rounded-[14px] bg-white/78 px-3 py-3 text-sm leading-7 text-primary">
                  {{ qaSelectedSource.snippet }}
                </div>
              </SeekMindDetailSection>

              <SeekMindDetailSection :title="t('common.context')">
                <div class="grid gap-2 text-sm leading-6 text-secondary">
                  <div>
                    {{ t("page.appSearch.qa.sourceMeta") }}：{{ qaSelectedCitation || t("common.none") }}
                  </div>
                  <div>
                    {{ t("common.matchReason") }}：{{ qaSelectedSource.rank_reason || t("common.none") }}
                  </div>
                </div>
              </SeekMindDetailSection>

              <div class="flex flex-wrap gap-2">
                <button class="rounded-md border border-default bg-surface px-3 py-2 text-xs text-secondary hover:bg-surface-hover" @click="openSelectedQaFile">
                  {{ t("page.appSearch.detail.openFile") }}
                </button>
                <button class="rounded-md border border-default bg-surface px-3 py-2 text-xs text-secondary hover:bg-surface-hover" @click="viewQaChunks">
                  {{ t("page.appSearch.detail.viewChunks") }}
                </button>
                <button class="rounded-md border border-default bg-surface px-3 py-2 text-xs text-secondary hover:bg-surface-hover" @click="quickLookSelectedQaFile">
                  {{ t("page.appSearch.detail.quickLook") }}
                </button>
                <button class="rounded-md border border-default bg-surface px-3 py-2 text-xs text-secondary hover:bg-surface-hover" @click="copySelectedQaPath">
                  {{ t("page.appSearch.detail.copyPath") }}
                </button>
                <button class="rounded-md border border-default bg-surface px-3 py-2 text-xs text-secondary hover:bg-surface-hover" @click="copySelectedQaCitation">
                  {{ t("page.appSearch.detail.copyCitation") }}
                </button>
              </div>
            </SeekMindDetailPanel>

            <div v-else class="rounded-lg border border-dashed border-default bg-surface p-6 text-sm text-muted">
              {{ qaMode ? t("page.appSearch.qa.noSourceSelected") : t("page.appSearch.noResults") }}
            </div>
          </aside>
        </template>
      </SplitPane>

      <SeekMindContextMenu
        v-if="resultMenuVisible"
        :items="resultContextMenuItems"
        :x="resultMenuPosition.x"
        :y="resultMenuPosition.y"
        @close="resultMenuVisible = false"
      />
      <SeekMindCollectionPicker
        :visible="collectionPickerVisible"
        :collections="collections"
        :loading="collectionPickerLoading"
        :title="collectionPickerTarget ? (collectionPickerTarget.file_name || collectionPickerTarget.path || t('page.collections.pickerTitle')) : t('page.collections.pickerTitle')"
        :subtitle="collectionPickerTarget ? collectionPickerTarget.path : t('page.collections.pickerSubtitle')"
        @close="collectionPickerVisible = false"
        @select="handleCollectionPickerSelect"
        @create="handleCollectionPickerCreate"
      />
    </main>
  </div>
</template>
