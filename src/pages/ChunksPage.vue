<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";
import { Cpu, FileText, FolderOpen, Layers3, RefreshCw } from "lucide-vue-next";
import { listen } from "@tauri-apps/api/event";
import DocMindBadge from "../components/docmind/DocMindBadge.vue";
import DocMindFileIcon from "../components/docmind/DocMindFileIcon.vue";
import { docmindApi, formatDocmindError } from "../services/docmindApi";
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
const docFilter = ref("");
const refreshJobResolvers = new Map<string, (payload: DocumentRefreshProgressView) => void>();
const refreshJobBufferedEvents = new Map<string, DocumentRefreshProgressView>();
const refreshJobPaths = new Map<string, string>();
let unlistenRefreshProgress: null | (() => void) = null;

const currentDocument = computed(
  () => documents.value.find((item) => item.path === selectedDocPath.value) ?? null,
);

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
  selectedDocPath.value = "";
  chunks.value = [];
  docFilter.value = "";
  await loadDocuments();
  selectedDocPath.value = documents.value[0]?.path ?? "";
  await loadChunks();
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
  <div class="h-full overflow-y-auto p-8">
    <div class="mb-7 flex flex-wrap items-start justify-between gap-3">
      <div>
        <h1 class="text-2xl font-semibold tracking-tight text-slate-950">{{ t("page.chunks.title") }}</h1>
        <p class="mt-1 text-sm text-slate-500">{{ t("page.chunks.subtitle") }}</p>
        <p class="mt-1 text-xs text-slate-400">
          {{ t("page.chunks.parserInfo") }}
          <span class="font-medium text-slate-600">{{ parserRuntime?.active === "python" ? t("page.chunks.parserPython") : t("page.chunks.parserRust") }}</span>
          {{ t("page.chunks.parserInfo2") }}
        </p>
      </div>
      <DocMindBadge :tone="parserRuntime?.active === 'python' ? 'success' : 'warning'">
        <Cpu class="mr-1" :size="13" />
        {{ parserRuntime?.active === 'python' ? t("page.chunks.badgePython") : t("page.chunks.badgeRust") }}
      </DocMindBadge>
    </div>

    <div v-if="errorMessage" class="mb-4 rounded-2xl border border-red-100 bg-red-50 px-4 py-3 text-sm text-red-700">
      {{ errorMessage }}
    </div>

    <div class="grid min-h-[calc(100vh-180px)] grid-cols-[280px_minmax(340px,0.95fr)_minmax(420px,1.1fr)] gap-4">
      <section class="min-h-0 overflow-y-auto rounded-3xl border border-slate-200 bg-white p-4 shadow-sm">
        <div class="mb-4 flex items-center justify-between">
          <div class="text-sm font-semibold text-slate-900">{{ t("page.chunks.section.indexDirs") }}</div>
          <DocMindBadge tone="default">
            <FolderOpen class="mr-1" :size="13" />
            {{ dirs.length }}
          </DocMindBadge>
        </div>

        <div v-if="loading && dirs.length === 0" class="text-sm text-slate-500">{{ t("page.chunks.empty.dirs") }}</div>

        <div v-else class="space-y-2">
          <button
            v-for="dir in dirs"
            :key="dir.path"
            class="w-full rounded-2xl border px-3 py-3 text-left transition"
            :class="selectedDirPath === dir.path ? 'border-slate-300 bg-slate-50' : 'border-slate-200 hover:bg-slate-50'"
            @click="selectDir(dir.path)"
          >
            <div class="truncate text-sm font-medium text-slate-900">{{ dir.path }}</div>
            <div class="mt-1 flex items-center justify-between text-xs text-slate-500">
              <span>{{ t("page.chunks.docStats", { docs: dir.docs, chunks: dir.chunks.toLocaleString() }) }}</span>
              <span>{{ dir.enabled ? t("page.chunks.status.enabled") : t("page.chunks.status.disabled") }}</span>
            </div>
          </button>
        </div>
      </section>

      <section class="min-h-0 overflow-y-auto rounded-3xl border border-slate-200 bg-white p-4 shadow-sm">
        <div class="mb-4 flex items-center justify-between">
          <div>
            <div class="text-sm font-semibold text-slate-900">{{ t("page.chunks.section.docList") }}</div>
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

        <input
          v-model="docFilter"
          class="mb-4 w-full rounded-2xl border border-slate-200 bg-slate-50 px-4 py-2 text-sm outline-none focus:border-slate-300"
          :placeholder="t('page.chunks.filterPlaceholder')"
        />

        <div v-if="loadingDocs" class="text-sm text-slate-500">{{ t("page.chunks.readingDocs") }}</div>
        <div v-else-if="filteredDocuments.length === 0" class="rounded-2xl bg-slate-50 px-4 py-6 text-sm text-slate-500">
          {{ t("page.chunks.empty.docs") }}
        </div>
        <div v-else class="space-y-2">
          <div
            v-for="doc in filteredDocuments"
            :key="doc.id"
            class="w-full rounded-2xl border px-3 py-3 text-left transition"
            :class="selectedDocPath === doc.path ? 'border-slate-300 bg-slate-50' : 'border-slate-200 hover:bg-slate-50'"
            role="button"
            tabindex="0"
            @click="selectDoc(doc.path)"
          >
            <div class="flex items-start gap-3">
              <DocMindFileIcon :ext="doc.ext" />
              <div class="min-w-0 flex-1">
                <div class="truncate text-sm font-medium text-slate-900">{{ doc.file_name }}</div>
                <div class="mt-1 truncate text-xs text-slate-500">{{ doc.path }}</div>
                <div class="mt-2 flex items-center gap-2 text-xs text-slate-500">
                  <span>{{ t("page.chunks.chunkStats", { count: doc.chunks }) }}</span>
                  <span>·</span>
                  <span>{{ doc.modified }}</span>
                </div>
                <div
                  v-if="refreshWarnings[doc.path]"
                  class="mt-2 inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11px]"
                  :class="refreshOutcomes[doc.path] === 'python'
                    ? 'border-emerald-100 bg-emerald-50 text-emerald-700'
                    : 'border-amber-100 bg-amber-50 text-amber-700'"
                >
                  {{ refreshOutcomes[doc.path] === 'python' ? t("page.chunks.refreshState.pythonDone") : t("page.chunks.refreshState.rustFallback") }}
                </div>
                <div
                  v-else-if="refreshOutcomes[doc.path] === 'python' || refreshOutcomes[doc.path] === 'rust'"
                  class="mt-2 inline-flex items-center rounded-full px-2 py-0.5 text-[11px]"
                  :class="refreshOutcomes[doc.path] === 'python'
                    ? 'border border-emerald-100 bg-emerald-50 text-emerald-700'
                    : 'border border-amber-100 bg-amber-50 text-amber-700'"
                >
                  {{ refreshOutcomeLabel(doc.path) }}
                </div>
              </div>
              <button
                class="inline-flex shrink-0 items-center gap-1 rounded-xl border border-slate-200 bg-white px-2.5 py-1.5 text-xs text-slate-600 transition hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-60"
                :disabled="loadingDocs || isDocRefreshBusy(doc.path)"
                @click.stop="void refreshDocument(doc)"
              >
                <RefreshCw :size="13" :class="isDocRefreshing(doc.path) ? 'animate-spin' : ''" />
                {{ refreshStateLabel(doc.path) }}
              </button>
            </div>
          </div>
        </div>
      </section>

      <section class="min-h-0 overflow-y-auto rounded-3xl border border-slate-200 bg-white p-4 shadow-sm">
        <div class="mb-4 flex items-center justify-between">
          <div>
            <div class="text-sm font-semibold text-slate-900">{{ t("page.chunks.section.chunkDetail") }}</div>
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
            class="inline-flex items-center gap-2 rounded-2xl border border-slate-200 bg-white px-3 py-2 text-xs text-slate-600 hover:bg-slate-50"
            :disabled="loading || !selectedDirPath"
            @click="void syncSelection()"
          >
            <RefreshCw :size="14" />
            {{ t("page.chunks.btn.refresh") }}
          </button>
        </div>

        <div v-if="loadingChunks" class="text-sm text-slate-500">{{ t("page.chunks.readingChunks") }}</div>
        <div v-else-if="!currentDocument" class="rounded-2xl bg-slate-50 px-4 py-6 text-sm text-slate-500">
          {{ t("page.chunks.empty.selectDocToView") }}
        </div>
        <div v-else class="space-y-3">
          <div class="rounded-2xl border border-slate-200 bg-slate-50 px-4 py-3">
            <div class="text-sm font-medium text-slate-900">{{ currentDocument.file_name }}</div>
            <div class="mt-1 break-all text-xs text-slate-500">{{ currentDocument.path }}</div>
            <div class="mt-2 flex flex-wrap gap-2">
              <DocMindBadge>{{ currentDocument.ext.toUpperCase() }}</DocMindBadge>
              <DocMindBadge>{{ t("page.chunks.chunkStats", { count: currentDocument.chunks }) }}</DocMindBadge>
            </div>
          </div>

          <div v-if="chunks.length === 0" class="rounded-2xl bg-slate-50 px-4 py-6 text-sm text-slate-500">
            {{ t("page.chunks.empty.chunks") }}
          </div>

          <div v-else class="space-y-3">
            <div v-for="chunk in chunks" :key="chunk.id" class="rounded-2xl border border-slate-200 bg-white p-4">
              <div class="mb-2 flex items-center justify-between gap-2">
                <div class="text-sm font-medium text-slate-900">{{ chunk.heading }}</div>
                <DocMindBadge tone="default">
                  {{ chunk.page ? t("page.chunks.page", { page: chunk.page }) : t("page.chunks.paragraph", { para: chunk.paragraph ?? 0 }) }}
                </DocMindBadge>
              </div>
              <p class="text-sm leading-7 text-slate-700">{{ chunk.snippet }}</p>
            </div>
          </div>
        </div>
      </section>
    </div>
  </div>
</template>
