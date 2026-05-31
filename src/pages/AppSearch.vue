<script setup lang="ts">
defineOptions({
  name: "AppSearchPage",
});

import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { listen } from "@tauri-apps/api/event";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";
import { AlertCircle, ClipboardCopy, Clock, Eye, FileText, Files, Filter, Link2, MessageSquareText, Search, SquareArrowOutUpRight } from "lucide-vue-next";
import DocMindBadge from "../components/docmind/DocMindBadge.vue";
import DocMindContextMenu from "../components/docmind/DocMindContextMenu.vue";
import type { ContextMenuItem } from "../components/docmind/DocMindContextMenu.vue";
import DocMindHighlightedText from "../components/docmind/DocMindHighlightedText.vue";
import DocMindMarkdownRenderer from "../components/docmind/DocMindMarkdownRenderer.vue";
import DocMindSearchResultGroupCard from "../components/docmind/DocMindSearchResultGroupCard.vue";
import SplitPane from "../components/SplitPane.vue";
import { useQuickAccessData } from "../composables/useQuickAccessData";
import { docmindApi, formatDocmindError } from "../services/docmindApi";
import { buildDocumentLocationParts, formatDocumentCitation, resolveDocumentTitlePath } from "../utils/citation";

const { t } = useI18n();
import type {
  ChunkView,
  ParserRuntimeView,
  QaAnswerView,
  QaAnswerProgressView,
  QaAskStartView,
  QaSettingsView,
  SearchDebugReportEventView,
  SearchDebugView,
  SearchResultView,
} from "../types/docmind";

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
const routeSyncReady = ref(false);
let selectedContextRequestId = 0;
let unlistenSearchDebugReport: null | (() => void) = null;
let unlistenQaProgress: null | (() => void) = null;
const { favorites, loadQuickAccessData } = useQuickAccessData();

interface SearchResultGroup {
  path: string;
  fileName: string;
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
  () => qaAnswer.value?.sources.find((item) => item.source_id === qaSelectedSourceId.value) ?? qaAnswer.value?.sources[0] ?? null,
);

const selectedTitlePath = computed(() =>
  resolveDocumentTitlePath({
    fileName: selected.value?.fileName,
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
    fileName: selected.value.fileName,
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

const groupedResults = computed<SearchResultGroup[]>(() => {
  const groups = new Map<string, SearchResultView[]>();

  for (const item of results.value) {
    const items = groups.get(item.path) ?? [];
    items.push(item);
    groups.set(item.path, items);
  }

  return [...groups.entries()]
    .map(([path, items]) => {
      const sorted = [...items].sort((a, b) => b.score - a.score);
      return {
        path,
        fileName: sorted[0]?.fileName ?? path,
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
    console.error("[DocMind] copyText failed", error);
    setActionError(t("page.appSearch.detail.copyFailed"));
  }
};

const handleGlobalShortcut = (event: KeyboardEvent) => {
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
    event.preventDefault();
    focusSearchInput();
  }
};

const isSearchClickDebugEnabled = () => globalThis.localStorage?.getItem("DOCMIND_DEBUG_SEARCH_CLICK") === "1";

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

  selectedId.value = id;
};

const toggleGroup = (path: string) => {
  expandedGroups.value = {
    ...expandedGroups.value,
    [path]: !expandedGroups.value[path],
  };
};

const loadParserRuntime = async () => {
  parserRuntime.value = await docmindApi.getParserRuntime();
};

const loadQaSettings = async () => {
  try {
    qaSettings.value = await docmindApi.getQaSettings();
  } catch (error) {
    console.error("[DocMind] getQaSettings failed", error);
  }
};

const loadLatestQaSession = async () => {
  try {
    const sessions = await docmindApi.listQaSessions(1);
    const latest = sessions[0];
    if (!latest) {
      return;
    }

    qaSessionId.value = latest.id;
    qaSessionTitle.value = latest.title;
    const messages = await docmindApi.listQaMessages(latest.id, 50);
    qaMessages.value = messages;
    qaAnswer.value = messages[messages.length - 1] ?? null;
    qaSelectedSourceId.value = qaAnswer.value?.sources[0]?.source_id ?? "";
  } catch (error) {
    console.error("[DocMind] loadLatestQaSession failed", error);
  }
};

const ensureQaSession = async (title: string) => {
  if (qaSessionId.value) {
    return qaSessionId.value;
  }

  const session = await docmindApi.createQaSession(title);
  qaSessionId.value = session.id;
  qaSessionTitle.value = session.title;
  return session.id;
};

const installQaProgressListener = async () => {
  if (unlistenQaProgress) {
    return;
  }

  unlistenQaProgress = await listen<QaAnswerProgressView>(
    "docmind:qa:answer-progress",
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
      qaSelectedSourceId.value = qaSelectedSourceId.value || payload.sources[0]?.source_id || "";

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
    await loadQaSettings();
    if (!isQaConfigured(qaSettings.value)) {
      qaAnswer.value = null;
      qaSelectedSourceId.value = "";
      qaInfoMessage.value = t("page.appSearch.qa.notConfigured");
      qaLoading.value = false;
      return;
    }

    const questionText = qaQuestion.value.trim();
    const sessionId = await ensureQaSession(questionText);
    const started: QaAskStartView = await docmindApi.askQuestion(questionText, [], 6, sessionId);
    qaActiveJobId.value = started.job_id;
    qaAnswer.value = started.status;
    qaMessages.value.push(started.status);
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
    qaSelectedSourceId.value = "";
    qaErrorMessage.value = formatDocmindError(error, t("page.appSearch.qa.askFailed"));
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
  qaSelectedSourceId.value = "";
  qaInfoMessage.value = "";
  qaErrorMessage.value = "";
  qaActiveJobId.value = "";
};

const selectQaMessage = (message: QaAnswerView) => {
  qaAnswer.value = message;
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
    await docmindApi.cancelQaQuestion(jobId);
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
  qaSelectedSourceId.value = sourceId;
};

const openSelectedQaFile = async () => {
  if (!qaSelectedSource.value) return;
  await docmindApi.openFile(qaSelectedSource.value.path);
  await loadQuickAccessData();
};

const quickLookSelectedQaFile = async () => {
  if (!qaSelectedSource.value) return;

  try {
    await docmindApi.quickLookFile(qaSelectedSource.value.path);
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
    "docmind:search-debug-report",
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
    await docmindApi.requestSearchDebugReport(requestId, query.value, 20);
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
    const chunks = await docmindApi.listDocumentChunks(current.path);
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
    console.error("[DocMind] loadSelectedContext failed", error);
  }
};

const runSearch = async () => {
  if (!query.value.trim()) {
    results.value = [];
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
    results.value = await docmindApi.searchDocuments(query.value, 20);
    selectedId.value = "";
    expandedGroups.value = {};
    if (showDebugPanel.value) {
      await requestSearchDebugReport();
    } else {
      clearSearchDebugReport();
    }
    await loadQuickAccessData();
  } catch (error) {
    results.value = [];
    selectedId.value = "";
    expandedGroups.value = {};
    errorMessage.value = error instanceof Error ? error.message : t("page.appSearch.searchFailed");
    if (showDebugPanel.value) {
      await requestSearchDebugReport();
    } else {
      clearSearchDebugReport();
    }
    await loadQuickAccessData();
  } finally {
    loading.value = false;
  }
};

const openSelectedFile = async () => {
  if (!selected.value) return;
  await docmindApi.openFile(selected.value.path);
  await loadQuickAccessData();
};

const quickLookSelectedFile = async () => {
  if (!selected.value) return;

  try {
    await docmindApi.quickLookFile(selected.value.path);
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
  await docmindApi.toggleResultFavorite(
    item.path,
    item.heading,
    item.paragraph ?? null,
    item.page ?? null,
    item.fileName,
  );
  await loadQuickAccessData();
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
  await installSearchDebugReportListener();
  await Promise.all([loadParserRuntime(), loadQuickAccessData(), loadQaSettings(), loadLatestQaSession(), installQaProgressListener()]);
  query.value = routeSearchQuery.value;
  if (query.value.trim()) {
    await runSearch();
  } else {
    results.value = [];
    selectedId.value = "";
    expandedGroups.value = {};
    clearSearchDebugReport();
  }
  routeSyncReady.value = true;
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", handleGlobalShortcut);
  unlistenSearchDebugReport?.();
  unlistenSearchDebugReport = null;
  unlistenQaProgress?.();
  unlistenQaProgress = null;
});

watch(query, () => {
  if (!query.value.trim()) {
    results.value = [];
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

watch(showDebugPanel, async (visible) => {
  if (!visible) {
    clearSearchDebugReport();
    return;
  }

  if (query.value.trim()) {
    await requestSearchDebugReport();
  }
});

watch(qaMode, async (visible) => {
  if (visible) {
    await loadQaSettings();
    if (!qaSessionId.value && qaMessages.value.length === 0) {
      await loadLatestQaSession();
    }
    await installQaProgressListener();
  }
});
</script>

<template>
  <div class="flex h-full min-h-0 flex-col bg-page text-primary">
    <main class="flex min-h-0 flex-1 overflow-hidden">
      <SplitPane :panels="splitPanels">
        <template #center>
          <section class="flex min-h-0 flex-1 flex-col overflow-hidden bg-panel/70">
            <div class="border-b border-default bg-surface px-4 py-3 space-y-3">
              <div class="flex items-center justify-between gap-3">
                <div>
                  <div class="docmind-section-label">{{ qaMode ? t("page.appSearch.qa.title") : t("page.appSearch.title") }}</div>
                  <div class="docmind-item-meta mt-1">
                    {{ qaMode ? t("page.appSearch.qa.subtitle") : t("page.appSearch.subtitle") }}
                  </div>
                </div>
                <div class="inline-flex rounded-md border border-default bg-panel p-1 text-xs font-medium">
                  <button
                    class="rounded px-3 py-1.5 transition"
                    :class="qaMode ? 'text-secondary hover:bg-surface-hover' : 'bg-accent text-white'"
                    @click.prevent="qaMode = false"
                  >
                    {{ t("page.appSearch.mode.search") }}
                  </button>
                  <button
                    class="rounded px-3 py-1.5 transition"
                    :class="qaMode ? 'bg-accent text-white' : 'text-secondary hover:bg-surface-hover'"
                    @click.prevent="qaMode = true"
                  >
                    <MessageSquareText :size="14" class="mr-1 inline" />
                    {{ t("page.appSearch.mode.qa") }}
                  </button>
                </div>
              </div>

              <form
                v-if="!qaMode"
                class="mx-auto flex h-9 w-full max-w-none items-center gap-2 rounded-md border border-default bg-input px-3 transition focus-within:border-accent focus-within:ring-2 focus-within:ring-accent-soft"
                @submit.prevent="runSearch"
              >
                <Search :size="15" class="shrink-0 text-muted" />
                <input
                  ref="searchInputRef"
                  v-model="query"
                  :placeholder="t('page.appSearch.placeholder')"
                  class="min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-muted"
                />
                <span class="hidden text-xs text-muted sm:inline">Cmd+K</span>
              </form>

              <form v-else class="space-y-3" @submit.prevent="runQa">
                <textarea
                  v-model="qaQuestion"
                  rows="3"
                  class="w-full rounded-md border border-default bg-input px-3 py-2.5 text-sm text-primary outline-none transition focus:border-accent focus:bg-surface"
                  :placeholder="t('page.appSearch.qa.placeholder')"
                />
                <div class="flex items-center justify-between gap-3">
                  <div class="text-xs text-dim">
                    {{ isQaConfigured(qaSettings) ? t("page.appSearch.qa.ready") : t("page.appSearch.qa.notConfigured") }}
                    <span v-if="qaSessionTitle" class="ml-2 text-muted">· {{ qaSessionTitle }}</span>
                  </div>
                  <div class="flex items-center gap-2">
                    <button
                      v-if="qaMessages.length"
                      class="inline-flex items-center gap-2 rounded-md border border-default bg-surface px-3 py-2 text-sm font-medium text-secondary hover:bg-surface-hover disabled:cursor-not-allowed disabled:opacity-70"
                      type="button"
                      :disabled="qaLoading || qaCancelling"
                      @click="newQaSession"
                    >
                      {{ t("page.appSearch.qa.newSession") }}
                    </button>
                    <button
                      v-if="qaLoading || qaCancelling"
                      class="inline-flex items-center gap-2 rounded-md border border-default bg-surface px-3 py-2 text-sm font-medium text-secondary hover:bg-surface-hover"
                      type="button"
                      @click="stopQa"
                    >
                      {{ qaCancelling ? t("page.appSearch.qa.stopping") : t("page.appSearch.qa.stop") }}
                    </button>
                    <button
                      class="inline-flex items-center gap-2 rounded-md bg-accent px-3 py-2 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-70"
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
                  <div class="docmind-item-meta mt-1 leading-5 text-secondary">
                    {{ officeNotice.desc }}
                  </div>
                  <div class="docmind-item-meta mt-1 leading-5">
                    {{ officeNotice.hint }}
                  </div>
                </div>
              </div>
            </div>

            <div v-if="!qaMode" class="flex items-center justify-between gap-3 border-b border-default bg-surface px-4 py-2">
              <div class="text-xs font-medium text-dim">
                {{ t("page.appSearch.stats.foundDocs", { count: groupedResults.length, total: results.length }) }}
              </div>
              <button class="flex items-center gap-1 rounded-md px-2 py-1 text-xs text-accent-text hover:bg-accent-soft">
                <Filter :size="14" />
                {{ t("page.appSearch.filter") }}
              </button>
            </div>
            <div v-else class="flex items-center justify-between gap-3 border-b border-default bg-surface px-4 py-2">
              <div class="text-xs font-medium text-dim">
                {{ qaAnswer ? t("page.appSearch.qa.resultCount", { count: qaAnswer.sources.length }) : t("page.appSearch.qa.waiting") }}
              </div>
              <div class="flex items-center gap-2 text-xs text-dim">
                <DocMindBadge :tone="qaStateTone">
                  {{ qaStateLabel }}
                </DocMindBadge>
                <DocMindBadge tone="default">
                  {{ isQaConfigured(qaSettings) ? t("page.appSearch.qa.enabled") : t("page.appSearch.qa.disabled") }}
                </DocMindBadge>
              </div>
            </div>

            <div class="min-h-0 flex-1 overflow-y-auto">
              <div v-if="!qaMode && showDebugPanel" class="border-b border-default bg-panel px-4 py-3 text-xs text-secondary">
                <div class="flex items-center justify-between gap-3">
                  <div>
                  <div class="docmind-section-label">{{ t("page.appSearch.debug.title") }}</div>
                    <div class="docmind-item-meta mt-1">{{ t("page.appSearch.debug.desc") }}</div>
                  </div>
                  <button class="rounded-md border border-default bg-surface px-2 py-1 text-[11px] text-secondary hover:bg-surface-hover" @click="requestSearchDebugReport">
                    {{ t("common.refresh") }}
                  </button>
                </div>
                <div v-if="debugReportLoading" class="mt-3 rounded-md border border-dashed border-default bg-surface px-3 py-3 text-muted">
                  {{ t("page.appSearch.debug.loading") }}
                </div>
                <div v-else-if="debugReportError" class="mt-3 rounded-md border border-danger-soft bg-danger-soft px-3 py-3 text-danger">
                  {{ debugReportError }}
                </div>
                <div v-else-if="debugReport" class="mt-3 space-y-3">
                  <div class="grid grid-cols-2 gap-2 lg:grid-cols-4">
                    <div class="rounded-md border border-default bg-surface px-3 py-2">
                      <div class="docmind-section-label">{{ t("page.appSearch.debug.hits") }}</div>
                      <div class="mt-1 docmind-metric-value text-primary">{{ debugReport.hit_count }}</div>
                    </div>
                    <div class="rounded-md border border-default bg-surface px-3 py-2">
                      <div class="docmind-section-label">{{ t("page.appSearch.debug.keywordHits") }}</div>
                      <div class="mt-1 docmind-metric-value text-primary">{{ debugReport.keyword_hit_count }}</div>
                    </div>
                    <div class="rounded-md border border-default bg-surface px-3 py-2">
                      <div class="docmind-section-label">{{ t("page.appSearch.debug.semanticHits") }}</div>
                      <div class="mt-1 docmind-metric-value text-primary">{{ debugReport.semantic_hit_count }}</div>
                    </div>
                    <div class="rounded-md border border-default bg-surface px-3 py-2">
                      <div class="docmind-section-label">{{ t("page.appSearch.debug.mode") }}</div>
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

              <div v-if="!qaMode && errorMessage" class="m-4 rounded-md border border-danger-soft bg-danger-soft px-4 py-3 text-sm text-danger">
                {{ errorMessage }}
              </div>

              <template v-if="!qaMode">
                <div v-if="!results.length && !loading" class="m-4 rounded-md border border-dashed border-default bg-surface px-4 py-6 text-center text-xs text-muted">
                  {{ t("page.appSearch.noResults") }}
                </div>

                <div class="space-y-3 pr-2 pb-4">
                  <DocMindSearchResultGroupCard
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
                <div v-if="qaErrorMessage" class="m-4 rounded-md border border-danger-soft bg-danger-soft px-4 py-3 text-sm text-danger">
                  {{ qaErrorMessage }}
                </div>
                <div v-if="qaInfoMessage" class="m-4 rounded-md border border-emerald-soft bg-emerald-soft px-4 py-3 text-sm text-success">
                  {{ qaInfoMessage }}
                </div>
                <div v-if="qaLoading && qaMessages.length === 0" class="m-4 rounded-md border border-dashed border-default bg-surface px-4 py-6 text-center text-xs text-muted">
                  {{ t("page.appSearch.qa.loading") }}
                </div>
                <div v-else-if="qaMessages.length" class="space-y-3 p-4">
                  <div
                    v-for="message in qaMessages"
                    :key="message.id"
                    class="rounded-lg border border-default bg-surface p-4"
                    :class="qaAnswer?.id === message.id ? 'ring-1 ring-accent-soft' : ''"
                    @click="selectQaMessage(message)"
                  >
                    <div class="flex items-center justify-between gap-3">
                      <div class="min-w-0">
                        <div class="docmind-section-label">{{ t("page.appSearch.qa.answerTitle") }}</div>
                        <div class="mt-1 text-sm font-medium text-primary">{{ message.question }}</div>
                      </div>
                      <div class="flex items-center gap-2">
                        <DocMindBadge tone="default">
                          {{ qaLoading && qaAnswer?.id === message.id ? qaInfoMessage || t("page.appSearch.qa.streaming") : t(`page.appSearch.qa.state.${message.state}`) }}
                        </DocMindBadge>
                        <DocMindBadge v-if="message.state === 'cancelled'" tone="danger">
                          {{ t("page.appSearch.qa.cancelledByUser") }}
                        </DocMindBadge>
                      </div>
                    </div>
                    <div class="mt-2 whitespace-pre-wrap text-sm leading-7 text-primary">
                      {{ message.answer || t("page.appSearch.qa.noAnswer") }}
                    </div>
                    <div class="mt-3 flex flex-wrap gap-2 text-xs text-dim">
                      <DocMindBadge tone="default">{{ message.model || t("common.none") }}</DocMindBadge>
                      <DocMindBadge tone="default">{{ message.created_at }}</DocMindBadge>
                      <DocMindBadge tone="default">{{ t("page.appSearch.qa.sourceCount", { count: message.sources.length }) }}</DocMindBadge>
                    </div>
                  </div>

                  <div v-if="qaAnswer" class="space-y-2">
                    <div class="docmind-section-label px-1">{{ t("page.appSearch.qa.sourcesTitle") }}</div>
                    <div
                      v-for="source in qaAnswer.sources"
                      :key="source.source_id"
                      class="cursor-pointer rounded-lg border px-3 py-3 transition"
                      :class="qaSelectedSourceId === source.source_id ? 'border-accent bg-accent-soft' : 'border-default bg-surface hover:border-accent'"
                      @click="selectQaSource(source.source_id)"
                    >
                      <div class="flex items-start justify-between gap-3">
                        <div class="min-w-0">
                          <div class="flex flex-wrap items-center gap-2">
                            <DocMindBadge tone="default">{{ source.source_id }}</DocMindBadge>
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
                <div v-else class="m-4 rounded-md border border-dashed border-default bg-surface px-4 py-6 text-center text-xs text-muted">
                  {{ t("page.appSearch.qa.enterQuestion") }}
                </div>
              </template>
            </div>
          </section>
        </template>

        <template #right>
          <aside class="min-h-0 flex-1 overflow-y-auto bg-panel/70 p-5">
            <div v-if="!qaMode && selected" class="docmind-detail">
              <div class="mb-4 rounded-lg border border-default bg-panel p-4">
                <div class="flex items-start justify-between gap-3">
                  <div class="min-w-0">
                    <div class="text-lg font-semibold text-primary">{{ selected.fileName }}</div>
                    <div class="mt-1 break-all text-xs text-muted">{{ selected.path }}</div>
                  </div>
                  <div class="docmind-file-icon flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-surface text-[11px] font-semibold text-secondary">
                    {{ selected.ext.toUpperCase() }}
                  </div>
                </div>
                <div v-if="selectedTitlePath" class="mt-3 rounded-md border border-default bg-surface px-3 py-2">
                  <div class="text-[11px] font-semibold uppercase tracking-[0.16em] text-dim">
                    {{ t("page.appSearch.detail.titlePath") }}
                  </div>
                  <div class="mt-1 text-sm leading-6 text-primary">
                    {{ selectedTitlePath }}
                  </div>
                </div>
              </div>

              <div class="mb-4 flex flex-wrap gap-2">
                <DocMindBadge>{{ selected.ext.toUpperCase() }}</DocMindBadge>
                <DocMindBadge>{{ selected.page ? t("searchResultCard.page", { page: selected.page }) : t("searchResultCard.paragraph", { para: selected.paragraph }) }}</DocMindBadge>
                <DocMindBadge v-if="selected.page" tone="default">{{ t("page.appSearch.detail.pdfPage", { page: selected.page }) }}</DocMindBadge>
                <DocMindBadge tone="success">{{ t("searchResultCard.matchField", { field: matchedFieldLabel }) }}</DocMindBadge>
                <DocMindBadge tone="default">{{ selected.snippet_window_start }}-{{ selected.snippet_window_end }} / {{ selected.snippet_source_len }}</DocMindBadge>
                <DocMindBadge tone="default">{{ selectedChunkPositionLabel }}</DocMindBadge>
                <DocMindBadge tone="default">{{ t("page.appSearch.detail.chunkCount", { count: selectedChunkCount ?? "..." }) }}</DocMindBadge>
                <DocMindBadge tone="default"><Clock class="mr-1 inline" :size="12" />{{ selected.modified }}</DocMindBadge>
              </div>

              <div class="mb-4 rounded-lg border border-default bg-surface p-4">
                <div class="mb-2 text-sm font-medium text-secondary">{{ t("page.appSearch.detail.hitParagraph") }}</div>
                <DocMindMarkdownRenderer
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
              </div>

              <div class="rounded-lg border border-default bg-panel p-4">
                <div class="mb-2 text-sm font-medium text-secondary">{{ t("page.appSearch.detail.contextPreview") }}</div>
                <div v-if="selectedChunk" class="space-y-3">
                  <div
                    v-for="item in selectedContextChunks"
                    :key="item.key"
                    class="rounded-md border px-3 py-2"
                    :class="item.key === 'current' ? 'border-accent bg-surface' : 'border-default bg-surface/70'"
                  >
                    <div class="docmind-section-label" :class="item.key === 'current' ? 'text-accent-text' : 'text-dim'">
                      {{ item.label }}
                    </div>
                    <div v-if="item.chunk?.title_path || item.chunk?.heading" class="mt-1 text-[11px] text-dim">
                      {{ t("page.appSearch.detail.titlePath") }}：{{ item.chunk?.title_path || item.chunk?.heading }}
                    </div>
                    <div class="mt-1 text-sm leading-7" :class="item.key === 'current' ? 'text-primary' : 'text-secondary'">
                      <DocMindHighlightedText
                        v-if="item.key === 'current'"
                        :text="item.chunk?.snippet ?? ''"
                        :query="query"
                        :spans="selected.highlight_spans"
                      />
                      <DocMindMarkdownRenderer
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
                    <div class="docmind-item-meta mt-1">
                      {{ item.chunk?.page ? t("page.appSearch.detail.pdfPage", { page: item.chunk.page }) : t("searchResultCard.paragraph", { para: item.chunk?.paragraph ?? "-" }) }}
                    </div>
                  </div>
                </div>
                <div v-else class="rounded-md border border-dashed border-default bg-surface px-3 py-3 text-sm text-muted">
                  {{ t("page.appSearch.detail.noContext") }}
                </div>
                <p class="docmind-item-meta mt-3">{{ t("page.appSearch.detail.snippetSource", { start: selected.snippet_window_start, end: selected.snippet_window_end, length: selected.snippet_source_len }) }}</p>
              </div>

              <div class="mt-4 rounded-lg border border-default bg-surface p-4">
                <div class="docmind-section-label">{{ t("searchResultCard.rankReason") }}</div>
                <div class="mt-1 docmind-metric-value text-primary">{{ selected.rank_reason.summary || t("common.none") }}</div>
                <div v-if="selected.rank_reason.boosts.length > 0" class="docmind-item-meta mt-1">
                  {{ selected.rank_reason.boosts.join(" · ") }}
                </div>
              </div>

              <div v-if="actionMessage" class="mt-3 rounded-md border border-emerald-soft bg-emerald-soft px-3 py-2 text-xs text-success">
                {{ actionMessage }}
              </div>
              <div v-if="actionErrorMessage" class="mt-3 rounded-md border border-danger-soft bg-danger-soft px-3 py-2 text-xs text-danger">
                {{ actionErrorMessage }}
              </div>
            </div>

            <div v-else-if="qaMode && qaSelectedSource" class="docmind-detail">
              <div class="mb-4 rounded-lg border border-default bg-panel p-4">
                <div class="flex items-start justify-between gap-3">
                  <div class="min-w-0">
                    <div class="text-lg font-semibold text-primary">{{ qaSelectedSource.file_name }}</div>
                    <div class="mt-1 break-all text-xs text-muted">{{ qaSelectedSource.path }}</div>
                  </div>
                  <div class="docmind-file-icon flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-surface text-[11px] font-semibold text-secondary">
                    {{ qaSelectedSource.ext.toUpperCase() }}
                  </div>
                </div>
                <div v-if="qaSelectedTitlePath" class="mt-3 rounded-md border border-default bg-surface px-3 py-2">
                  <div class="text-[11px] font-semibold uppercase tracking-[0.16em] text-dim">
                    {{ t("page.appSearch.detail.titlePath") }}
                  </div>
                  <div class="mt-1 text-sm leading-6 text-primary">
                    {{ qaSelectedTitlePath }}
                  </div>
                </div>
              </div>

              <div class="mb-4 flex flex-wrap gap-2">
                <DocMindBadge>{{ qaSelectedSource.ext.toUpperCase() }}</DocMindBadge>
                <DocMindBadge>{{ qaSelectedSource.page ? t("searchResultCard.page", { page: qaSelectedSource.page }) : t("searchResultCard.paragraph", { para: qaSelectedSource.paragraph }) }}</DocMindBadge>
                <DocMindBadge v-if="qaSelectedSource.page" tone="default">{{ t("page.appSearch.detail.pdfPage", { page: qaSelectedSource.page }) }}</DocMindBadge>
                <DocMindBadge tone="success">{{ t("page.appSearch.qa.sourceId", { id: qaSelectedSource.source_id }) }}</DocMindBadge>
              </div>

              <div class="mb-4 rounded-lg border border-default bg-surface p-4">
                <div class="mb-2 text-sm font-medium text-secondary">{{ t("page.appSearch.qa.sourceSnippet") }}</div>
                <div class="whitespace-pre-wrap text-sm leading-7 text-primary">
                  {{ qaSelectedSource.snippet }}
                </div>
                <p class="docmind-item-meta mt-3">{{ qaSelectedSource.rank_reason }}</p>
              </div>

              <div class="mt-4 rounded-lg border border-default bg-surface p-4">
                <div class="docmind-section-label">{{ t("page.appSearch.qa.sourceMeta") }}</div>
                <div class="mt-1 docmind-metric-value text-primary">{{ qaSelectedCitation || t("common.none") }}</div>
                <div class="mt-1 docmind-item-meta">
                  {{ qaAnswer?.retrieval.search_mode || t("common.none") }} · {{ qaAnswer?.retrieval.selected_count ?? 0 }}/{{ qaAnswer?.retrieval.candidate_count ?? 0 }}
                </div>
              </div>

              <div class="mt-4 flex flex-wrap gap-2">
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
            </div>

            <div v-else class="rounded-lg border border-dashed border-default bg-surface p-6 text-sm text-muted">
              {{ qaMode ? t("page.appSearch.qa.noSourceSelected") : t("page.appSearch.noResults") }}
            </div>
          </aside>
        </template>
      </SplitPane>

      <DocMindContextMenu
        v-if="resultMenuVisible"
        :items="resultContextMenuItems"
        :x="resultMenuPosition.x"
        :y="resultMenuPosition.y"
        @close="resultMenuVisible = false"
      />
    </main>
  </div>
</template>
