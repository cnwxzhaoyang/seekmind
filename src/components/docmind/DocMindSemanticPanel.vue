<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
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
      message: "语义向量重建已启动",
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
    infoMessage.value = "语义向量重建已启动";
  } catch (error) {
    errorMessage.value = formatDocmindError(error, "重建语义向量失败");
    console.error("[DocMind] rebuildSemanticEmbeddings failed", error);
  } finally {
    saving.value = false;
  }
};

const setDefaultEmbeddingModel = async () => {
  errorMessage.value = "";
  infoMessage.value = "";

  if (!selectedEmbeddingModelId.value) {
    errorMessage.value = "请选择一个 embedding 模型";
    return;
  }

  saving.value = true;
  try {
    semanticStatus.value = await docmindApi.setDefaultEmbeddingModel(selectedEmbeddingModelId.value);
    infoMessage.value = "默认语义模型已更新";
  } catch (error) {
    errorMessage.value = formatDocmindError(error, "切换默认语义模型失败");
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
    errorMessage.value = formatDocmindError(error, "语义调试失败");
    console.error("[DocMind] getSemanticDebugReport failed", error);
  } finally {
    loadingSemanticDebug.value = false;
  }
};

const canSwitchModel = computed(() => selectedEmbeddingModelId.value !== semanticStatus.value?.model.id);

const installSemanticProgressListener = async () => {
  if (unlistenSemanticProgress) {
    return;
  }

  unlistenSemanticProgress = await listen<SemanticRebuildProgressView>(
    "docmind:semantic:rebuild-progress",
    (event) => {
      const payload = event.payload;
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
        infoMessage.value = "语义向量已重新构建";
        void refreshAll();
      } else if (payload.state === "failed") {
        semanticRebuildJobId.value = "";
        errorMessage.value = payload.last_error || payload.message || "重建语义向量失败";
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
  <section class="rounded-3xl border border-slate-200 bg-white p-6 shadow-sm">
    <div class="mb-4 flex items-center justify-between">
      <div>
        <div class="text-sm font-semibold text-slate-900">语义模型</div>
        <div class="mt-1 text-xs text-slate-500">当前语义索引的模型状态和重建入口。</div>
      </div>
      <DocMindBadge tone="default">{{ semanticStatus?.index_status || "idle" }}</DocMindBadge>
    </div>

    <div v-if="errorMessage" class="mb-4 rounded-2xl border border-red-100 bg-red-50 px-4 py-3 text-sm text-red-700">
      {{ errorMessage }}
    </div>

    <div v-if="infoMessage" class="mb-4 rounded-2xl border border-emerald-100 bg-emerald-50 px-4 py-3 text-sm text-emerald-700">
      {{ infoMessage }}
    </div>

    <div v-if="loading" class="rounded-2xl border border-dashed border-slate-200 bg-slate-50 px-4 py-6 text-sm text-slate-500">
      正在读取语义状态...
    </div>

    <div v-else-if="semanticStatus" class="space-y-4 text-sm">
      <label class="block">
        <div class="mb-2 text-xs text-slate-500">默认模型</div>
        <select
          v-model="selectedEmbeddingModelId"
          class="w-full rounded-2xl border border-slate-200 bg-slate-50 px-4 py-3 text-sm text-slate-900 outline-none transition focus:border-slate-400 focus:bg-white"
        >
          <option v-for="model in embeddingModels" :key="model.id" :value="model.id">
            {{ model.name }} · {{ model.provider }} · {{ model.dimension }} 维
          </option>
        </select>
      </label>

      <div v-if="semanticRebuildProgress" class="rounded-2xl border border-slate-200 bg-slate-50 px-4 py-4">
        <div class="flex items-center justify-between gap-3">
          <div>
            <div class="text-xs text-slate-500">语义重建进度</div>
            <div class="mt-1 text-sm font-medium text-slate-900">
              {{ semanticRebuildProgress.state === "completed" ? "已完成" : semanticRebuildProgress.message }}
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
            <div class="text-slate-500">当前目录 / 文档</div>
            <div class="mt-1 break-all text-slate-900">
              {{ semanticRebuildProgress.current_document || "暂无" }}
            </div>
          </div>
          <div>
            <div class="text-slate-500">当前块</div>
            <div class="mt-1 break-all text-slate-900">
              {{ semanticRebuildProgress.current_chunk || "暂无" }}
            </div>
          </div>
          <div>
            <div class="text-slate-500">已处理</div>
            <div class="mt-1 text-slate-900">
              {{ semanticRebuildProgress.processed_chunks }} / {{ semanticRebuildProgress.total_chunks }}
            </div>
          </div>
          <div>
            <div class="text-slate-500">已向量化</div>
            <div class="mt-1 text-slate-900">{{ semanticRebuildProgress.embedded_chunks }}</div>
          </div>
        </div>
      </div>

      <div class="grid grid-cols-2 gap-3">
        <div class="rounded-2xl bg-slate-50 px-4 py-3">
          <div class="text-xs text-slate-500">已向量化</div>
          <div class="mt-1 font-medium text-slate-900">{{ semanticStatus.embedded_chunks }}</div>
        </div>
        <div class="rounded-2xl bg-slate-50 px-4 py-3">
          <div class="text-xs text-slate-500">待重建</div>
          <div class="mt-1 font-medium text-slate-900">
            {{ semanticStatus.needs_rebuild ? "是" : "否" }}
          </div>
        </div>
      </div>

      <div class="rounded-2xl bg-slate-50 px-4 py-3 text-xs text-slate-600">
        <div>模型：{{ semanticStatus.model.name }}</div>
        <div class="mt-1">提供方：{{ semanticStatus.model.provider }} · {{ semanticStatus.model.dimension }} 维</div>
        <div class="mt-1">可用性：{{ semanticStatus.model.available ? "可用" : "不可用" }}</div>
        <div class="mt-1">最近索引：{{ semanticStatus.last_indexed_at || "暂无" }}</div>
        <div class="mt-1">最后错误：{{ semanticStatus.last_error || "无" }}</div>
      </div>

      <div class="grid gap-2 md:grid-cols-2">
        <button
          class="inline-flex items-center justify-center gap-2 rounded-2xl border border-slate-200 bg-white px-4 py-3 text-sm font-medium text-slate-700 shadow-sm hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
          :disabled="saving || !canSwitchModel"
          @click="setDefaultEmbeddingModel"
        >
          <Save :size="16" />
          设为默认模型
        </button>
        <button
          class="inline-flex items-center justify-center gap-2 rounded-2xl border border-slate-200 bg-white px-4 py-3 text-sm font-medium text-slate-700 shadow-sm hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
          :disabled="saving"
          @click="rebuildSemanticEmbeddings"
        >
          <Sparkles :size="16" />
          {{ saving ? "处理中..." : "重建语义向量" }}
        </button>
      </div>
    </div>

    <div class="mt-6 rounded-3xl border border-slate-200 bg-slate-50 p-5">
      <div class="mb-4 flex items-center justify-between">
        <div>
          <div class="text-sm font-semibold text-slate-900">语义调试</div>
          <div class="mt-1 text-xs text-slate-500">输入一个查询词，查看向量生成和语义召回情况。</div>
        </div>
        <DocMindBadge tone="success">仅本地</DocMindBadge>
      </div>

      <div class="flex gap-2">
        <input
          v-model="semanticQuery"
          type="text"
          class="min-w-0 flex-1 rounded-2xl border border-slate-200 bg-white px-4 py-3 text-sm text-slate-900 outline-none transition focus:border-slate-400"
          placeholder="例如：离线仓库 / 语义搜索 / Markdown 切片"
          @keyup.enter="runSemanticDebug"
        />
        <button
          class="inline-flex items-center gap-2 rounded-2xl bg-slate-900 px-4 py-3 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-70"
          :disabled="loadingSemanticDebug"
          @click="runSemanticDebug"
        >
          <Search :size="16" />
          {{ loadingSemanticDebug ? "调试中..." : "调试" }}
        </button>
      </div>

      <div v-if="semanticDebug" class="mt-4 space-y-3">
        <div class="grid grid-cols-2 gap-3 text-sm">
          <div class="rounded-2xl bg-white px-4 py-3">
            <div class="text-xs text-slate-500">向量维度</div>
            <div class="mt-1 font-medium text-slate-900">{{ semanticDebug.query_vector_dim }}</div>
          </div>
          <div class="rounded-2xl bg-white px-4 py-3">
            <div class="text-xs text-slate-500">命中数</div>
            <div class="mt-1 font-medium text-slate-900">{{ semanticDebug.hit_count }}</div>
          </div>
        </div>

        <div class="rounded-2xl bg-white px-4 py-3 text-xs text-slate-600">
          <div>归一化查询：{{ semanticDebug.normalized_query || "空" }}</div>
          <div class="mt-1">模型：{{ semanticDebug.model.name }} / {{ semanticDebug.model.provider }}</div>
          <div class="mt-1">状态：{{ semanticDebug.index_status || "idle" }} · {{ semanticDebug.last_error || "无错误" }}</div>
        </div>

        <div v-if="semanticDebug.hits.length > 0" class="space-y-2">
          <div
            v-for="hit in semanticDebug.hits.slice(0, 3)"
            :key="hit.chunk_id"
            class="rounded-2xl border border-slate-200 bg-white px-4 py-3 text-sm shadow-sm"
          >
            <div class="flex items-center justify-between gap-3">
              <div class="font-medium text-slate-900">{{ hit.file_name }}</div>
              <DocMindBadge tone="default">{{ hit.score.toFixed(3) }}</DocMindBadge>
            </div>
            <div class="mt-1 text-xs text-slate-500">{{ hit.heading || "未命中标题" }}</div>
            <div class="mt-2 line-clamp-2 text-slate-600">{{ hit.snippet }}</div>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>
