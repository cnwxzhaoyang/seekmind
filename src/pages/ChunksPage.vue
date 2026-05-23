<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
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
const errorMessage = ref("");
const docFilter = ref("");
const refreshJobResolvers = new Map<string, (payload: DocumentRefreshProgressView) => void>();
const refreshJobBufferedEvents = new Map<string, DocumentRefreshProgressView>();
let unlistenRefreshProgress: null | (() => void) = null;

const currentDocument = computed(
  () => documents.value.find((item) => item.path === selectedDocPath.value) ?? null,
);

const queuedDocPaths = computed(() => new Set(refreshQueue.value.map((doc) => doc.path)));

const refreshTaskCount = computed(
  () => refreshQueue.value.length + (refreshingDocPath.value ? 1 : 0),
);

const currentDocumentRefreshWarning = computed(() =>
  currentDocument.value ? refreshWarnings.value[currentDocument.value.path] ?? "" : "",
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
    errorMessage.value = formatDocmindError(error, "文档列表加载失败");
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
    errorMessage.value = formatDocmindError(error, "切片加载失败");
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

      if (payload.state === "running") {
        return;
      }

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
      errorMessage.value = "";

      try {
        const refreshStart = await docmindApi.refreshDocument(doc.path, doc.dir_path);
        const refreshResult = await waitForRefreshJob(refreshStart.job_id);

        if (refreshResult.state === "completed" && refreshResult.warning) {
          refreshWarnings.value = {
            ...refreshWarnings.value,
            [doc.path]: refreshResult.warning,
          };
        } else {
          const { [doc.path]: _removed, ...rest } = refreshWarnings.value;
          refreshWarnings.value = rest;
        }
        if (refreshResult.state === "completed" && doc.dir_path === selectedDirPath.value) {
          await loadDocuments();
          if (selectedDocPath.value === doc.path) {
            await loadChunks();
          }
        }
        if (refreshResult.state === "failed") {
          errorMessage.value = formatDocmindError(
            refreshResult.message,
            `重新切片失败：${doc.file_name}`,
          );
        }
      } catch (error) {
        const { [doc.path]: _removed, ...rest } = refreshWarnings.value;
        refreshWarnings.value = rest;
        errorMessage.value = formatDocmindError(error, `重新切片失败：${doc.file_name}`);
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

  refreshQueue.value.push(doc);
  void processRefreshQueue();
};

const syncSelection = async () => {
  loading.value = true;
  errorMessage.value = "";
  refreshWarnings.value = {};

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
    errorMessage.value = formatDocmindError(error, "切片页面加载失败");
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
        <h1 class="text-2xl font-semibold tracking-tight text-slate-950">文档切片</h1>
        <p class="mt-1 text-sm text-slate-500">按目录选择文档，直接查看 docMind 如何把内容切成段落块。</p>
        <p class="mt-1 text-xs text-slate-400">
          当前解析器：
          <span class="font-medium text-slate-600">{{ parserRuntime?.active === "python" ? "Python" : "Rust" }}</span>
          ；点击“重新切片”会优先使用当前启用的解析链路。
        </p>
      </div>
      <DocMindBadge :tone="parserRuntime?.active === 'python' ? 'success' : 'warning'">
        <Cpu class="mr-1" :size="13" />
        {{ parserRuntime?.active === 'python' ? "Python 解析" : "Rust 回退" }}
      </DocMindBadge>
    </div>

    <div v-if="errorMessage" class="mb-4 rounded-2xl border border-red-100 bg-red-50 px-4 py-3 text-sm text-red-700">
      {{ errorMessage }}
    </div>

    <div class="grid min-h-[calc(100vh-180px)] grid-cols-[280px_minmax(340px,0.95fr)_minmax(420px,1.1fr)] gap-4">
      <section class="min-h-0 overflow-y-auto rounded-3xl border border-slate-200 bg-white p-4 shadow-sm">
        <div class="mb-4 flex items-center justify-between">
          <div class="text-sm font-semibold text-slate-900">索引目录</div>
          <DocMindBadge tone="default">
            <FolderOpen class="mr-1" :size="13" />
            {{ dirs.length }}
          </DocMindBadge>
        </div>

        <div v-if="loading && dirs.length === 0" class="text-sm text-slate-500">正在读取目录...</div>

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
              <span>{{ dir.docs }} 文档 · {{ dir.chunks.toLocaleString() }} 切片</span>
              <span>{{ dir.enabled ? "启用" : "禁用" }}</span>
            </div>
          </button>
        </div>
      </section>

      <section class="min-h-0 overflow-y-auto rounded-3xl border border-slate-200 bg-white p-4 shadow-sm">
        <div class="mb-4 flex items-center justify-between">
          <div>
            <div class="text-sm font-semibold text-slate-900">文档列表</div>
            <div class="mt-1 text-xs text-slate-500">{{ selectedDirPath || "请选择一个索引目录" }}</div>
          </div>
          <div class="flex items-center gap-2">
            <DocMindBadge tone="default">
              <FileText class="mr-1" :size="13" />
              {{ filteredDocuments.length }}
            </DocMindBadge>
            <DocMindBadge v-if="refreshTaskCount > 0" tone="warning">
              <RefreshCw class="mr-1" :size="13" />
              队列 {{ refreshTaskCount }}
            </DocMindBadge>
          </div>
        </div>

        <input
          v-model="docFilter"
          class="mb-4 w-full rounded-2xl border border-slate-200 bg-slate-50 px-4 py-2 text-sm outline-none focus:border-slate-300"
          placeholder="过滤文件名或路径..."
        />

        <div v-if="loadingDocs" class="text-sm text-slate-500">正在读取文档...</div>
        <div v-else-if="filteredDocuments.length === 0" class="rounded-2xl bg-slate-50 px-4 py-6 text-sm text-slate-500">
          当前目录没有已解析文档。
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
                  <span>{{ doc.chunks }} 切片</span>
                  <span>·</span>
                  <span>{{ doc.modified }}</span>
                </div>
                <div
                  v-if="refreshWarnings[doc.path]"
                  class="mt-2 inline-flex items-center rounded-full border border-amber-100 bg-amber-50 px-2 py-0.5 text-[11px] text-amber-700"
                >
                  本次回退到 Rust
                </div>
              </div>
              <button
                class="inline-flex shrink-0 items-center gap-1 rounded-xl border border-slate-200 bg-white px-2.5 py-1.5 text-xs text-slate-600 transition hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-60"
                :disabled="loadingDocs || refreshingDocPath === doc.path"
                @click.stop="void refreshDocument(doc)"
              >
                <RefreshCw :size="13" :class="refreshingDocPath === doc.path ? 'animate-spin' : ''" />
                {{ refreshingDocPath === doc.path ? "切片中" : queuedDocPaths.has(doc.path) ? "已排队" : "重新切片" }}
              </button>
            </div>
          </div>
        </div>
      </section>

      <section class="min-h-0 overflow-y-auto rounded-3xl border border-slate-200 bg-white p-4 shadow-sm">
        <div class="mb-4 flex items-center justify-between">
          <div>
            <div class="text-sm font-semibold text-slate-900">切片详情</div>
            <div class="mt-1 text-xs text-slate-500">
              {{ currentDocument?.file_name || "请选择一个文档" }}
            </div>
          </div>
          <button
            class="inline-flex items-center gap-2 rounded-2xl border border-slate-200 bg-white px-3 py-2 text-xs text-slate-600 hover:bg-slate-50"
            :disabled="loading || !selectedDirPath"
            @click="void syncSelection()"
          >
            <RefreshCw :size="14" />
            刷新
          </button>
        </div>

        <div v-if="loadingChunks" class="text-sm text-slate-500">正在读取切片...</div>
        <div v-else-if="!currentDocument" class="rounded-2xl bg-slate-50 px-4 py-6 text-sm text-slate-500">
          选中文档后，这里会显示它被切成的每个段落块。
        </div>
        <div v-else class="space-y-3">
          <div
            v-if="currentDocumentRefreshWarning"
            class="rounded-2xl border border-amber-100 bg-amber-50 px-4 py-3 text-sm text-amber-800"
          >
            <div class="font-medium">本次切片已从 Python 回退到 Rust</div>
            <div class="mt-1 text-xs leading-6 text-amber-700">{{ currentDocumentRefreshWarning }}</div>
          </div>

          <div class="rounded-2xl border border-slate-200 bg-slate-50 px-4 py-3">
            <div class="text-sm font-medium text-slate-900">{{ currentDocument.file_name }}</div>
            <div class="mt-1 break-all text-xs text-slate-500">{{ currentDocument.path }}</div>
            <div class="mt-2 flex flex-wrap gap-2">
              <DocMindBadge>{{ currentDocument.ext.toUpperCase() }}</DocMindBadge>
              <DocMindBadge>{{ currentDocument.chunks }} 个切片</DocMindBadge>
            </div>
          </div>

          <div v-if="chunks.length === 0" class="rounded-2xl bg-slate-50 px-4 py-6 text-sm text-slate-500">
            该文档暂未生成切片。
          </div>

          <div v-else class="space-y-3">
            <div v-for="chunk in chunks" :key="chunk.id" class="rounded-2xl border border-slate-200 bg-white p-4">
              <div class="mb-2 flex items-center justify-between gap-2">
                <div class="text-sm font-medium text-slate-900">{{ chunk.heading }}</div>
                <DocMindBadge tone="default">
                  {{ chunk.page ? `第 ${chunk.page} 页` : `第 ${chunk.paragraph ?? 0} 段` }}
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
