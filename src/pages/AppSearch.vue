<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { listen } from "@tauri-apps/api/event";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import { Clock, Copy, Eye, ExternalLink, FileText, Filter, FolderOpen, History, Search, Star } from "lucide-vue-next";
import DocMindBadge from "../components/docmind/DocMindBadge.vue";
import DocMindContextMenu from "../components/docmind/DocMindContextMenu.vue";
import type { ContextMenuItem } from "../components/docmind/DocMindContextMenu.vue";
import DocMindIndexTree from "../components/docmind/DocMindIndexTree.vue";
import DocMindHighlightedText from "../components/docmind/DocMindHighlightedText.vue";
import DocMindMarkdownRenderer from "../components/docmind/DocMindMarkdownRenderer.vue";
import DocMindSearchResultGroupCard from "../components/docmind/DocMindSearchResultGroupCard.vue";
import { useIndexDirTree } from "../composables/useIndexDirTree";
import type { VisibleIndexDirRow } from "../composables/useIndexDirTree";
import SplitPane from "../components/SplitPane.vue";
import { docmindApi } from "../services/docmindApi";
import { buildDocumentLocationParts, formatDocumentCitation, resolveDocumentTitlePath } from "../utils/citation";

const { t } = useI18n();
import type {
  ChunkView,
  FavoriteView,
  IndexDirView,
  IndexSettingsView,
  IndexStatusView,
  ParserRuntimeView,
  RecentDocumentView,
  SearchDebugReportEventView,
  SearchDebugView,
  SearchHistoryView,
  SearchResultView,
} from "../types/docmind";

const router = useRouter();
const query = ref("");
const searchInputRef = ref<HTMLInputElement | null>(null);
const selectedId = ref<string>("");
const results = ref<SearchResultView[]>([]);
const debugReport = ref<SearchDebugView | null>(null);
const showDebugPanel = ref(false);
const debugReportLoading = ref(false);
const debugReportError = ref("");
const activeDebugRequestId = ref("");
const status = ref<IndexStatusView | null>(null);
const parserRuntime = ref<ParserRuntimeView | null>(null);
const indexSettings = ref<IndexSettingsView | null>(null);
const quickDirs = ref<IndexDirView[]>([]);
const searchHistory = ref<SearchHistoryView[]>([]);
const recentDocuments = ref<RecentDocumentView[]>([]);
const favorites = ref<FavoriteView[]>([]);
const selectedChunkCount = ref<number | null>(null);
const selectedDocumentChunks = ref<ChunkView[]>([]);
const selectedChunkIndex = ref<number>(-1);
const actionMessage = ref("");
const actionErrorMessage = ref("");
const loading = ref(false);
const errorMessage = ref("");
const expandedGroups = ref<Record<string, boolean>>({});
let selectedContextRequestId = 0;
let unlistenSearchDebugReport: null | (() => void) = null;

const {
  visibleRows: visibleQuickDirRows,
  setExpanded: setQuickDirExpanded,
} = useIndexDirTree(quickDirs);

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

const selectedTitlePath = computed(() =>
  resolveDocumentTitlePath({
    fileName: selected.value?.fileName,
    titlePath: selected.value?.title_path,
    heading: selected.value?.heading,
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
    { key: "left", initialSize: 240, minSize: 160 },
    { key: "center", minSize: 300, flex: true },
  ];
  if (selected.value) {
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

const loadStatus = async () => {
  status.value = await docmindApi.getIndexStatus();
};

const loadParserRuntime = async () => {
  parserRuntime.value = await docmindApi.getParserRuntime();
};

const loadIndexSettings = async () => {
  indexSettings.value = await docmindApi.getIndexSettings();
};

const loadQuickPanels = async () => {
  const [dirs, history, recent, favoriteList] = await Promise.all([
    docmindApi.listIndexDirs(),
    docmindApi.listSearchHistory(10),
    docmindApi.listRecentDocuments(8),
    docmindApi.listFavorites(12),
  ]);

  quickDirs.value = dirs.filter((dir) => dir.enabled);
  searchHistory.value = history;
  recentDocuments.value = recent;
  favorites.value = favoriteList;
};

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
    await loadQuickPanels();
    await loadIndexSettings();
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
    await loadQuickPanels();
    await loadIndexSettings();
  } finally {
    loading.value = false;
  }
};

const openSelectedFile = async () => {
  if (!selected.value) return;
  await docmindApi.openFile(selected.value.path);
  await loadQuickPanels();
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

const runQueryFromHistory = async (item: SearchHistoryView) => {
  query.value = item.query;
  await runSearch();
};

const openRecentDocument = async (item: RecentDocumentView) => {
  await docmindApi.openFile(item.path);
  await loadQuickPanels();
};

const openFavoriteDocument = async (path: string) => {
  await docmindApi.openFile(path);
  await loadQuickPanels();
};

const toggleFavoriteResult = async (item: SearchResultView) => {
  await docmindApi.toggleResultFavorite(
    item.path,
    item.heading,
    item.paragraph ?? null,
    item.page ?? null,
    item.fileName,
  );
  await loadQuickPanels();
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

const openLibrary = async () => {
  await router.push({ path: "/status" });
};

const contextMenuRow = ref<VisibleIndexDirRow | null>(null);
const contextMenuPosition = ref({ x: 0, y: 0 });
const contextMenuVisible = ref(false);

const contextMenuItems = computed<ContextMenuItem[]>(() => {
  const row = contextMenuRow.value;
  if (!row) return [];
  return [
    {
      key: "openLibrary",
      label: t("common.open"),
      icon: FolderOpen,
      handler: () => openLibrary(),
    },
  ];
});

const handleTreeContextMenu = (row: VisibleIndexDirRow, event: MouseEvent) => {
  contextMenuRow.value = row;
  contextMenuPosition.value = { x: event.clientX, y: event.clientY };
  contextMenuVisible.value = true;
};

onMounted(async () => {
  window.addEventListener("keydown", handleGlobalShortcut);
  await installSearchDebugReportListener();
  await Promise.all([loadStatus(), loadParserRuntime(), loadIndexSettings(), loadQuickPanels()]);
  if (query.value.trim()) {
    await runSearch();
  } else {
    results.value = [];
    selectedId.value = "";
    expandedGroups.value = {};
    clearSearchDebugReport();
  }
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", handleGlobalShortcut);
  unlistenSearchDebugReport?.();
  unlistenSearchDebugReport = null;
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
</script>

<template>
  <div class="flex h-full min-h-0 flex-col bg-page text-primary">
    <header class="flex h-12 items-center justify-between gap-4 border-b border-default bg-header px-5">
      <div class="min-w-0 flex-1">
        <form
          class="flex h-8 max-w-[640px] items-center gap-2 rounded-md border border-default bg-input px-3 transition focus-within:border-accent focus-within:ring-2 focus-within:ring-accent-soft"
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
      </div>
      <div class="hidden shrink-0 items-center gap-2 text-xs lg:flex">
        <DocMindBadge tone="success">
          SQLite: {{ status?.indexed_docs ?? 0 }}/{{ status?.scanned_docs ?? 0 }}
        </DocMindBadge>
        <DocMindBadge tone="default">
          Tantivy: {{ status?.indexed_chunks ?? 0 }}
        </DocMindBadge>
        <DocMindBadge tone="default">
          {{ t("page.appSearch.semanticWeight", { weight: Math.round((indexSettings?.semantic_weight ?? 0.25) * 100) }) }}
        </DocMindBadge>
      </div>
    </header>

    <main class="flex min-h-0 flex-1 overflow-hidden">
      <SplitPane :panels="splitPanels">
        <template #left>
          <aside class="min-h-0 flex-1 overflow-y-auto bg-panel p-4">
            <div class="space-y-7">
              <section>
                <div class="mb-3 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-dim">
                  <History :size="14" />
                  {{ t("page.appSearch.section.recentSearch") }}
                </div>
                <div v-if="searchHistory.length === 0" class="rounded-md border border-dashed border-default bg-surface px-3 py-3 text-xs text-muted">
                  {{ t("page.appSearch.section.noHistory") }}
                </div>
                <div v-else class="flex flex-wrap gap-2">
                  <button
                    v-for="item in searchHistory"
                    :key="item.query"
                    class="rounded-md bg-surface-hover px-2 py-0.5 text-[11px] text-secondary transition hover:bg-badge"
                    @click="runQueryFromHistory(item)"
                  >
                    {{ item.query }}
                  </button>
                </div>
              </section>

              <section>
                <div class="mb-3 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-dim">
                  <FileText :size="14" />
                  {{ t("page.appSearch.section.recentOpen") }}
                </div>
                <div v-if="recentDocuments.length === 0" class="rounded-md border border-dashed border-default bg-surface px-3 py-3 text-xs text-muted">
                  {{ t("page.appSearch.section.noRecent") }}
                </div>
                <div v-else class="space-y-2">
                  <button
                    v-for="item in recentDocuments"
                    :key="item.path"
                    class="block w-full rounded-md px-2 py-1.5 text-left text-xs text-secondary transition hover:bg-panel"
                    @click="openRecentDocument(item)"
                  >
                    <div class="truncate font-medium text-primary">{{ item.title }}</div>
                    <div class="mt-1 truncate text-[11px] text-muted">{{ item.path }}</div>
                  </button>
                </div>
              </section>

              <section>
                <div class="mb-3 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-dim">
                  <Star :size="14" />
                  {{ t("page.appSearch.section.favorites") }}
                </div>
                <div v-if="favoriteResults.length === 0" class="rounded-md border border-dashed border-default bg-surface px-3 py-3 text-xs text-muted">
                  {{ t("page.appSearch.section.noFavorites") }}
                </div>
                <div v-else class="space-y-2">
                  <button
                    v-for="item in favoriteResults"
                    :key="item.target"
                    class="block w-full rounded-md px-2 py-1.5 text-left text-xs text-secondary transition hover:bg-panel"
                    @click="openFavoriteDocument(item.path)"
                  >
                    <div class="truncate font-medium text-primary">{{ item.title }}</div>
                    <div class="mt-1 truncate text-[11px] text-muted">{{ item.path }}</div>
                  </button>
                </div>
              </section>

              <section>
                <div class="mb-3 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-dim">
                  <FolderOpen :size="14" />
                  {{ t("page.appSearch.section.quickDirs") }}
                </div>
                <div v-if="quickDirs.length === 0" class="rounded-md border border-dashed border-default bg-surface px-3 py-3 text-xs text-muted">
                  {{ t("page.appSearch.section.noDirs") }}
                </div>
                <DocMindIndexTree
                  v-else
                  :rows="visibleQuickDirRows"
                  :selectable="false"
                  :path-tooltip="true"
                  :virtual-label="t('common.virtualDir')"
                  :expand-title="t('sidebar.expand')"
                  :collapse-title="t('sidebar.collapse')"
                  :node-padding-base="0"
                  :node-padding-step="12"
                  density="compact"
                  @contextmenu="handleTreeContextMenu"
                  @toggle="setQuickDirExpanded"
                />
                <DocMindContextMenu
                  v-if="contextMenuVisible"
                  :items="contextMenuItems"
                  :x="contextMenuPosition.x"
                  :y="contextMenuPosition.y"
                  @close="contextMenuVisible = false"
                />
              </section>
            </div>
          </aside>
        </template>

        <template #center>
          <section class="min-h-0 flex-1 overflow-y-auto bg-panel/70">
            <div class="flex items-center justify-between gap-3 border-b border-default bg-surface px-4 py-2">
              <div class="text-xs font-medium text-dim">
                {{ t("page.appSearch.stats.foundDocs", { count: groupedResults.length, total: results.length }) }}
              </div>
              <button class="flex items-center gap-1 rounded-md px-2 py-1 text-xs text-accent-text hover:bg-accent-soft">
                <Filter :size="14" />
                {{ t("page.appSearch.filter") }}
              </button>
            </div>

            <div v-if="showDebugPanel" class="border-b border-default bg-panel px-4 py-3 text-xs text-secondary">
              <div class="flex items-center justify-between gap-3">
                <div>
                  <div class="text-[10px] font-semibold uppercase tracking-[0.16em] text-dim">{{ t("page.appSearch.debug.title") }}</div>
                  <div class="mt-1 text-xs text-dim">{{ t("page.appSearch.debug.desc") }}</div>
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
                    <div class="text-[10px] uppercase tracking-[0.16em] text-dim">{{ t("page.appSearch.debug.hits") }}</div>
                    <div class="mt-1 text-sm font-medium text-primary">{{ debugReport.hit_count }}</div>
                  </div>
                  <div class="rounded-md border border-default bg-surface px-3 py-2">
                    <div class="text-[10px] uppercase tracking-[0.16em] text-dim">{{ t("page.appSearch.debug.keywordHits") }}</div>
                    <div class="mt-1 text-sm font-medium text-primary">{{ debugReport.keyword_hit_count }}</div>
                  </div>
                  <div class="rounded-md border border-default bg-surface px-3 py-2">
                    <div class="text-[10px] uppercase tracking-[0.16em] text-dim">{{ t("page.appSearch.debug.semanticHits") }}</div>
                    <div class="mt-1 text-sm font-medium text-primary">{{ debugReport.semantic_hit_count }}</div>
                  </div>
                  <div class="rounded-md border border-default bg-surface px-3 py-2">
                    <div class="text-[10px] uppercase tracking-[0.16em] text-dim">{{ t("page.appSearch.debug.mode") }}</div>
                    <div class="mt-1 text-sm font-medium text-primary">{{ debugReport.search_mode }}</div>
                  </div>
                </div>
                <div class="rounded-md border border-default bg-surface px-3 py-2 text-xs text-secondary">
                  <div>{{ t("page.appSearch.debug.normalized", { query: debugReport.normalized_search_text || t("common.none") }) }}</div>
                  <div class="mt-1 break-all">{{ t("page.appSearch.debug.rewritten", { query: debugReport.rewritten_query || t("common.none") }) }}</div>
                  <div class="mt-1 break-all">{{ t("page.appSearch.debug.expanded", { query: debugReport.expanded_query || t("common.none") }) }}</div>
                </div>
              </div>
            </div>

            <div v-if="errorMessage" class="m-4 rounded-md border border-danger-soft bg-danger-soft px-4 py-3 text-sm text-danger">
              {{ errorMessage }}
            </div>

            <div v-if="!results.length && !loading" class="m-4 rounded-md border border-dashed border-default bg-surface px-4 py-6 text-center text-xs text-muted">
              {{ t("page.appSearch.noResults") }}
            </div>

            <div class="space-y-3 p-4">
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
              />
            </div>
          </section>
        </template>

        <template #right>
          <aside class="min-h-0 flex-1 overflow-y-auto bg-surface p-5">
            <div class="docmind-detail">
              <div class="mb-4 rounded-lg border border-default bg-panel p-4">
                <div class="flex items-start justify-between gap-3">
                  <div class="min-w-0">
                    <div class="text-lg font-semibold text-primary">{{ selected.fileName }}</div>
                    <div class="mt-1 break-all text-xs text-muted">{{ selected.path }}</div>
                  </div>
                  <div class="docmind-file-icon flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-surface text-[10px] font-semibold text-secondary">
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
                    <div class="text-[11px] font-semibold uppercase tracking-[0.16em]" :class="item.key === 'current' ? 'text-accent-text' : 'text-dim'">
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
                    <div class="mt-1 text-[11px] text-muted">
                      {{ item.chunk?.page ? t("page.appSearch.detail.pdfPage", { page: item.chunk.page }) : t("searchResultCard.paragraph", { para: item.chunk?.paragraph ?? "-" }) }}
                    </div>
                  </div>
                </div>
                <div v-else class="rounded-md border border-dashed border-default bg-surface px-3 py-3 text-xs text-muted">
                  {{ t("page.appSearch.detail.noContext") }}
                </div>
                <p class="mt-3 text-xs text-muted">{{ t("page.appSearch.detail.snippetSource", { start: selected.snippet_window_start, end: selected.snippet_window_end, length: selected.snippet_source_len }) }}</p>
              </div>

              <div class="mt-4 rounded-lg border border-default bg-surface p-4">
                <div class="text-[11px] uppercase tracking-wide text-dim">{{ t("searchResultCard.rankReason") }}</div>
                <div class="mt-1 text-sm font-medium text-primary">{{ selected.rank_reason.summary || t("common.none") }}</div>
                <div v-if="selected.rank_reason.boosts.length > 0" class="mt-1 text-xs text-dim">
                  {{ selected.rank_reason.boosts.join(" · ") }}
                </div>
              </div>

              <div class="mt-4 grid grid-cols-2 gap-3">
                <button class="flex items-center justify-center gap-2 rounded-lg bg-accent px-4 py-3 text-sm font-medium text-white" @click="openSelectedFile">
                  <ExternalLink :size="16" />
                  {{ t("common.openFile") }}
                </button>
                <button class="flex items-center justify-center gap-2 rounded-lg border border-default bg-surface px-4 py-3 text-sm font-medium text-secondary" @click="viewChunks">
                  <FileText :size="16" />
                  {{ t("common.viewChunks") }}
                </button>
              </div>
              <div class="mt-3 grid grid-cols-2 gap-2">
                <button class="flex items-center justify-center gap-2 rounded-md border border-default bg-surface px-3 py-2 text-xs font-medium text-secondary hover:bg-panel" @click="quickLookSelectedFile">
                  <Eye :size="14" />
                  {{ t("page.appSearch.detail.quickLook") }}
                </button>
                <button class="flex items-center justify-center gap-2 rounded-md border border-default bg-surface px-3 py-2 text-xs font-medium text-secondary hover:bg-panel" @click="copySelectedPath">
                  <Copy :size="14" />
                  {{ t("page.appSearch.detail.copyPath") }}
                </button>
                <button class="flex items-center justify-center gap-2 rounded-md border border-default bg-surface px-3 py-2 text-xs font-medium text-secondary hover:bg-panel" @click="copySelectedTitlePath">
                  <Copy :size="14" />
                  {{ t("page.appSearch.detail.copyTitlePath") }}
                </button>
                <button class="flex items-center justify-center gap-2 rounded-md border border-default bg-surface px-3 py-2 text-xs font-medium text-secondary hover:bg-panel" @click="copySelectedCitation">
                  <FileText :size="14" />
                  {{ t("page.appSearch.detail.copyCitation") }}
                </button>
              </div>
              <div v-if="actionMessage" class="mt-3 rounded-md border border-emerald-soft bg-emerald-soft px-3 py-2 text-xs text-success">
                {{ actionMessage }}
              </div>
              <div v-if="actionErrorMessage" class="mt-3 rounded-md border border-danger-soft bg-danger-soft px-3 py-2 text-xs text-danger">
                {{ actionErrorMessage }}
              </div>
            </div>
          </aside>
        </template>
      </SplitPane>
    </main>
  </div>
</template>
