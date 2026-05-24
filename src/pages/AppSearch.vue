<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import { CheckCircle2, Clock, Cpu, ExternalLink, FileText, Filter, FolderOpen, History, Search, Sparkles, Star } from "lucide-vue-next";
import DocMindBadge from "../components/docmind/DocMindBadge.vue";
import DocMindHighlightedText from "../components/docmind/DocMindHighlightedText.vue";
import DocMindSearchResultGroupCard from "../components/docmind/DocMindSearchResultGroupCard.vue";
import { docmindApi } from "../services/docmindApi";

const { t } = useI18n();
import type {
  FavoriteView,
  IndexDirView,
  IndexSettingsView,
  IndexStatusView,
  ParserRuntimeView,
  RecentDocumentView,
  SearchDebugView,
  SearchHistoryView,
  SearchResultView,
} from "../types/docmind";

const router = useRouter();
const query = ref("");
const selectedId = ref<string>("");
const results = ref<SearchResultView[]>([]);
const debugReport = ref<SearchDebugView | null>(null);
const status = ref<IndexStatusView | null>(null);
const parserRuntime = ref<ParserRuntimeView | null>(null);
const indexSettings = ref<IndexSettingsView | null>(null);
const quickDirs = ref<IndexDirView[]>([]);
const searchHistory = ref<SearchHistoryView[]>([]);
const recentDocuments = ref<RecentDocumentView[]>([]);
const favorites = ref<FavoriteView[]>([]);
const selectedChunkCount = ref<number | null>(null);
const loading = ref(false);
const errorMessage = ref("");
const expandedGroups = ref<Record<string, boolean>>({});

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
  () => results.value.find((item) => item.id === selectedId.value) ?? results.value[0] ?? null,
);

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

const selectResult = (id: string) => {
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

const loadSelectedChunkCount = async (path: string | undefined) => {
  if (!path) {
    selectedChunkCount.value = null;
    return;
  }

  try {
    const chunks = await docmindApi.listDocumentChunks(path);
    selectedChunkCount.value = chunks.length;
  } catch (error) {
    selectedChunkCount.value = null;
    console.error("[DocMind] loadSelectedChunkCount failed", error);
  }
};

const runSearch = async () => {
  loading.value = true;
  errorMessage.value = "";

  try {
    const items = await docmindApi.searchDocuments(query.value, 20);
    results.value = items;
    selectedId.value = items[0]?.id ?? "";
    expandedGroups.value = {};
    debugReport.value = await docmindApi.getSearchDebugReport(query.value, 20);
    await loadQuickPanels();
    await loadIndexSettings();
  } catch (error) {
    results.value = [];
    selectedId.value = "";
    expandedGroups.value = {};
    errorMessage.value = error instanceof Error ? error.message : t("page.appSearch.searchFailed");
    debugReport.value = await docmindApi.getSearchDebugReport(query.value, 20).catch(() => null);
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
  await router.push({ path: "/library" });
};

onMounted(async () => {
  await Promise.all([loadStatus(), loadParserRuntime(), loadIndexSettings(), loadQuickPanels(), runSearch()]);
});

watch(query, () => {
  if (!query.value.trim()) {
    results.value = [];
    selectedId.value = "";
  }
});

watch(
  () => selected.value?.path,
  async (path) => {
    await loadSelectedChunkCount(path);
  },
  { immediate: true },
);
</script>

<template>
  <div class="flex h-full flex-col">
    <header class="border-b border-slate-200 bg-white/70 px-8 py-5 backdrop-blur-xl">
        <div class="mb-4 flex items-center justify-between">
          <div>
            <h1 class="text-2xl font-semibold tracking-tight text-slate-950">{{ t("page.appSearch.title") }}</h1>
            <p class="mt-1 text-sm text-slate-500">{{ t("page.appSearch.subtitle") }}</p>
          </div>
          <div class="flex flex-wrap items-center gap-2">
            <DocMindBadge tone="success">
              <CheckCircle2 class="mr-1" :size="13" />
               {{ t("page.appSearch.indexedDocs", { count: status?.indexed_docs ?? 0 }) }}
            </DocMindBadge>
            <DocMindBadge :tone="parserRuntime?.active === 'python' ? 'success' : 'warning'">
              <Cpu class="mr-1" :size="13" />
              {{ parserRuntime?.active === 'python' ? t("page.appSearch.parserPython") : t("page.appSearch.parserRust") }}
            </DocMindBadge>
            <DocMindBadge :tone="indexSettings?.semantic_search_enabled ? 'success' : 'default'">
              <Sparkles class="mr-1" :size="13" />
              {{ indexSettings?.semantic_search_enabled ? t("page.appSearch.semanticHybrid") : t("page.appSearch.semanticFullText") }}
            </DocMindBadge>
            <DocMindBadge tone="default">
              {{ t("page.appSearch.semanticWeight", { weight: Math.round((indexSettings?.semantic_weight ?? 0.25) * 100) }) }}
            </DocMindBadge>
            <DocMindBadge tone="default">
              {{ t("page.appSearch.semanticThreshold", { threshold: Math.round((indexSettings?.semantic_threshold ?? 0.2) * 100) }) }}
            </DocMindBadge>
          </div>
        </div>

      <form class="flex items-center gap-3 rounded-3xl border border-slate-200 bg-white px-4 py-3 shadow-sm" @submit.prevent="runSearch">
        <Search :size="20" class="text-slate-400" />
        <input
          v-model="query"
          :placeholder="t('page.appSearch.placeholder')"
          class="flex-1 bg-transparent text-base outline-none placeholder:text-slate-400"
        />
        <button class="rounded-2xl bg-slate-900 px-4 py-2 text-sm font-medium text-white" :disabled="loading">
          {{ loading ? t("page.appSearch.searching") : t("page.appSearch.search") }}
        </button>
      </form>

      <div v-if="debugReport" class="mt-4 grid gap-3 md:grid-cols-2 xl:grid-cols-5">
        <div class="rounded-2xl bg-white px-4 py-3 shadow-sm ring-1 ring-slate-200">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">{{ t("page.appSearch.stats.documents") }}</div>
          <div class="mt-1 text-sm font-semibold text-slate-900">
            {{ debugReport.sqlite_documents }} / {{ debugReport.sqlite_chunks }}
          </div>
        </div>
        <div class="rounded-2xl bg-white px-4 py-3 shadow-sm ring-1 ring-slate-200">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">{{ t("page.appSearch.stats.paragraphs") }}</div>
          <div class="mt-1 text-sm font-semibold text-slate-900">{{ debugReport.tantivy_documents }}</div>
        </div>
        <div class="rounded-2xl bg-white px-4 py-3 shadow-sm ring-1 ring-slate-200">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">{{ t("page.appSearch.stats.normalizedQueryLabel") }}</div>
          <div class="mt-1 break-words text-sm font-medium text-slate-900">
            {{ debugReport.normalized_terms.join(" · ") || t("common.none") }}
          </div>
        </div>
        <div class="rounded-2xl bg-white px-4 py-3 shadow-sm ring-1 ring-slate-200">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">{{ t("page.appSearch.stats.hitCount") }}</div>
          <div class="mt-1 text-sm font-semibold text-slate-900">{{ debugReport.hit_count }}</div>
        </div>
        <div class="rounded-2xl bg-white px-4 py-3 shadow-sm ring-1 ring-slate-200">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">{{ t("page.appSearch.stats.semanticBreakdown") }}</div>
          <div class="mt-1 text-sm font-semibold text-slate-900">
            {{ debugReport.keyword_hit_count }} / {{ debugReport.semantic_hit_count }}
          </div>
          <div class="mt-1 text-xs text-slate-500">
            {{ debugReport.search_mode === 'hybrid' ? t("page.appSearch.semanticHybrid") : t("page.appSearch.semanticFullText") }}
            <span v-if="!debugReport.semantic_enabled">· {{ t("page.appSearch.semanticDisabled") }}</span>
          </div>
        </div>
        <div class="rounded-2xl bg-white px-4 py-3 shadow-sm ring-1 ring-slate-200">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">{{ t("page.appSearch.queryRewrite") }}</div>
          <div class="mt-1 break-words text-sm font-semibold text-slate-900">
            {{ debugReport.rewritten_query || t("common.none") }}
          </div>
          <div v-if="debugReport.rewritten_terms.length > 0" class="mt-1 break-words text-xs text-slate-500">
            {{ debugReport.rewritten_terms.join(" · ") }}
          </div>
          <div class="mt-1 text-xs text-slate-500">
            {{ debugReport.query_rewrite_applied ? t("page.appSearch.queryRewriteApplied") : t("page.appSearch.queryRewriteDisabled") }}
          </div>
        </div>
        <div class="rounded-2xl bg-white px-4 py-3 shadow-sm ring-1 ring-slate-200">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">{{ t("page.appSearch.semanticThreshold") }}</div>
          <div class="mt-1 text-sm font-semibold text-slate-900">
            {{ Math.round((debugReport.semantic_threshold ?? 0.2) * 100) }}%
          </div>
          <div class="mt-1 text-xs text-slate-500">
            {{ t("page.appSearch.semanticCandidates", { count: debugReport.semantic_candidate_count }) }}
          </div>
          <div class="mt-1 text-xs text-slate-500">
            {{ t("page.appSearch.semanticFiltered", { count: debugReport.semantic_filtered_count }) }}
          </div>
        </div>
        <div v-if="selected" class="rounded-2xl bg-white px-4 py-3 shadow-sm ring-1 ring-slate-200">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">{{ t("page.appSearch.stats.trim") }}</div>
          <div class="mt-1 text-sm font-semibold text-slate-900">
            {{ selected.snippet_window_start }} - {{ selected.snippet_window_end }} / {{ selected.snippet_source_len }}
          </div>
          <div class="mt-1 text-xs text-slate-500">{{ selected.match_origin }}</div>
        </div>
      </div>

      <div class="mt-4 grid gap-3 xl:grid-cols-4">
        <section class="rounded-2xl bg-white px-4 py-3 shadow-sm ring-1 ring-slate-200">
          <div class="mb-2 flex items-center gap-2 text-[11px] uppercase tracking-wide text-slate-500">
            <History :size="13" />
            {{ t("page.appSearch.section.recentSearch") }}
          </div>
          <div v-if="searchHistory.length === 0" class="text-xs text-slate-400">{{ t("page.appSearch.section.noHistory") }}</div>
          <div v-else class="flex flex-wrap gap-2">
            <button
              v-for="item in searchHistory"
              :key="item.query"
              class="rounded-full border border-slate-200 bg-slate-50 px-3 py-1 text-xs text-slate-700 hover:bg-slate-100"
              @click="runQueryFromHistory(item)"
            >
              {{ item.query }}
            </button>
          </div>
        </section>

        <section class="rounded-2xl bg-white px-4 py-3 shadow-sm ring-1 ring-slate-200">
          <div class="mb-2 flex items-center gap-2 text-[11px] uppercase tracking-wide text-slate-500">
            <FileText :size="13" />
            {{ t("page.appSearch.section.recentOpen") }}
          </div>
          <div v-if="recentDocuments.length === 0" class="text-xs text-slate-400">{{ t("page.appSearch.section.noRecent") }}</div>
          <div v-else class="space-y-2">
            <button
              v-for="item in recentDocuments"
              :key="item.path"
              class="block w-full rounded-2xl border border-slate-200 bg-slate-50 px-3 py-2 text-left text-xs text-slate-700 hover:bg-slate-100"
              @click="openRecentDocument(item)"
            >
              <div class="truncate font-medium text-slate-900">{{ item.title }}</div>
              <div class="mt-1 truncate text-[11px] text-slate-400">{{ item.path }}</div>
            </button>
          </div>
        </section>

        <section class="rounded-2xl bg-white px-4 py-3 shadow-sm ring-1 ring-slate-200">
          <div class="mb-2 flex items-center gap-2 text-[11px] uppercase tracking-wide text-slate-500">
            <Star :size="13" />
            {{ t("page.appSearch.section.favorites") }}
          </div>
          <div v-if="favoriteResults.length === 0" class="text-xs text-slate-400">
            {{ t("page.appSearch.section.noFavorites") }}
          </div>
          <div v-else class="space-y-2">
            <button
              v-for="item in favoriteResults"
              :key="item.target"
              class="block w-full rounded-2xl border border-slate-200 bg-slate-50 px-3 py-2 text-left text-xs text-slate-700 hover:bg-slate-100"
              @click="openFavoriteDocument(item.path)"
            >
              <div class="truncate font-medium text-slate-900">{{ item.title }}</div>
              <div class="mt-1 truncate text-[11px] text-slate-400">{{ item.path }}</div>
            </button>
          </div>
        </section>

        <section class="rounded-2xl bg-white px-4 py-3 shadow-sm ring-1 ring-slate-200">
          <div class="mb-2 flex items-center gap-2 text-[11px] uppercase tracking-wide text-slate-500">
            <FolderOpen :size="13" />
            {{ t("page.appSearch.section.quickDirs") }}
          </div>
          <div v-if="quickDirs.length === 0" class="text-xs text-slate-400">{{ t("page.appSearch.section.noDirs") }}</div>
          <div v-else class="space-y-2">
            <button
              v-for="dir in quickDirs"
              :key="dir.path"
              class="block w-full rounded-2xl border border-slate-200 bg-slate-50 px-3 py-2 text-left text-xs text-slate-700 hover:bg-slate-100"
              @click="openLibrary"
            >
              <div class="truncate font-medium text-slate-900">{{ dir.path }}</div>
              <div class="mt-1 text-[11px] text-slate-400">{{ t("page.appSearch.section.dirStats", { docs: dir.docs, chunks: dir.chunks }) }}</div>
            </button>
          </div>
        </section>
      </div>
    </header>

    <main class="grid min-h-0 flex-1 grid-cols-[minmax(420px,0.95fr)_minmax(360px,0.8fr)] gap-0">
      <section class="min-h-0 overflow-y-auto border-r border-slate-200 bg-slate-50/50 p-5">
        <div class="mb-4 flex items-center justify-between">
          <div class="text-sm text-slate-500">
            {{ t("page.appSearch.stats.foundDocs", { count: groupedResults.length, total: results.length }) }}
          </div>
          <button class="flex items-center gap-1 rounded-xl border border-slate-200 bg-white px-3 py-1.5 text-xs text-slate-600">
            <Filter :size="14" />
            {{ t("page.appSearch.filter") }}
          </button>
        </div>

        <div v-if="errorMessage" class="mb-4 rounded-2xl border border-red-100 bg-red-50 px-4 py-3 text-sm text-red-700">
          {{ errorMessage }}
        </div>

        <div v-if="!results.length && !loading" class="rounded-3xl border border-dashed border-slate-300 bg-white px-5 py-10 text-center text-sm text-slate-500">
          {{ t("page.appSearch.noResults") }}
        </div>

        <div class="space-y-3">
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

      <aside class="min-h-0 overflow-y-auto bg-white p-6">
        <div v-if="selected" class="docmind-detail">
          <div class="mb-5 flex items-start justify-between gap-3">
            <div>
              <div class="text-lg font-semibold text-slate-950">{{ selected.fileName }}</div>
              <div class="mt-1 break-all text-xs text-slate-400">{{ selected.path }}</div>
            </div>
            <div class="docmind-file-icon flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-slate-100 text-[10px] font-semibold text-slate-600">
              {{ selected.ext.toUpperCase() }}
            </div>
          </div>

          <div class="mb-4 flex flex-wrap gap-2">
            <DocMindBadge>{{ selected.ext.toUpperCase() }}</DocMindBadge>
            <DocMindBadge>{{ selected.page ? t("searchResultCard.page", { page: selected.page }) : t("searchResultCard.paragraph", { para: selected.paragraph }) }}</DocMindBadge>
            <DocMindBadge tone="success">{{ t("searchResultCard.matchField", { field: matchedFieldLabel }) }}</DocMindBadge>
            <DocMindBadge tone="default">{{ selected.snippet_window_start }}-{{ selected.snippet_window_end }} / {{ selected.snippet_source_len }}</DocMindBadge>
            <DocMindBadge tone="default">{{ t("page.appSearch.detail.chunkCount", { count: selectedChunkCount ?? "..." }) }}</DocMindBadge>
            <DocMindBadge tone="default"><Clock class="mr-1 inline" :size="12" />{{ selected.modified }}</DocMindBadge>
          </div>

          <div class="rounded-3xl border border-slate-200 bg-slate-50 p-5">
            <div class="mb-2 text-sm font-medium text-slate-700">{{ t("page.appSearch.detail.hitParagraph") }}</div>
            <p class="text-sm leading-7 text-slate-700">
              <DocMindHighlightedText :text="selected.snippet" :query="query" :spans="selected.highlight_spans" />
            </p>
          </div>

          <div class="mt-5 rounded-3xl border border-slate-200 bg-white p-5">
            <div class="mb-2 text-sm font-medium text-slate-700">{{ t("page.appSearch.detail.contextPreview") }}</div>
            <p class="text-sm leading-7 text-slate-500">{{ t("page.appSearch.detail.previousPara") }}</p>
            <p class="mt-3 text-sm leading-7 text-slate-800">
              {{ t("page.appSearch.detail.currentPara") }}<DocMindHighlightedText :text="selected.snippet" :query="query" :spans="selected.highlight_spans" />
            </p>
            <p class="mt-3 text-sm leading-7 text-slate-500">{{ t("page.appSearch.detail.nextPara") }}</p>
            <p class="mt-3 text-xs text-slate-400">{{ t("page.appSearch.detail.snippetSource", { start: selected.snippet_window_start, end: selected.snippet_window_end, length: selected.snippet_source_len }) }}</p>
          </div>

          <div class="mt-6 grid grid-cols-2 gap-3">
            <button class="flex items-center justify-center gap-2 rounded-2xl bg-slate-900 px-4 py-3 text-sm font-medium text-white" @click="openSelectedFile">
              <ExternalLink :size="16" />
              {{ t("common.openFile") }}
            </button>
            <button class="flex items-center justify-center gap-2 rounded-2xl border border-slate-200 bg-white px-4 py-3 text-sm font-medium text-slate-700" @click="viewChunks">
              <FileText :size="16" />
              {{ t("common.viewChunks") }}
            </button>
          </div>
        </div>
        <div v-else class="rounded-3xl border border-dashed border-slate-300 bg-slate-50 p-6 text-sm text-slate-500">
          {{ t("page.appSearch.enterQuery") }}
        </div>
      </aside>
    </main>
  </div>
</template>
