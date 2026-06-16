<script setup lang="ts">
/**
 * @author MorningSun
 * @CreatedDate 2026/06/02
 * @Description 设置页中的语义检索配置面板，负责模型选择、重建和调试。
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { RefreshCw, Save, Search, Sparkles } from "lucide-vue-next";
import { listen } from "@tauri-apps/api/event";
import SeekMindBadge from "./SeekMindBadge.vue";
import SeekMindToast from "./SeekMindToast.vue";
import { seekMindApi, formatSeekMindError } from "../../services/seekMindApi";
import { useInfoMessage } from "../../composables/useInfoMessage";
import type {
  EmbeddingModelView,
  SemanticDebugView,
  SemanticModelStatusView,
  SemanticRebuildProgressView,
} from "../../types/SeekMind";

const { t } = useI18n();
const emit = defineEmits<{
  (event: "status-changed", status: SemanticModelStatusView | null): void;
}>();

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
const semanticProbeError = ref("");
const { infoMessage } = useInfoMessage();
let unlistenSemanticProgress: null | (() => void) = null;
let unlistenStorageReset: null | (() => void) = null;

const buildFallbackSemanticStatus = (message: string): SemanticModelStatusView | null => {
  const fallbackModel = embeddingModels.value.find((model) => model.is_default) ?? embeddingModels.value[0];
  if (!fallbackModel) {
    return null;
  }

  // 修复：如果运行时探测失败，语义面板不能直接空白；至少要展示一个“不可用”的占位状态，避免用户误以为 embedding 正常。
  return {
    model: {
      ...fallbackModel,
      available: false,
    },
    sqlite_chunks: 0,
    embedded_chunks: 0,
    needs_rebuild: true,
    last_indexed_at: "",
    last_error: message,
    index_status: "unknown",
  };
};

const loadSemanticStatus = async () => {
  semanticProbeError.value = "";
  try {
    semanticStatus.value = await seekMindApi.getEmbeddingModelStatus();
    selectedEmbeddingModelId.value = semanticStatus.value.model.id;
  } catch (error) {
    semanticProbeError.value = formatSeekMindError(error, t("semantic.runtime.probeFailed"));
    semanticStatus.value = buildFallbackSemanticStatus(semanticProbeError.value);
    if (semanticStatus.value) {
      selectedEmbeddingModelId.value = semanticStatus.value.model.id;
    }
    console.error("[SeekMind] getEmbeddingModelStatus failed", error);
  }
};

const loadEmbeddingModels = async () => {
  try {
    embeddingModels.value = await seekMindApi.listEmbeddingModels();
  } catch (error) {
    console.error("[SeekMind] listEmbeddingModels failed", error);
  }
};

const refreshAll = async () => {
  loading.value = true;
  try {
    await loadEmbeddingModels();
    await loadSemanticStatus();
    if (!semanticStatus.value && embeddingModels.value.length > 0) {
      semanticStatus.value = buildFallbackSemanticStatus(
        semanticProbeError.value || t("semantic.runtime.unavailableHint"),
      );
      if (semanticStatus.value) {
        selectedEmbeddingModelId.value = semanticStatus.value.model.id;
      }
    }
  } finally {
    loading.value = false;
  }
};

const listenStorageReset = async () => {
  if (unlistenStorageReset) {
    return;
  }

  unlistenStorageReset = await listen("seekmind:storage-reset", () => {
    // 修复：清空索引后，语义状态面板需要重新拉取状态，避免继续显示旧的可用/正常状态。
    void refreshAll();
  });
};

const rebuildSemanticEmbeddings = async () => {
  saving.value = true;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    const started = await seekMindApi.rebuildSemanticEmbeddings();
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
    errorMessage.value = formatSeekMindError(error, t("semantic.error.rebuildFailed"));
    console.error("[SeekMind] rebuildSemanticEmbeddings failed", error);
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
    semanticStatus.value = await seekMindApi.setDefaultEmbeddingModel(selectedEmbeddingModelId.value);
    infoMessage.value = t("semantic.info.modelUpdated");
  } catch (error) {
    errorMessage.value = formatSeekMindError(error, t("semantic.error.switchModelFailed"));
    console.error("[SeekMind] setDefaultEmbeddingModel failed", error);
  } finally {
    saving.value = false;
    await loadEmbeddingModels();
  }
};

const runSemanticDebug = async () => {
  loadingSemanticDebug.value = true;
  errorMessage.value = "";

  try {
    semanticDebug.value = await seekMindApi.getSemanticDebugReport(semanticQuery.value, 8);
  } catch (error) {
    errorMessage.value = formatSeekMindError(error, t("semantic.error.debugFailed"));
    console.error("[SeekMind] getSemanticDebugReport failed", error);
  } finally {
    loadingSemanticDebug.value = false;
  }
};

const canSwitchModel = computed(() => selectedEmbeddingModelId.value !== semanticStatus.value?.model.id);
const semanticRuntimeState = computed(() => {
  if (loading.value && !semanticStatus.value) {
    return {
      tone: "default" as const,
      label: t("semantic.runtime.detecting"),
      detail: t("semantic.runtime.detectingHint"),
    };
  }

  if (!semanticStatus.value) {
    return {
      tone: "warning" as const,
      label: t("semantic.runtime.unavailable"),
      detail: semanticProbeError.value || t("semantic.runtime.unavailableHint"),
    };
  }

  if (semanticStatus.value.model.available) {
    return {
      tone: "success" as const,
      label: t("semantic.runtime.available"),
      detail: semanticStatus.value.last_error.trim()
        ? t("semantic.runtime.availableWithNote", {
            note: semanticStatus.value.last_error,
          })
        : t("semantic.runtime.availableHint"),
    };
  }

  return {
    tone: "warning" as const,
    label: t("semantic.runtime.unavailable"),
    detail: semanticStatus.value.last_error || semanticProbeError.value || t("semantic.runtime.unavailableHint"),
  };
});
const embeddingActionsDisabled = computed(() => semanticRuntimeState.value.tone !== "success");

watch(semanticStatus, (status) => {
  emit("status-changed", status);
});

const installSemanticProgressListener = async () => {
  if (unlistenSemanticProgress) {
    return;
  }

  unlistenSemanticProgress = await listen<SemanticRebuildProgressView>(
    "seekmind:semantic:rebuild-progress",
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
  await listenStorageReset();
});

onBeforeUnmount(() => {
  if (unlistenSemanticProgress) {
    unlistenSemanticProgress();
    unlistenSemanticProgress = null;
  }
  if (unlistenStorageReset) {
    unlistenStorageReset();
    unlistenStorageReset = null;
  }
});
</script>

<template>
  <section class="settings-card-shell">
    <div class="settings-card-head">
      <div class="settings-card-head-left">
        <span class="settings-card-icon settings-card-icon--plain">
          <Sparkles :size="18" />
        </span>
        <div class="min-w-0">
          <div class="settings-card-title">{{ t("semantic.title") }}</div>
        </div>
      </div>
      <div class="flex flex-wrap justify-end gap-2">
        <!-- 修复：只保留一个语义可用性入口，避免状态与按钮可用性出现分裂。 -->
        <SeekMindBadge :tone="semanticRuntimeState.tone" :title="semanticRuntimeState.detail">
          {{ semanticRuntimeState.label }}
        </SeekMindBadge>
      </div>
    </div>

    <div class="settings-card-body">
      <SeekMindToast v-if="errorMessage" :message="errorMessage" tone="error" />
      <SeekMindToast v-if="infoMessage" :message="infoMessage" tone="success" />

      <div v-if="loading" class="rounded-md border border-dashed border-default bg-surface px-4 py-5 text-sm text-muted">
        {{ t("semantic.loading") }}
      </div>

      <div v-else-if="semanticStatus" class="space-y-3 text-sm">
        <label class="block">
          <div class="mb-1.5 seekmind-section-label">{{ t("semantic.defaultModel") }}</div>
          <select
            v-model="selectedEmbeddingModelId"
            class="seekmind-select w-full px-4 py-2.5 text-sm outline-none transition"
          >
            <option v-for="model in embeddingModels" :key="model.id" :value="model.id">
              {{ model.name }} · {{ model.provider }} · {{ t("semantic.dimension", { dim: model.dimension }) }}
            </option>
          </select>
        </label>

      <div v-if="semanticRebuildProgress" class="rounded-lg border border-default bg-panel px-4 py-3.5">
        <div class="flex items-center justify-between gap-3">
          <div>
            <div class="seekmind-section-label">{{ t("semantic.rebuildProgress") }}</div>
            <div class="mt-1 text-sm font-medium text-primary">
              {{ semanticRebuildProgress.state === "completed" ? t("semantic.completed") : semanticRebuildProgress.message }}
            </div>
          </div>
          <SeekMindBadge tone="default">{{ semanticRebuildProgress.percent }}%</SeekMindBadge>
        </div>

        <div class="mt-3 h-2 overflow-hidden rounded-full bg-surface-active">
          <div
            class="h-full rounded-full bg-accent transition-all"
            :style="{ width: `${semanticRebuildProgress.percent}%` }"
          />
        </div>

        <div class="mt-3 grid gap-3 text-sm text-secondary md:grid-cols-2">
          <div>
            <div class="seekmind-item-meta">{{ t("semantic.currentDirDoc") }}</div>
            <div class="mt-1 truncate text-primary" :title="semanticRebuildProgress.current_document || t('semantic.none')">
              {{ semanticRebuildProgress.current_document || t("semantic.none") }}
            </div>
          </div>
          <div>
            <div class="seekmind-item-meta">{{ t("semantic.currentChunk") }}</div>
            <div class="mt-1 truncate text-primary" :title="semanticRebuildProgress.current_chunk || t('semantic.none')">
              {{ semanticRebuildProgress.current_chunk || t("semantic.none") }}
            </div>
          </div>
          <div>
            <div class="seekmind-item-meta">{{ t("semantic.processed") }}</div>
            <div class="mt-1 seekmind-metric-value text-primary">
              {{ semanticRebuildProgress.processed_chunks }} / {{ semanticRebuildProgress.total_chunks }}
            </div>
          </div>
          <div>
            <div class="seekmind-item-meta">{{ t("semantic.embedded") }}</div>
            <div class="mt-1 seekmind-metric-value text-primary">{{ semanticRebuildProgress.embedded_chunks }}</div>
          </div>
        </div>
      </div>

      <div class="grid grid-cols-2 gap-2.5">
        <div class="rounded-lg bg-panel px-4 py-2.5">
          <div class="seekmind-item-meta">{{ t("semantic.embeddedShort") }}</div>
          <div class="mt-1 seekmind-metric-value text-primary">{{ semanticStatus.embedded_chunks }}</div>
        </div>
        <div class="rounded-lg bg-panel px-4 py-2.5">
          <div class="seekmind-item-meta">{{ t("semantic.pendingRebuild") }}</div>
          <div class="mt-1 seekmind-metric-value text-primary">
            {{ semanticStatus.needs_rebuild ? t("semantic.yes") : t("semantic.no") }}
          </div>
        </div>
      </div>

      <div class="rounded-lg bg-panel px-4 py-2.5 text-sm text-secondary">
        <div>{{ t("semantic.model", { name: semanticStatus.model.name }) }}</div>
        <div class="mt-1">{{ t("semantic.provider", { provider: semanticStatus.model.provider, dim: semanticStatus.model.dimension }) }}</div>
        <div class="mt-1">{{ t("semantic.availability", { status: semanticStatus.model.available ? t("semantic.yes") : t("semantic.no") }) }}</div>
        <div class="mt-1">{{ t("semantic.lastIndexed", { time: semanticStatus.last_indexed_at || t("semantic.none") }) }}</div>
        <div class="mt-1">{{ t("semantic.lastError", { msg: semanticStatus.last_error || t("semantic.noError") }) }}</div>
      </div>

      <div class="grid gap-2 md:grid-cols-2">
        <button
          class="inline-flex items-center justify-center gap-2 rounded-lg border border-default bg-surface px-4 py-3 text-sm font-medium text-secondary hover:bg-surface-hover disabled:cursor-not-allowed disabled:opacity-70"
          :disabled="saving || !canSwitchModel"
          @click="setDefaultEmbeddingModel"
        >
          <Save :size="16" />
          {{ t("semantic.btn.setDefault") }}
        </button>
        <button
          class="inline-flex items-center justify-center gap-2 rounded-lg border border-default bg-surface px-4 py-3 text-sm font-medium text-secondary hover:bg-surface-hover disabled:cursor-not-allowed disabled:opacity-70"
          :disabled="saving || embeddingActionsDisabled"
          @click="rebuildSemanticEmbeddings"
        >
          <Sparkles :size="16" />
          {{ saving ? t("semantic.btn.processing") : t("semantic.btn.rebuild") }}
        </button>
      </div>
      </div>

      <!-- 修复：调试区仍然属于 settings-card-body，避免模板层级缺失。 -->
      <div class="mt-4 rounded-lg border border-default bg-panel p-4">
        <div class="mb-3 flex items-center justify-between">
          <div>
            <div class="seekmind-section-label">{{ t("semantic.debug.title") }}</div>
            <div class="seekmind-item-meta mt-1">{{ t("semantic.debug.desc") }}</div>
          </div>
          <SeekMindBadge tone="success">{{ t("semantic.debug.onlyLocal") }}</SeekMindBadge>
        </div>

        <div class="flex gap-2">
          <input
            v-model="semanticQuery"
            type="text"
            class="min-w-0 flex-1 rounded-lg border border-default bg-input px-4 py-3 text-sm text-primary outline-none transition focus:border-[var(--color-text-dim)] focus:bg-surface"
            :placeholder="t('semantic.debug.placeholder')"
            @keyup.enter="runSemanticDebug"
          />
          <button
            class="inline-flex items-center gap-2 rounded-lg bg-accent px-4 py-3 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-70"
            :disabled="loadingSemanticDebug || embeddingActionsDisabled"
            @click="runSemanticDebug"
          >
            <Search :size="16" />
            {{ loadingSemanticDebug ? t("semantic.debug.debugging") : t("semantic.debug.debug") }}
          </button>
        </div>

        <div v-if="semanticDebug" class="mt-4 space-y-3">
          <div class="grid grid-cols-2 gap-3 text-sm">
            <div class="rounded-lg bg-panel px-4 py-3">
              <div class="seekmind-item-meta">{{ t("semantic.debug.vectorDim") }}</div>
              <div class="mt-1 seekmind-metric-value text-primary">{{ semanticDebug.query_vector_dim }}</div>
            </div>
            <div class="rounded-lg bg-panel px-4 py-3">
              <div class="seekmind-item-meta">{{ t("semantic.debug.hitCount") }}</div>
              <div class="mt-1 seekmind-metric-value text-primary">{{ semanticDebug.hit_count }}</div>
            </div>
          </div>

          <div class="rounded-lg bg-panel px-4 py-3 text-sm text-secondary">
            <div>{{ t("semantic.debug.normalizedQuery", { query: semanticDebug.normalized_query || t("semantic.debug.empty") }) }}</div>
            <div class="mt-1">{{ t("semantic.model", { name: semanticDebug.model.name }) }} / {{ semanticDebug.model.provider }}</div>
            <div class="mt-1">{{ t("semantic.debug.status", { status: semanticDebug.index_status || "idle", error: semanticDebug.last_error || t("semantic.noError") }) }}</div>
          </div>

          <div v-if="semanticDebug.hits.length > 0" class="space-y-2">
            <div
              v-for="hit in semanticDebug.hits.slice(0, 3)"
              :key="hit.chunk_id"
              class="rounded-lg border border-default bg-surface px-4 py-3 text-sm"
            >
              <div class="flex items-center justify-between gap-3">
                <div class="seekmind-item-title">{{ hit.file_name }}</div>
                <SeekMindBadge tone="default">{{ hit.score.toFixed(3) }}</SeekMindBadge>
              </div>
              <div class="seekmind-item-meta mt-1">{{ hit.title_path || hit.heading || t("semantic.debug.noHitHeading") }}</div>
              <div class="mt-2 line-clamp-2 text-sm text-secondary">{{ hit.snippet }}</div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.settings-card-shell {
  border: 1px solid var(--color-border);
  border-radius: 16px;
  background: linear-gradient(180deg, rgba(15, 23, 42, 0.82), rgba(10, 16, 28, 0.78));
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.035);
}

.settings-card-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  min-height: 42px;
  padding: 8px 14px;
  border-bottom: 1px solid var(--color-border);
}

.settings-card-head-left {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}

.settings-card-icon {
  width: 24px;
  height: 24px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  color: var(--color-accent);
}

/* 修复：设置页卡头图标不再复用全局蓝色图标壳，避免浅色主题下把图形吃掉。 */
.settings-card-icon--plain {
  background: transparent;
  border: 0;
  box-shadow: none;
}

.settings-card-title {
  font-size: 14px;
  font-weight: 700;
  line-height: 1;
  letter-spacing: -0.01em;
  color: var(--color-text-primary);
}

.settings-card-body {
  padding: 10px 14px 12px;
}

html:not(.dark) .settings-card-shell {
  background: rgba(255, 255, 255, 0.92);
}

html:not(.dark) .settings-card-title {
  color: #0f172a;
}

html:not(.dark) .settings-card-desc {
  color: #64748b;
}

@media (max-width: 768px) {
  .settings-card-head {
    padding: 8px 12px 7px;
  }

  .settings-card-body {
    padding: 10px 12px 12px;
  }
}
</style>
