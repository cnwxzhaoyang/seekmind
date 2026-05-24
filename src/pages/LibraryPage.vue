<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderPlus, FolderOpen, CheckCircle2, Loader2, RefreshCw, X, ToggleLeft, ToggleRight } from "lucide-vue-next";
import DocMindBadge from "../components/docmind/DocMindBadge.vue";
import DocMindTaskCard from "../components/docmind/DocMindTaskCard.vue";
import { docmindApi, formatDocmindError } from "../services/docmindApi";
import type { IndexDirView, IndexRefreshProgressView, IndexStatusView } from "../types/docmind";

const { t } = useI18n();

const dirs = ref<IndexDirView[]>([]);
const status = ref<IndexStatusView | null>(null);
const loading = ref(false);
const refreshing = ref(false);
const busyPath = ref<string | null>(null);
const errorMessage = ref("");
const infoMessage = ref("");
const refreshJobResolvers = new Map<string, (payload: IndexRefreshProgressView) => void>();
const refreshJobBufferedEvents = new Map<string, IndexRefreshProgressView>();
let unlistenIndexRefreshProgress: null | (() => void) = null;

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
    },
  );
};

void installIndexRefreshListener();

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

onMounted(loadDirs);
onMounted(loadStatus);

onMounted(() => {
  void installIndexRefreshListener();
});

onBeforeUnmount(() => {
  if (unlistenIndexRefreshProgress) {
    unlistenIndexRefreshProgress();
    unlistenIndexRefreshProgress = null;
  }
  refreshJobResolvers.clear();
  refreshJobBufferedEvents.clear();
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
        :disabled="refreshing"
        @click="chooseAndAddDir"
      >
        <FolderPlus :size="16" />
        {{ t("page.library.btn.addDir") }}
      </button>
    </header>

    <main class="min-h-0 flex-1 overflow-y-auto p-4">
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
          {{ dirs.length }}
        </DocMindBadge>
      </div>

      <div v-if="loading" class="rounded-md border border-dashed border-slate-200 bg-white px-4 py-6 text-xs text-slate-400">
        {{ t("page.library.loading") }}
      </div>

      <div v-else-if="dirs.length === 0" class="rounded-md border border-dashed border-slate-200 bg-white px-4 py-6 text-xs text-slate-400">
        <div class="text-[10px] font-semibold uppercase tracking-[0.16em] text-slate-500">{{ t("page.library.emptyState.title") }}</div>
        <div class="mt-1.5">{{ t("page.library.emptyState.subtitle") }}</div>
      </div>

      <div v-else class="space-y-2">
        <div
          v-for="dir in dirs"
          :key="dir.path"
          class="rounded-md border border-slate-200 bg-white px-3 py-3"
        >
          <div class="flex items-start justify-between gap-3">
            <div class="min-w-0 flex-1">
              <div class="truncate text-sm font-medium text-slate-950">{{ dir.path }}</div>
              <div class="mt-1 text-[11px] text-slate-500">{{ t("page.chunks.dirDocs", { docs: dir.docs, chunks: dir.chunks.toLocaleString() }) }}</div>
              <div class="mt-2 inline-flex items-center gap-2 text-[11px] text-slate-500">
                <span>{{ dir.enabled ? t("common.enabled") : t("common.disabled") }}</span>
                <span>·</span>
                <span>{{ dir.status }}</span>
              </div>
            </div>

            <div class="flex items-center gap-1.5">
              <button
                class="rounded-md border border-slate-200 p-2 text-slate-500 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
                :disabled="busyPath === dir.path"
                :title="t('page.library.status.indexing')"
                @click="refreshSingleDir(dir.path)"
              >
                <RefreshCw :size="14" />
              </button>
              <button
                class="rounded-md border border-slate-200 p-2 text-slate-500 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
                :disabled="busyPath === dir.path"
                :title="dir.enabled ? t('common.disabled') : t('common.enabled')"
                @click="toggleDir(dir)"
              >
                <component :is="dir.enabled ? ToggleRight : ToggleLeft" :size="14" />
              </button>
              <button
                class="rounded-md border border-slate-200 p-2 text-slate-500 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
                :disabled="busyPath === dir.path"
                title="删除"
                @click="removeDir(dir.path)"
              >
                <X :size="14" />
              </button>
            </div>
          </div>
        </div>
      </div>
    </main>
  </div>
</template>
