<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { FolderPlus, FolderOpen, CheckCircle2, Loader2, RefreshCw, X, ToggleLeft, ToggleRight, UploadCloud, Eye, Copy, FileText } from "lucide-vue-next";
import DocMindBadge from "../components/docmind/DocMindBadge.vue";
import DocMindIndexTree from "../components/docmind/DocMindIndexTree.vue";
import DocMindTaskCard from "../components/docmind/DocMindTaskCard.vue";
import { useIndexDirTree } from "../composables/useIndexDirTree";
import { docmindApi, formatDocmindError } from "../services/docmindApi";
import { formatDirectoryCitation } from "../utils/citation";
import type {
  DocumentRefreshProgressView,
  IndexDirView,
  IndexRefreshProgressView,
  IndexStatusView,
  ImportedPathView,
  ImportPathsView,
} from "../types/docmind";

const { t } = useI18n();

const dirs = ref<IndexDirView[]>([]);
const status = ref<IndexStatusView | null>(null);
const loading = ref(false);
const refreshing = ref(false);
const importing = ref(false);
const dragActive = ref(false);
const busyPath = ref<string | null>(null);
const errorMessage = ref("");
const infoMessage = ref("");
const refreshJobResolvers = new Map<string, (payload: IndexRefreshProgressView) => void>();
const refreshJobBufferedEvents = new Map<string, IndexRefreshProgressView>();
const documentRefreshResolvers = new Map<string, (payload: DocumentRefreshProgressView) => void>();
const documentRefreshBufferedEvents = new Map<string, DocumentRefreshProgressView>();
let unlistenIndexRefreshProgress: null | (() => void) = null;
let unlistenDocumentRefreshProgress: null | (() => void) = null;
let unlistenFileDrop: null | (() => void) = null;

const {
  visibleRows: visibleDirRows,
} = useIndexDirTree(dirs);

const explicitIndexDirCount = computed(() => dirs.value.filter((dir) => dir.is_explicit).length);

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
    errorMessage.value = formatDocmindError(error, successMessage);
  }
};

const taskDisplayState = computed(() => {
  const task = status.value?.current_task;
  if (!task) {
    return { label: t("status.idle"), tone: "default" as const, spinning: false };
  }

  if (task.state === "paused") {
    return { label: t("status.paused"), tone: "default" as const, spinning: false };
  }
  if (task.state === "running") {
    return { label: t("status.running"), tone: "warning" as const, spinning: true };
  }

  return { label: task.state || t("status.processing"), tone: "warning" as const, spinning: true };
});

const loadDirs = async () => {
  loading.value = true;
  errorMessage.value = "";

  try {
    dirs.value = await docmindApi.listIndexDirs();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.library.title"));
    console.error("[DocMind] loadDirs failed", error);
  } finally {
    loading.value = false;
  }
};

const loadStatus = async () => {
  try {
    status.value = await docmindApi.getIndexStatus();
  } catch (error) {
    console.error("[DocMind] loadStatus failed", error);
  }
};

const waitForIndexRefreshJob = (jobId: string) => {
  const buffered = refreshJobBufferedEvents.get(jobId);
  if (buffered) {
    refreshJobBufferedEvents.delete(jobId);
    return Promise.resolve(buffered);
  }

  return new Promise<IndexRefreshProgressView>((resolve) => {
    refreshJobResolvers.set(jobId, resolve);
  });
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

const installIndexRefreshListener = async () => {
  if (unlistenIndexRefreshProgress) {
    return;
  }

  unlistenIndexRefreshProgress = await listen<IndexRefreshProgressView>(
    "docmind:index-refresh-progress",
    (event) => {
      const payload = event.payload;
      status.value = payload.status;

      const resolver = refreshJobResolvers.get(payload.job_id);
      if (payload.state !== "running" && resolver) {
        refreshJobResolvers.delete(payload.job_id);
        resolver(payload);
      } else if (payload.state !== "running") {
        refreshJobBufferedEvents.set(payload.job_id, payload);
      }

      if (payload.state !== "running") {
        void loadDirs();
        void loadStatus();
      }
    },
  );
};

const installDocumentRefreshListener = async () => {
  if (unlistenDocumentRefreshProgress) {
    return;
  }

  unlistenDocumentRefreshProgress = await listen<DocumentRefreshProgressView>(
    "docmind:document-refresh-progress",
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

void installIndexRefreshListener();
void installDocumentRefreshListener();
void installFileDropListener();

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
    await docmindApi.addIndexDir(selected);
    infoMessage.value = t("page.library.info.added", { path: selected });
    await loadDirs();
    await loadStatus();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.library.error.addDir"));
    console.error("[DocMind] addIndexDir failed", error);
  } finally {
    busyPath.value = null;
  }
};

const refreshIndex = async () => {
  refreshing.value = true;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    const started = await docmindApi.refreshIndex();
    const finished = await waitForIndexRefreshJob(started.job_id);
    if (finished.state === "failed") {
      throw new Error(finished.message || t("page.library.error.rebuild"));
    }
    await loadDirs();
    await loadStatus();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.status.btn.reindex"));
    console.error("[DocMind] refreshIndex failed", error);
  } finally {
    refreshing.value = false;
  }
};

const refreshSingleDir = async (path: string) => {
  busyPath.value = path;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    const started = await docmindApi.refreshIndexDir(path);
    const finished = await waitForIndexRefreshJob(started.job_id);
    if (finished.state === "failed") {
      throw new Error(finished.message || t("page.library.error.rebuild"));
    }
    infoMessage.value = t("page.library.info.reindexed", { path });
    await loadDirs();
    await loadStatus();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.library.error.rebuild"));
    console.error("[DocMind] refreshIndexDir failed", error);
  } finally {
    busyPath.value = null;
  }
};

const toggleDir = async (dir: IndexDirView) => {
  busyPath.value = dir.path;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    await docmindApi.setIndexDirEnabled(dir.path, !dir.enabled);
    infoMessage.value = dir.enabled ? t("page.library.info.disabled", { path: dir.path }) : t("page.library.info.enabled", { path: dir.path });
    await loadDirs();
    await loadStatus();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.library.error.toggleDir"));
    console.error("[DocMind] setIndexDirEnabled failed", error);
  } finally {
    busyPath.value = null;
  }
};

const quickLookDir = async (path: string) => {
  busyPath.value = path;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    await docmindApi.quickLookFile(path);
    infoMessage.value = t("page.library.action.quickLookOpened");
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.library.action.quickLookFailed"));
    console.error("[DocMind] quickLookDir failed", error);
  } finally {
    busyPath.value = null;
  }
};

const copyDirPath = async (path: string) => {
  await copyText(path, t("page.library.action.copiedPath"));
};

const copyDirCitation = async (row: { displayName: string; fullPath: string; dir: IndexDirView }) => {
  await copyText(
    formatDirectoryCitation({
      displayName: row.displayName,
      path: row.fullPath,
      docs: row.dir.docs,
      chunks: row.dir.chunks,
    }),
    t("page.library.action.copiedCitation"),
  );
};

const removeDir = async (path: string) => {
  if (!window.confirm(t("page.library.confirmRemove", { path }))) {
    return;
  }

  busyPath.value = path;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    await docmindApi.removeIndexDir(path);
    infoMessage.value = t("page.library.info.deleted", { path });
    await loadDirs();
    await loadStatus();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.library.error.deleteDir"));
    console.error("[DocMind] removeIndexDir failed", error);
  } finally {
    busyPath.value = null;
  }
};

const processImportedFiles = async (importedFiles: ImportedPathView[]) => {
  const queued = importedFiles.filter((item) => item.dir_path !== "");
  for (const file of queued) {
    busyPath.value = file.dir_path;
    try {
      const started = await docmindApi.refreshDocument(file.path, file.dir_path);
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
      const started = await docmindApi.refreshIndexDir(path);
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
  const normalized = paths.map((path) => path.trim()).filter((path) => path.length > 0);
  if (normalized.length === 0) {
    return;
  }

  importing.value = true;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    const result: ImportPathsView = await docmindApi.importPaths(normalized);
    const dirsToRefresh = result.added_dirs.filter((path) => path !== result.virtual_dir);
    if (dirsToRefresh.length > 0) {
      infoMessage.value = t("page.library.info.importing", { count: normalized.length });
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
      t("page.library.info.importedFiles", { count: result.imported_files.length }),
    ];
    if (result.virtual_dir) {
      summaryParts.push(t("page.library.info.virtualDir", { path: result.virtual_dir }));
    }
    if (result.unsupported.length > 0) {
      summaryParts.push(t("page.library.info.unsupported", { count: result.unsupported.length }));
    }
    if (result.skipped.length > 0) {
      summaryParts.push(t("page.library.info.skipped", { count: result.skipped.length }));
    }
    infoMessage.value = summaryParts.join(" · ");
    await loadDirs();
    await loadStatus();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.library.error.importPaths"));
    console.error("[DocMind] importPaths failed", error);
  } finally {
    importing.value = false;
    dragActive.value = false;
    busyPath.value = null;
  }
};

onMounted(loadDirs);
onMounted(loadStatus);

onMounted(() => {
  void installIndexRefreshListener();
  void installDocumentRefreshListener();
  void installFileDropListener();
});

onBeforeUnmount(() => {
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
  refreshJobResolvers.clear();
  refreshJobBufferedEvents.clear();
  documentRefreshResolvers.clear();
  documentRefreshBufferedEvents.clear();
});
</script>

<template>
  <div class="flex h-full min-h-0 flex-col bg-slate-50 text-slate-900">
    <header class="flex h-12 items-center justify-between gap-4 border-b border-slate-200 bg-white px-5">
      <div class="min-w-0">
        <h1 class="text-base font-semibold tracking-tight text-slate-950">{{ t("page.library.title") }}</h1>
        <p class="mt-0.5 text-xs text-slate-500">{{ t("page.library.subtitle") }}</p>
      </div>
      <button
        class="inline-flex items-center gap-2 rounded-md bg-slate-900 px-3 py-1.5 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-70"
        :disabled="refreshing || importing"
        @click="chooseAndAddDir"
      >
        <FolderPlus :size="16" />
        {{ t("page.library.btn.addDir") }}
      </button>
    </header>

    <main class="relative min-h-0 flex-1 overflow-y-auto p-4">
      <div
        class="mb-4 rounded-md border border-dashed px-4 py-3 text-xs transition"
        :class="dragActive
          ? 'border-indigo-300 bg-indigo-50 text-indigo-700'
          : 'border-slate-200 bg-white text-slate-400'"
      >
        <div class="flex items-center gap-2">
          <UploadCloud :size="14" />
          <span class="font-medium">
            {{ dragActive ? t("page.library.dropActive") : t("page.library.dropHint") }}
          </span>
          <span v-if="importing" class="ml-auto inline-flex items-center gap-1 text-slate-500">
            <Loader2 :size="12" class="animate-spin" />
            {{ t("page.library.importing") }}
          </span>
        </div>
      </div>

      <DocMindTaskCard
        :task="status?.current_task ?? null"
        :title="t('page.library.taskTitle')"
        :description="status?.current_task?.details ?? t('page.library.taskSyncing')"
        :badge-label="taskDisplayState.label"
        :badge-tone="taskDisplayState.tone"
        :badge-spinning="taskDisplayState.spinning"
        class="mb-4"
      />

      <div v-if="errorMessage" class="mb-3 rounded-md border border-red-100 bg-red-50 px-4 py-2.5 text-sm text-red-700">
        {{ errorMessage }}
      </div>

      <div v-if="infoMessage" class="mb-3 rounded-md border border-emerald-100 bg-emerald-50 px-4 py-2.5 text-sm text-emerald-700">
        {{ infoMessage }}
      </div>

        <div class="mb-3 flex items-center justify-between border-b border-slate-200 bg-white px-4 py-2">
        <div class="text-[10px] font-semibold uppercase tracking-[0.16em] text-slate-500">{{ t("page.library.emptyState.title") }}</div>
        <DocMindBadge tone="default">
          <FolderOpen class="mr-1" :size="13" />
          {{ explicitIndexDirCount }}
        </DocMindBadge>
      </div>

      <div v-if="loading" class="rounded-md border border-dashed border-slate-200 bg-white px-4 py-6 text-xs text-slate-400">
        {{ t("page.library.loading") }}
      </div>

      <div v-else-if="dirs.length === 0" class="rounded-md border border-dashed border-slate-200 bg-white px-4 py-6 text-xs text-slate-400">
        <div class="text-[10px] font-semibold uppercase tracking-[0.16em] text-slate-500">{{ t("page.library.emptyState.title") }}</div>
        <div class="mt-1.5">{{ t("page.library.emptyState.subtitle") }}</div>
      </div>

      <DocMindIndexTree
        v-else
        :rows="visibleDirRows"
        :selected-path="''"
        :path-tooltip="true"
        :selectable="false"
        :virtual-label="t('common.virtualDir')"
        :expand-title="t('sidebar.expand')"
        :collapse-title="t('sidebar.collapse')"
        density="compact"
      >
        <template #meta="{ row }">
          <span class="truncate">{{ t("page.chunks.dirDocs", { docs: row.dir.docs, chunks: row.dir.chunks.toLocaleString() }) }}</span>
        </template>
        <template #status="{ row }">
          <div class="flex items-center gap-1.5">
            <span
              class="rounded-full px-1.5 py-0.5 text-[10px]"
              :class="row.dir.enabled ? 'bg-emerald-50 text-emerald-700' : 'bg-slate-100 text-slate-500'"
            >
              {{ row.dir.enabled ? t("common.enabled") : t("common.disabled") }}
            </span>
          </div>
        </template>
        <template #actions="{ row }">
          <div class="flex items-center gap-1.5">
            <button
              class="rounded-md border border-slate-200 p-2 text-slate-500 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
              :disabled="busyPath === row.dir.path || row.isVirtual"
              :title="t('page.library.action.quickLook')"
              @click.stop="quickLookDir(row.dir.path)"
            >
              <Eye :size="14" />
            </button>
            <button
              class="rounded-md border border-slate-200 p-2 text-slate-500 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
              :disabled="busyPath === row.dir.path"
              :title="t('page.library.action.copyPath')"
              @click.stop="copyDirPath(row.dir.path)"
            >
              <Copy :size="14" />
            </button>
            <button
              class="rounded-md border border-slate-200 p-2 text-slate-500 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
              :disabled="busyPath === row.dir.path"
              :title="t('page.library.action.copyCitation')"
              @click.stop="copyDirCitation(row)"
            >
              <FileText :size="14" />
            </button>
            <button
              class="rounded-md border border-slate-200 p-2 text-slate-500 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
              :disabled="busyPath === row.dir.path || !row.dir.is_explicit"
              :title="t('page.library.status.indexing')"
              @click.stop="refreshSingleDir(row.dir.path)"
            >
              <RefreshCw :size="14" />
            </button>
            <button
              class="rounded-md border border-slate-200 p-2 text-slate-500 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
              :disabled="busyPath === row.dir.path || !row.dir.is_explicit"
              :title="row.dir.enabled ? t('common.disabled') : t('common.enabled')"
              @click.stop="toggleDir(row.dir)"
            >
              <component :is="row.dir.enabled ? ToggleRight : ToggleLeft" :size="14" />
            </button>
            <button
              class="rounded-md border border-slate-200 p-2 text-slate-500 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
              :disabled="busyPath === row.dir.path || !row.dir.is_explicit"
              :title="t('common.clear')"
              @click.stop="removeDir(row.dir.path)"
            >
              <X :size="14" />
            </button>
          </div>
        </template>
      </DocMindIndexTree>
    </main>
  </div>
</template>
