<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";
import { Copy, Cpu, Eye, ExternalLink, FileText, FolderOpen, RefreshCw } from "lucide-vue-next";
import { listen } from "@tauri-apps/api/event";
import DocMindBadge from "../components/docmind/DocMindBadge.vue";
import DocMindFileIcon from "../components/docmind/DocMindFileIcon.vue";
import DocMindIndexTree from "../components/docmind/DocMindIndexTree.vue";
import DocMindPreviewBlockRenderer from "../components/docmind/DocMindPreviewBlockRenderer.vue";
import DocMindMarkdownRenderer from "../components/docmind/DocMindMarkdownRenderer.vue";
import SplitPane from "../components/SplitPane.vue";
import { docmindApi, formatDocmindError } from "../services/docmindApi";
import { useIndexDirTree } from "../composables/useIndexDirTree";
import { buildDocumentLocationParts, formatDocumentCitation, resolveDocumentTitlePath } from "../utils/citation";
import type {
  ChunkView,
  DocumentRefreshProgressView,
  DocumentView,
  IndexDirView,
  ParserRuntimeView,
} from "../types/docmind";

const { t } = useI18n();

const route = useRoute();
const router = useRouter();

const dirs = ref<IndexDirView[]>([]);
const documents = ref<DocumentView[]>([]);
const chunks = ref<ChunkView[]>([]);
const selectedDirPath = ref("");
const selectedDocPath = ref("");
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
const errorMessage = ref("");
const actionMessage = ref("");
const actionErrorMessage = ref("");
const docFilter = ref("");
const refreshJobResolvers = new Map<string, (payload: DocumentRefreshProgressView) => void>();
const refreshJobBufferedEvents = new Map<string, DocumentRefreshProgressView>();
const refreshJobPaths = new Map<string, string>();
const version = "v1.0.2";
let unlistenRefreshProgress: null | (() => void) = null;

const {
  visibleRows: visibleDirRows,
  expandAncestors: expandDirAncestors,
  setExpanded: setDirExpanded,
} = useIndexDirTree(dirs);

const explicitIndexDirCount = computed(() => dirs.value.filter((dir) => dir.is_explicit).length);

const currentDocument = computed(
  () => documents.value.find((item) => item.path === selectedDocPath.value) ?? null,
);

const splitPanels = computed(() => [
  { key: "left", initialSize: 240, minSize: 160 },
  { key: "center", minSize: 320, flex: true },
  { key: "right", minSize: 380, flex: true },
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

const currentDocumentRefreshWarning = computed(() =>
  currentDocument.value ? refreshWarnings.value[currentDocument.value.path] ?? "" : "",
);

const currentDocumentRefreshOutcome = computed(() =>
  currentDocument.value ? refreshOutcomes.value[currentDocument.value.path] ?? "idle" : "idle",
);

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

const loadDirs = async () => {
  dirs.value = await docmindApi.listIndexDirs();
};

const loadParserRuntime = async () => {
  parserRuntime.value = await docmindApi.getParserRuntime();
};

const loadDocuments = async () => {
  if (!selectedDirPath.value) {
    documents.value = [];
    return;
  }

  loadingDocs.value = true;
  try {
    documents.value = await docmindApi.listDocumentsInDir(selectedDirPath.value);
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.chunks.section.docList"));
    console.error("[DocMind] listDocumentsInDir failed", error);
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
    chunks.value = await docmindApi.listDocumentChunks(selectedDocPath.value);
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.chunks.section.chunkDetail"));
    console.error("[DocMind] listDocumentChunks failed", error);
    chunks.value = [];
  } finally {
    loadingChunks.value = false;
  }
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
    setActionError(t("page.chunks.action.copyFailed"));
  }
};

const openCurrentDocument = async () => {
  if (!currentDocument.value) {
    return;
  }

  await docmindApi.openFile(currentDocument.value.path);
};

const quickLookCurrentDocument = async () => {
  if (!currentDocument.value) {
    return;
  }

  try {
    await docmindApi.quickLookFile(currentDocument.value.path);
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
    "docmind:document-refresh-progress",
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
        const refreshStart = await docmindApi.refreshDocument(doc.path, doc.dir_path);
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
          errorMessage.value = formatDocmindError(
            refreshResult.message,
            `${t("page.chunks.btn.reslice")}：${doc.file_name}`,
          );
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
        errorMessage.value = formatDocmindError(error, `${t("page.chunks.btn.reslice")}：${doc.file_name}`);
        console.error("[DocMind] refreshDocument failed", error);
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

const syncSelection = async () => {
  loading.value = true;
  errorMessage.value = "";

  try {
    await loadParserRuntime();
    await loadDirs();
    const routePath = resolveDirFromPath(route.query.path);
    selectedDirPath.value =
      routePath || dirs.value.find((dir) => dir.enabled)?.path || dirs.value[0]?.path || "";

    expandDirAncestors(selectedDirPath.value);

    await loadDocuments();

    const targetPath = typeof route.query.path === "string" ? route.query.path : "";
    const matchedDoc = documents.value.find((doc) => doc.path === targetPath);
    selectedDocPath.value = matchedDoc?.path ?? documents.value[0]?.path ?? "";

    await loadChunks();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.chunks.title"));
    console.error("[DocMind] chunks syncSelection failed", error);
  } finally {
    loading.value = false;
  }
};

const selectDir = async (path: string) => {
  selectedDirPath.value = path;
  expandDirAncestors(path);
  selectedDocPath.value = "";
  chunks.value = [];
  docFilter.value = "";
  await loadDocuments();
  selectedDocPath.value = documents.value[0]?.path ?? "";
  await loadChunks();
  if (selectedDocPath.value) {
    void router.replace({ query: { ...route.query, path: selectedDocPath.value } });
  } else {
    const { path: _path, ...nextQuery } = route.query;
    void router.replace({ query: nextQuery });
  }
};

const selectDoc = async (path: string) => {
  selectedDocPath.value = path;
  await loadChunks();
  if (!refreshWarnings.value[path]) {
    errorMessage.value = "";
  }
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

const refreshOutcomeTone = (path?: string) => {
  if (!path) {
    return "default" as const;
  }
  const outcome = refreshOutcomes.value[path];
  if (outcome === "python") {
    return "success" as const;
  }
  if (outcome === "rust") {
    return "warning" as const;
  }
  if (outcome === "failed") {
    return "danger" as const;
  }
  return "default" as const;
};

onMounted(() => {
  void installRefreshProgressListener();
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
    void syncSelection();
  },
  { immediate: true },
);
</script>

<template>
  <div class="flex h-full min-h-0 flex-col bg-slate-50 text-slate-900">
    <header class="flex h-12 items-center justify-between gap-4 border-b border-slate-200 bg-white px-5">
      <div class="min-w-0">
        <div class="flex items-center gap-2">
          <h1 class="text-base font-semibold tracking-tight text-slate-950">{{ t("page.chunks.title") }}</h1>
          <span class="rounded-md bg-slate-100 px-2 py-0.5 text-[10px] text-slate-500">{{ t("page.chunks.section.indexDirs") }}</span>
        </div>
        <p class="mt-0.5 text-xs text-slate-500">
          {{ t("page.chunks.subtitle") }}
        </p>
      </div>
      <div class="flex items-center gap-3 text-xs text-slate-500">
        <div class="hidden sm:block">
          {{ t("page.chunks.parserInfo") }}
          <span class="font-medium text-slate-600">{{ parserRuntime?.active === "python" ? t("page.chunks.parserPython") : t("page.chunks.parserRust") }}</span>
          {{ t("page.chunks.parserInfo2") }}
        </div>
        <DocMindBadge :tone="parserRuntime?.active === 'python' ? 'success' : 'warning'">
          <Cpu class="mr-1" :size="13" />
          {{ parserRuntime?.active === 'python' ? t("page.chunks.badgePython") : t("page.chunks.badgeRust") }}
        </DocMindBadge>
      </div>
    </header>

    <div v-if="errorMessage" class="mb-4 rounded-2xl border border-red-100 bg-red-50 px-4 py-3 text-sm text-red-700">
      {{ errorMessage }}
    </div>

    <main class="flex min-h-0 flex-1 overflow-hidden">
      <SplitPane :panels="splitPanels">
        <template #left>
          <section class="flex min-h-0 flex-1 flex-col overflow-hidden bg-white px-3 py-3">
            <div class="shrink-0 mb-3 flex items-center justify-between">
              <div class="text-xs font-semibold uppercase tracking-wide text-slate-500">{{ t("page.chunks.section.indexDirs") }}</div>
              <DocMindBadge tone="default">
                <FolderOpen class="mr-1" :size="13" />
                {{ explicitIndexDirCount }}
              </DocMindBadge>
            </div>

            <div class="min-h-0 flex-1 overflow-y-auto pr-1">
              <div v-if="loading && dirs.length === 0" class="text-sm text-slate-500">{{ t("page.chunks.empty.dirs") }}</div>

              <div v-else-if="visibleDirRows.length === 0" class="rounded-md bg-slate-50 px-4 py-6 text-sm text-slate-500">
                {{ t("page.chunks.empty.dirs") }}
              </div>

          <DocMindIndexTree
            v-else
            :rows="visibleDirRows"
            :selected-path="selectedDirPath"
            :empty-text="t('page.chunks.empty.dirs')"
            :path-tooltip="true"
            :virtual-label="t('common.virtualDir')"
            :expand-title="t('sidebar.expand')"
            :collapse-title="t('sidebar.collapse')"
            density="normal"
            @node-select="selectDir"
            @toggle="setDirExpanded"
          />
            </div>
          </section>
        </template>

        <template #center>
          <section class="flex min-h-0 flex-1 flex-col overflow-hidden bg-slate-50/60 px-3 py-3">
            <div class="shrink-0 mb-3 flex items-start justify-between gap-3">
              <div>
                <div class="text-xs font-semibold uppercase tracking-wide text-slate-500">{{ t("page.chunks.section.docList") }}</div>
                <div class="mt-1 text-xs text-slate-500">{{ selectedDirPath || t("page.chunks.selectDir") }}</div>
              </div>
              <div class="flex items-center gap-2">
                <DocMindBadge tone="default">
                  <FileText class="mr-1" :size="13" />
                  {{ filteredDocuments.length }}
                </DocMindBadge>
                <DocMindBadge v-if="refreshTaskCount > 0" tone="warning">
                  <RefreshCw class="mr-1" :size="13" />
                  {{ t("page.chunks.btn.queue", { count: refreshTaskCount }) }}
                </DocMindBadge>
              </div>
            </div>

            <div class="shrink-0 mb-3">
              <input
                v-model="docFilter"
                class="w-full rounded-md border border-slate-200 bg-white px-3 py-2 text-sm outline-none focus:border-indigo-300"
                :placeholder="t('page.chunks.filterPlaceholder')"
              />
            </div>

            <div class="min-h-0 flex-1 overflow-y-auto pr-1">
              <div v-if="loadingDocs" class="text-sm text-slate-500">{{ t("page.chunks.readingDocs") }}</div>
              <div v-else-if="filteredDocuments.length === 0" class="rounded-md bg-white px-4 py-6 text-sm text-slate-500">
                {{ t("page.chunks.empty.docs") }}
              </div>
              <div v-else class="space-y-2">
                <div
                  v-for="doc in filteredDocuments"
                  :key="doc.id"
                  class="w-full rounded-md border px-2.5 py-2 text-left transition"
                  :class="selectedDocPath === doc.path ? 'border-indigo-300 bg-indigo-50' : 'border-slate-200 hover:bg-slate-50'"
                  role="button"
                  tabindex="0"
                  @click="selectDoc(doc.path)"
                >
                  <div class="flex items-start gap-3">
                    <DocMindFileIcon :ext="doc.ext" />
                    <div class="min-w-0 flex-1">
                      <div class="truncate text-sm font-medium text-slate-950">{{ doc.file_name }}</div>
                      <div class="mt-1 truncate text-[11px] text-slate-500">{{ doc.path }}</div>
                      <div class="mt-2 flex items-center gap-2 text-[11px] text-slate-500">
                        <span>{{ t("page.chunks.chunkStats", { count: doc.chunks }) }}</span>
                        <span>·</span>
                        <span>{{ doc.modified }}</span>
                      </div>
                      <div
                        v-if="refreshWarnings[doc.path]"
                        class="mt-2 inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[10px]"
                        :class="refreshOutcomes[doc.path] === 'python'
                          ? 'border-emerald-100 bg-emerald-50 text-emerald-700'
                          : 'border-amber-100 bg-amber-50 text-amber-700'"
                      >
                        {{ refreshOutcomes[doc.path] === 'python' ? t("page.chunks.refreshState.pythonDone") : t("page.chunks.refreshState.rustFallback") }}
                      </div>
                      <div
                        v-else-if="refreshOutcomes[doc.path] === 'python' || refreshOutcomes[doc.path] === 'rust'"
                        class="mt-2 inline-flex items-center rounded-full px-2 py-0.5 text-[10px]"
                        :class="refreshOutcomes[doc.path] === 'python'
                          ? 'border border-emerald-100 bg-emerald-50 text-emerald-700'
                          : 'border border-amber-100 bg-amber-50 text-amber-700'"
                      >
                        {{ refreshOutcomeLabel(doc.path) }}
                      </div>
                    </div>
                    <button
                      class="inline-flex shrink-0 items-center gap-1 rounded-md border border-slate-200 bg-white px-2 py-1 text-xs text-slate-600 transition hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-60"
                      :disabled="loadingDocs || isDocRefreshBusy(doc.path)"
                      @click.stop="void refreshDocument(doc)"
                    >
                      <RefreshCw :size="13" :class="isDocRefreshing(doc.path) ? 'animate-spin' : ''" />
                      {{ refreshStateLabel(doc.path) }}
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </section>
        </template>

        <template #right>
          <section class="flex min-h-0 flex-1 flex-col overflow-hidden bg-white px-3 py-3">
            <div class="shrink-0 mb-3 flex items-center justify-between gap-3">
              <div>
                <div class="text-xs font-semibold uppercase tracking-wide text-slate-500">{{ t("page.chunks.section.chunkDetail") }}</div>
                <div class="mt-1 text-xs text-slate-500">
                  {{ currentDocument?.file_name || t("page.chunks.selectDoc") }}
                </div>
                <div
                  v-if="currentDocument && currentDocumentRefreshOutcome !== 'idle'"
                  class="mt-2 inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11px]"
                  :class="refreshOutcomeTone(currentDocument.path) === 'success'
                    ? 'border-emerald-100 bg-emerald-50 text-emerald-700'
                    : refreshOutcomeTone(currentDocument.path) === 'warning'
                      ? 'border-amber-100 bg-amber-50 text-amber-700'
                      : refreshOutcomeTone(currentDocument.path) === 'danger'
                        ? 'border-rose-100 bg-rose-50 text-rose-700'
                        : 'border-slate-200 bg-slate-50 text-slate-600'"
                >
                  <Cpu :size="11" />
                  {{ refreshOutcomeLabel(currentDocument.path) }}
                </div>
                <div
                  v-if="currentDocumentRefreshWarning"
                  class="mt-2 text-[11px] leading-5 text-amber-700"
                >
                  {{ currentDocumentRefreshWarning }}
                </div>
              </div>
              <button
                class="inline-flex items-center gap-2 rounded-md border border-slate-200 bg-white px-3 py-2 text-xs text-slate-600 hover:bg-slate-50"
                :disabled="loading || !selectedDirPath"
                @click="void syncSelection()"
              >
                <RefreshCw :size="14" />
                {{ t("page.chunks.btn.refresh") }}
              </button>
            </div>

            <div class="shrink-0 mb-3">
              <div class="rounded-md border border-slate-200 bg-slate-50 px-4 py-3">
                <div class="text-sm font-medium text-slate-950">{{ currentDocument?.file_name || t("page.chunks.selectDoc") }}</div>
                <div class="mt-1 break-all text-[11px] text-slate-500">
                  {{ currentDocument?.path || t("page.chunks.selectDoc") }}
                </div>
                <div class="mt-2 flex flex-wrap gap-2">
                  <DocMindBadge v-if="currentDocument">{{ currentDocument.ext.toUpperCase() }}</DocMindBadge>
                  <DocMindBadge v-if="currentDocument">{{ t("page.chunks.chunkStats", { count: currentDocument.chunks }) }}</DocMindBadge>
                </div>
                <div class="mt-3 grid grid-cols-2 gap-2">
                  <button
                    class="inline-flex items-center justify-center gap-2 rounded-md border border-slate-200 bg-white px-3 py-2 text-xs font-medium text-slate-700 hover:bg-slate-50"
                    :disabled="!currentDocument"
                    @click="quickLookCurrentDocument"
                  >
                    <Eye :size="14" />
                    {{ t("page.chunks.action.quickLook") }}
                  </button>
                  <button
                    class="inline-flex items-center justify-center gap-2 rounded-md border border-slate-200 bg-white px-3 py-2 text-xs font-medium text-slate-700 hover:bg-slate-50"
                    :disabled="!currentDocument"
                    @click="openCurrentDocument"
                  >
                    <ExternalLink :size="14" />
                    {{ t("common.openFile") }}
                  </button>
                  <button
                    class="inline-flex items-center justify-center gap-2 rounded-md border border-slate-200 bg-white px-3 py-2 text-xs font-medium text-slate-700 hover:bg-slate-50"
                    :disabled="!currentDocument"
                    @click="copyCurrentDocumentPath"
                  >
                    <Copy :size="14" />
                    {{ t("page.chunks.action.copyPath") }}
                  </button>
                  <button
                    class="inline-flex items-center justify-center gap-2 rounded-md border border-slate-200 bg-white px-3 py-2 text-xs font-medium text-slate-700 hover:bg-slate-50"
                    :disabled="!currentDocument"
                    @click="copyText(currentDocumentCitation, t('page.chunks.action.copiedCitation'))"
                  >
                    <FileText :size="14" />
                    {{ t("page.chunks.action.copyCitation") }}
                  </button>
                </div>
                <div
                  v-if="currentDocument && currentDocumentRefreshOutcome !== 'idle'"
                  class="mt-2 inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11px]"
                  :class="refreshOutcomeTone(currentDocument.path) === 'success'
                    ? 'border-emerald-100 bg-emerald-50 text-emerald-700'
                    : refreshOutcomeTone(currentDocument.path) === 'warning'
                      ? 'border-amber-100 bg-amber-50 text-amber-700'
                      : refreshOutcomeTone(currentDocument.path) === 'danger'
                        ? 'border-rose-100 bg-rose-50 text-rose-700'
                        : 'border-slate-200 bg-slate-50 text-slate-600'"
                >
                  <Cpu :size="11" />
                  {{ refreshOutcomeLabel(currentDocument.path) }}
                </div>
                <div
                  v-if="currentDocumentRefreshWarning"
                  class="mt-2 text-[11px] leading-5 text-amber-700"
                >
                  {{ currentDocumentRefreshWarning }}
                </div>
                <div v-if="actionMessage" class="mt-3 rounded-md border border-emerald-100 bg-emerald-50 px-3 py-2 text-xs text-emerald-700">
                  {{ actionMessage }}
                </div>
                <div v-if="actionErrorMessage" class="mt-3 rounded-md border border-red-100 bg-red-50 px-3 py-2 text-xs text-red-700">
                  {{ actionErrorMessage }}
                </div>
              </div>
            </div>

            <div class="min-h-0 flex-1 overflow-y-auto pr-1">
              <div v-if="loadingChunks" class="text-sm text-slate-500">{{ t("page.chunks.readingChunks") }}</div>
              <div v-else-if="!currentDocument" class="rounded-md bg-slate-50 px-4 py-6 text-sm text-slate-500">
                {{ t("page.chunks.empty.selectDocToView") }}
              </div>
              <div v-else-if="chunks.length === 0" class="rounded-md bg-slate-50 px-4 py-6 text-sm text-slate-500">
                {{ t("page.chunks.empty.chunks") }}
              </div>
              <div v-else class="space-y-3">
                <div v-for="chunk in chunks" :key="chunk.id" class="rounded-md border border-slate-200 bg-white p-3">
                  <div class="mb-2 flex items-center justify-between gap-2">
                    <div class="min-w-0 flex-1">
                      <div class="text-sm font-medium text-slate-950">{{ chunk.title_path || chunk.heading }}</div>
                      <div class="mt-1 text-[11px] text-slate-500">
                        {{ t("page.chunks.titlePath") }}：{{ chunk.title_path || chunk.heading }}
                      </div>
                    </div>
                    <div class="flex shrink-0 items-center gap-2">
                      <DocMindBadge tone="default">
                        {{ chunk.page ? t("page.chunks.page", { page: chunk.page }) : t("page.chunks.paragraph", { para: chunk.paragraph ?? 0 }) }}
                      </DocMindBadge>
                      <button
                        class="rounded-md border border-slate-200 bg-white px-2 py-1 text-[11px] text-slate-600 hover:bg-slate-50"
                        @click="copyChunkCitation(chunk)"
                      >
                        {{ t("page.chunks.action.copyCitation") }}
                      </button>
                    </div>
                  </div>
                  <div v-if="chunk.preview_blocks && chunk.preview_blocks.length > 0" class="space-y-1">
                    <DocMindPreviewBlockRenderer
                      v-for="block in chunk.preview_blocks"
                      :key="block.block_index"
                      :block="block"
                    />
                  </div>
                  <DocMindMarkdownRenderer
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
            </div>
          </section>
        </template>
      </SplitPane>
    </main>

    <footer class="flex h-6 items-center justify-between border-t border-slate-200 bg-slate-100 px-4 text-[11px] text-slate-500">
      <div class="flex items-center gap-3">
        <span>
          <Cpu :size="12" class="mr-1 inline" />
          {{ parserRuntime?.active === "python" ? t("page.chunks.parserPython") : t("page.chunks.parserRust") }}
        </span>
        <span>SQLite + Tantivy</span>
      </div>
      <div class="flex items-center gap-3">
        <span>{{ version }}</span>
      </div>
    </footer>
  </div>
</template>
