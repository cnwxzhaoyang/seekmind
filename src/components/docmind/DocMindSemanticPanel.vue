<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { RefreshCw, Save, Search, Sparkles } from "lucide-vue-next";
import { listen } from "@tauri-apps/api/event";
import DocMindBadge from "./DocMindBadge.vue";
import { docmindApi, formatDocmindError } from "../../services/docmindApi";
import type {
  EmbeddingModelView,
  SemanticDebugView,
  SemanticModelStatusView,
  SemanticRebuildProgressView,
} from "../../types/docmind";

const { t } = useI18n();

const semanticStatus = ref<SemanticModelStatusView | null>(null);
const embeddingModels = ref<EmbeddingModelView[]>([]);
const selectedEmbeddingModelId = ref("");
const semanticQuery = ref("");
const semanticDebug = ref<SemanticDebugView | null>(null);
const semanticRebuildProgress = ref<SemanticRebuildProgressView | null>(null);
const semanticRebuildJobId = ref("");
const loadingSemanticDebug = ref(false);
const loading = ref(false);
const saving = ref(false);
const errorMessage = ref("");
const infoMessage = ref("");
let unlistenSemanticProgress: null | (() => void) = null;

const loadSemanticStatus = async () => {
  try {
    semanticStatus.value = await docmindApi.getEmbeddingModelStatus();
    selectedEmbeddingModelId.value = semanticStatus.value.model.id;
  } catch (error) {
    console.error("[DocMind] getEmbeddingModelStatus failed", error);
  }
};

const loadEmbeddingModels = async () => {
  try {
    embeddingModels.value = await docmindApi.listEmbeddingModels();
  } catch (error) {
    console.error("[DocMind] listEmbeddingModels failed", error);
  }
};

const refreshAll = async () => {
  loading.value = true;
  try {
    await loadSemanticStatus();
    await loadEmbeddingModels();
  } finally {
    loading.value = false;
  }
};

const rebuildSemanticEmbeddings = async () => {
  saving.value = true;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    const started = await docmindApi.rebuildSemanticEmbeddings();
    semanticStatus.value = started.status;
    selectedEmbeddingModelId.value = started.status.model.id;
    semanticRebuildJobId.value = started.job_id;
    semanticRebuildProgress.value = {
      job_id: started.job_id,
      state: "running",
      message: t("semantic.info.rebuildStarted"),
      source: "rebuild",
      model: started.status.model,
      total_chunks: started.status.sqlite_chunks,
      processed_chunks: 0,
      embedded_chunks: 0,
      current_document: "",
      current_chunk: "",
      percent: 0,
      last_error: "",
      updated_at: new Date().toISOString(),
    };
    infoMessage.value = t("semantic.info.rebuildStarted");
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("semantic.error.rebuildFailed"));
    console.error("[DocMind] rebuildSemanticEmbeddings failed", error);
  } finally {
    saving.value = false;
  }
};

const setDefaultEmbeddingModel = async () => {
  errorMessage.value = "";
  infoMessage.value = "";

  if (!selectedEmbeddingModelId.value) {
    errorMessage.value = t("semantic.error.selectModel");
    return;
  }

  saving.value = true;
  try {
    semanticStatus.value = await docmindApi.setDefaultEmbeddingModel(selectedEmbeddingModelId.value);
    infoMessage.value = t("semantic.info.modelUpdated");
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("semantic.error.switchModelFailed"));
    console.error("[DocMind] setDefaultEmbeddingModel failed", error);
  } finally {
    saving.value = false;
    await loadEmbeddingModels();
  }
};

const runSemanticDebug = async () => {
  loadingSemanticDebug.value = true;
  errorMessage.value = "";

  try {
    semanticDebug.value = await docmindApi.getSemanticDebugReport(semanticQuery.value, 8);
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("semantic.error.debugFailed"));
    console.error("[DocMind] getSemanticDebugReport failed", error);
  } finally {
    loadingSemanticDebug.value = false;
  }
};

const canSwitchModel = computed(() => selectedEmbeddingModelId.value !== semanticStatus.value?.model.id);
const semanticIndexStatusLabel = computed(() => {
  const status = semanticStatus.value?.index_status || "idle";
  switch (status) {
    case "idle":
      return t("status.idle");
    case "running":
      return t("status.running");
    case "completed":
      return t("status.completed");
    case "failed":
      return t("status.failed");
    default:
      return status;
  }
});

const installSemanticProgressListener = async () => {
  if (unlistenSemanticProgress) {
    return;
  }

  unlistenSemanticProgress = await listen<SemanticRebuildProgressView>(
    "docmind:semantic:rebuild-progress",
    (event) => {
      const payload = event.payload;
      if (payload.source !== "rebuild") {
        return;
      }
      if (semanticRebuildJobId.value && payload.job_id !== semanticRebuildJobId.value) {
        return;
      }

      semanticRebuildProgress.value = payload;
      semanticStatus.value = {
        model: payload.model,
        sqlite_chunks: payload.total_chunks,
        embedded_chunks: payload.embedded_chunks,
        needs_rebuild: payload.state !== "completed",
        last_indexed_at: payload.updated_at,
        last_error: payload.last_error,
        index_status: payload.state,
      };

      if (payload.state === "completed") {
        semanticRebuildJobId.value = "";
        infoMessage.value = t("semantic.info.rebuilt");
        void refreshAll();
      } else if (payload.state === "failed") {
        semanticRebuildJobId.value = "";
        errorMessage.value = payload.last_error || payload.message || t("semantic.error.rebuildFailed");
        void refreshAll();
      }
    },
  );
};

onMounted(async () => {
  await refreshAll();
  await installSemanticProgressListener();
});

onBeforeUnmount(() => {
  if (unlistenSemanticProgress) {
    unlistenSemanticProgress();
    unlistenSemanticProgress = null;
  }
});
</script>

<template>
  <section class="rounded-lg border border-slate-200 bg-white p-4">
    <div class="mb-3 flex items-center justify-between">
      <div>
        <div class="text-[10px] font-semibold uppercase tracking-[0.16em] text-slate-500">{{ t("semantic.title") }}</div>
        <div class="mt-1 text-xs text-slate-500">{{ t("semantic.desc") }}</div>
      </div>
      <DocMindBadge tone="default">{{ semanticIndexStatusLabel }}</DocMindBadge>
    </div>

    <div v-if="errorMessage" class="mb-3 rounded-md border border-red-100 bg-red-50 px-4 py-2.5 text-xs text-red-700">
      {{ errorMessage }}
    </div>

    <div v-if="infoMessage" class="mb-3 rounded-md border border-emerald-100 bg-emerald-50 px-4 py-2.5 text-xs text-emerald-700">
      {{ infoMessage }}
    </div>

    <div v-if="loading" class="rounded-md border border-dashed border-slate-200 bg-white px-4 py-5 text-xs text-slate-400">
      {{ t("semantic.loading") }}
    </div>

    <div v-else-if="semanticStatus" class="space-y-4 text-sm">
      <label class="block">
        <div class="mb-2 text-[10px] font-semibold uppercase tracking-[0.16em] text-slate-500">{{ t("semantic.defaultModel") }}</div>
        <select
          v-model="selectedEmbeddingModelId"
          class="w-full rounded-lg border border-slate-200 bg-slate-50 px-4 py-3 text-sm text-slate-900 outline-none transition focus:border-slate-400 focus:bg-white"
        >
          <option v-for="model in embeddingModels" :key="model.id" :value="model.id">
            {{ model.name }} · {{ model.provider }} · {{ t("semantic.dimension", { dim: model.dimension }) }}
          </option>
        </select>
      </label>

      <div v-if="semanticRebuildProgress" class="rounded-lg border border-slate-200 bg-slate-50 px-4 py-4">
        <div class="flex items-center justify-between gap-3">
          <div>
            <div class="text-[10px] font-semibold uppercase tracking-[0.16em] text-slate-500">{{ t("semantic.rebuildProgress") }}</div>
            <div class="mt-1 text-sm font-medium text-slate-900">
              {{ semanticRebuildProgress.state === "completed" ? t("semantic.completed") : semanticRebuildProgress.message }}
            </div>
          </div>
          <DocMindBadge tone="default">{{ semanticRebuildProgress.percent }}%</DocMindBadge>
        </div>

        <div class="mt-3 h-2 overflow-hidden rounded-full bg-slate-200">
          <div
            class="h-full rounded-full bg-slate-900 transition-all"
            :style="{ width: `${semanticRebuildProgress.percent}%` }"
          />
        </div>

        <div class="mt-3 grid gap-3 text-xs text-slate-600 md:grid-cols-2">
          <div>
            <div class="text-slate-500">{{ t("semantic.currentDirDoc") }}</div>
            <div class="mt-1 break-all text-slate-900">
              {{ semanticRebuildProgress.current_document || t("semantic.none") }}
            </div>
          </div>
          <div>
            <div class="text-slate-500">{{ t("semantic.currentChunk") }}</div>
            <div class="mt-1 break-all text-slate-900">
              {{ semanticRebuildProgress.current_chunk || t("semantic.none") }}
            </div>
          </div>
          <div>
            <div class="text-slate-500">{{ t("semantic.processed") }}</div>
            <div class="mt-1 text-slate-900">
              {{ semanticRebuildProgress.processed_chunks }} / {{ semanticRebuildProgress.total_chunks }}
            </div>
          </div>
          <div>
            <div class="text-slate-500">{{ t("semantic.embedded") }}</div>
            <div class="mt-1 text-slate-900">{{ semanticRebuildProgress.embedded_chunks }}</div>
          </div>
        </div>
      </div>

      <div class="grid grid-cols-2 gap-3">
        <div class="rounded-lg bg-slate-50 px-4 py-3">
          <div class="text-xs text-slate-500">{{ t("semantic.embeddedShort") }}</div>
          <div class="mt-1 font-medium text-slate-900">{{ semanticStatus.embedded_chunks }}</div>
        </div>
        <div class="rounded-lg bg-slate-50 px-4 py-3">
          <div class="text-xs text-slate-500">{{ t("semantic.pendingRebuild") }}</div>
          <div class="mt-1 font-medium text-slate-900">
            {{ semanticStatus.needs_rebuild ? t("semantic.yes") : t("semantic.no") }}
          </div>
        </div>
      </div>

      <div class="rounded-lg bg-slate-50 px-4 py-3 text-xs text-slate-600">
        <div>{{ t("semantic.model", { name: semanticStatus.model.name }) }}</div>
        <div class="mt-1">{{ t("semantic.provider", { provider: semanticStatus.model.provider, dim: semanticStatus.model.dimension }) }}</div>
        <div class="mt-1">{{ t("semantic.availability", { status: semanticStatus.model.available ? t("semantic.yes") : t("semantic.no") }) }}</div>
        <div class="mt-1">{{ t("semantic.lastIndexed", { time: semanticStatus.last_indexed_at || t("semantic.none") }) }}</div>
        <div class="mt-1">{{ t("semantic.lastError", { msg: semanticStatus.last_error || t("semantic.noError") }) }}</div>
      </div>

      <div class="grid gap-2 md:grid-cols-2">
        <button
          class="inline-flex items-center justify-center gap-2 rounded-lg border border-slate-200 bg-white px-4 py-3 text-sm font-medium text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
          :disabled="saving || !canSwitchModel"
          @click="setDefaultEmbeddingModel"
        >
          <Save :size="16" />
          {{ t("semantic.btn.setDefault") }}
        </button>
        <button
          class="inline-flex items-center justify-center gap-2 rounded-lg border border-slate-200 bg-white px-4 py-3 text-sm font-medium text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
          :disabled="saving"
          @click="rebuildSemanticEmbeddings"
        >
          <Sparkles :size="16" />
          {{ saving ? t("semantic.btn.processing") : t("semantic.btn.rebuild") }}
        </button>
      </div>
    </div>

    <div class="mt-4 rounded-lg border border-slate-200 bg-white p-4">
      <div class="mb-3 flex items-center justify-between">
        <div>
          <div class="text-[10px] font-semibold uppercase tracking-[0.16em] text-slate-500">{{ t("semantic.debug.title") }}</div>
          <div class="mt-1 text-xs text-slate-500">{{ t("semantic.debug.desc") }}</div>
        </div>
        <DocMindBadge tone="success">{{ t("semantic.debug.onlyLocal") }}</DocMindBadge>
      </div>

      <div class="flex gap-2">
        <input
          v-model="semanticQuery"
          type="text"
          class="min-w-0 flex-1 rounded-lg border border-slate-200 bg-slate-50 px-4 py-3 text-sm text-slate-900 outline-none transition focus:border-slate-400 focus:bg-white"
          :placeholder="t('semantic.debug.placeholder')"
          @keyup.enter="runSemanticDebug"
        />
        <button
          class="inline-flex items-center gap-2 rounded-lg bg-slate-900 px-4 py-3 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-70"
          :disabled="loadingSemanticDebug"
          @click="runSemanticDebug"
        >
          <Search :size="16" />
          {{ loadingSemanticDebug ? t("semantic.debug.debugging") : t("semantic.debug.debug") }}
        </button>
      </div>

      <div v-if="semanticDebug" class="mt-4 space-y-3">
        <div class="grid grid-cols-2 gap-3 text-sm">
          <div class="rounded-lg bg-slate-50 px-4 py-3">
            <div class="text-xs text-slate-500">{{ t("semantic.debug.vectorDim") }}</div>
            <div class="mt-1 font-medium text-slate-900">{{ semanticDebug.query_vector_dim }}</div>
          </div>
          <div class="rounded-lg bg-slate-50 px-4 py-3">
            <div class="text-xs text-slate-500">{{ t("semantic.debug.hitCount") }}</div>
            <div class="mt-1 font-medium text-slate-900">{{ semanticDebug.hit_count }}</div>
          </div>
        </div>

        <div class="rounded-lg bg-slate-50 px-4 py-3 text-xs text-slate-600">
          <div>{{ t("semantic.debug.normalizedQuery", { query: semanticDebug.normalized_query || t("semantic.debug.empty") }) }}</div>
          <div class="mt-1">{{ t("semantic.model", { name: semanticDebug.model.name }) }} / {{ semanticDebug.model.provider }}</div>
          <div class="mt-1">{{ t("semantic.debug.status", { status: semanticDebug.index_status || "idle", error: semanticDebug.last_error || t("semantic.noError") }) }}</div>
        </div>

        <div v-if="semanticDebug.hits.length > 0" class="space-y-2">
          <div
            v-for="hit in semanticDebug.hits.slice(0, 3)"
            :key="hit.chunk_id"
            class="rounded-lg border border-slate-200 bg-white px-4 py-3 text-sm"
          >
            <div class="flex items-center justify-between gap-3">
              <div class="font-medium text-slate-900">{{ hit.file_name }}</div>
              <DocMindBadge tone="default">{{ hit.score.toFixed(3) }}</DocMindBadge>
            </div>
            <div class="mt-1 text-xs text-slate-500">{{ hit.title_path || hit.heading || t("semantic.debug.noHitHeading") }}</div>
            <div class="mt-2 line-clamp-2 text-slate-600">{{ hit.snippet }}</div>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>
