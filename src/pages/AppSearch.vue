<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { CheckCircle2, Clock, Cpu, ExternalLink, FileText, Filter, Search } from "lucide-vue-next";
import DocMindBadge from "../components/docmind/DocMindBadge.vue";
import DocMindSearchResultCard from "../components/docmind/DocMindSearchResultCard.vue";
import { docmindApi } from "../services/docmindApi";
import type { IndexStatusView, ParserRuntimeView, SearchDebugView, SearchResultView } from "../types/docmind";

const router = useRouter();
const query = ref("");
const selectedId = ref<string>("");
const results = ref<SearchResultView[]>([]);
const debugReport = ref<SearchDebugView | null>(null);
const status = ref<IndexStatusView | null>(null);
const parserRuntime = ref<ParserRuntimeView | null>(null);
const selectedChunkCount = ref<number | null>(null);
const loading = ref(false);
const errorMessage = ref("");

const selected = computed(
  () => results.value.find((item) => item.id === selectedId.value) ?? results.value[0] ?? null,
);

const selectResult = (id: string) => {
  selectedId.value = id;
};

const loadStatus = async () => {
  status.value = await docmindApi.getIndexStatus();
};

const loadParserRuntime = async () => {
  parserRuntime.value = await docmindApi.getParserRuntime();
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
    debugReport.value = await docmindApi.getSearchDebugReport(query.value, 20);
  } catch (error) {
    results.value = [];
    selectedId.value = "";
    errorMessage.value = error instanceof Error ? error.message : "搜索失败";
    debugReport.value = await docmindApi.getSearchDebugReport(query.value, 20).catch(() => null);
  } finally {
    loading.value = false;
  }
};

const openSelectedFile = async () => {
  if (!selected.value) return;
  await docmindApi.openFile(selected.value.path);
};

const viewChunks = async () => {
  if (!selected.value) return;
  await router.push({ path: "/chunks", query: { path: selected.value.path } });
};

onMounted(async () => {
  await Promise.all([loadStatus(), loadParserRuntime(), runSearch()]);
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
            <h1 class="text-2xl font-semibold tracking-tight text-slate-950">搜索文档内容</h1>
            <p class="mt-1 text-sm text-slate-500">输入关键词，定位到文档中的具体段落。</p>
          </div>
          <div class="flex flex-wrap items-center gap-2">
            <DocMindBadge tone="success">
              <CheckCircle2 class="mr-1" :size="13" />
              已索引 {{ status?.indexed_docs ?? 0 }} 个文档
            </DocMindBadge>
            <DocMindBadge :tone="parserRuntime?.active === 'python' ? 'success' : 'warning'">
              <Cpu class="mr-1" :size="13" />
              {{ parserRuntime?.active === 'python' ? 'Python 解析' : 'Rust 回退' }}
            </DocMindBadge>
          </div>
        </div>

      <form class="flex items-center gap-3 rounded-3xl border border-slate-200 bg-white px-4 py-3 shadow-sm" @submit.prevent="runSearch">
        <Search :size="20" class="text-slate-400" />
        <input
          v-model="query"
          placeholder="搜索文档内容、标题或文件名..."
          class="flex-1 bg-transparent text-base outline-none placeholder:text-slate-400"
        />
        <button class="rounded-2xl bg-slate-900 px-4 py-2 text-sm font-medium text-white" :disabled="loading">
          {{ loading ? "搜索中..." : "搜索" }}
        </button>
      </form>

      <div v-if="debugReport" class="mt-4 grid gap-3 md:grid-cols-2 xl:grid-cols-4">
        <div class="rounded-2xl bg-white px-4 py-3 shadow-sm ring-1 ring-slate-200">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">SQLite 文档 / 段落</div>
          <div class="mt-1 text-sm font-semibold text-slate-900">
            {{ debugReport.sqlite_documents }} / {{ debugReport.sqlite_chunks }}
          </div>
        </div>
        <div class="rounded-2xl bg-white px-4 py-3 shadow-sm ring-1 ring-slate-200">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">Tantivy 文档</div>
          <div class="mt-1 text-sm font-semibold text-slate-900">{{ debugReport.tantivy_documents }}</div>
        </div>
        <div class="rounded-2xl bg-white px-4 py-3 shadow-sm ring-1 ring-slate-200">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">归一化查询</div>
          <div class="mt-1 break-words text-sm font-medium text-slate-900">
            {{ debugReport.normalized_terms.join(" · ") || "空" }}
          </div>
        </div>
        <div class="rounded-2xl bg-white px-4 py-3 shadow-sm ring-1 ring-slate-200">
          <div class="text-[11px] uppercase tracking-wide text-slate-500">命中数量</div>
          <div class="mt-1 text-sm font-semibold text-slate-900">{{ debugReport.hit_count }}</div>
        </div>
      </div>
    </header>

    <main class="grid min-h-0 flex-1 grid-cols-[minmax(420px,0.95fr)_minmax(360px,0.8fr)] gap-0">
      <section class="min-h-0 overflow-y-auto border-r border-slate-200 bg-slate-50/50 p-5">
        <div class="mb-4 flex items-center justify-between">
          <div class="text-sm text-slate-500">
            找到 <span class="font-semibold text-slate-800">{{ results.length }}</span> 个相关段落
          </div>
          <button class="flex items-center gap-1 rounded-xl border border-slate-200 bg-white px-3 py-1.5 text-xs text-slate-600">
            <Filter :size="14" />
            筛选
          </button>
        </div>

        <div v-if="errorMessage" class="mb-4 rounded-2xl border border-red-100 bg-red-50 px-4 py-3 text-sm text-red-700">
          {{ errorMessage }}
        </div>

        <div v-if="!results.length && !loading" class="rounded-3xl border border-dashed border-slate-300 bg-white px-5 py-10 text-center text-sm text-slate-500">
          没有找到相关段落
        </div>

        <div class="space-y-3">
          <DocMindSearchResultCard
            v-for="item in results"
            :key="item.id"
            :item="item"
            :selected="selectedId === item.id"
            @select="selectResult(item.id)"
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
            <DocMindBadge>{{ selected.page ? `第 ${selected.page} 页` : `第 ${selected.paragraph} 段` }}</DocMindBadge>
            <DocMindBadge tone="default">切片数 {{ selectedChunkCount ?? "..." }}</DocMindBadge>
            <DocMindBadge tone="default"><Clock class="mr-1 inline" :size="12" />{{ selected.modified }}</DocMindBadge>
          </div>

          <div class="rounded-3xl border border-slate-200 bg-slate-50 p-5">
            <div class="mb-2 text-sm font-medium text-slate-700">命中段落</div>
            <p class="text-sm leading-7 text-slate-700">{{ selected.snippet }}</p>
          </div>

          <div class="mt-5 rounded-3xl border border-slate-200 bg-white p-5">
            <div class="mb-2 text-sm font-medium text-slate-700">上下文预览</div>
            <p class="text-sm leading-7 text-slate-500">上一段：构建离线仓库时，需要先解析项目的 parent POM、BOM 以及 build plugins。</p>
            <p class="mt-3 text-sm leading-7 text-slate-800">当前段：{{ selected.snippet }}</p>
            <p class="mt-3 text-sm leading-7 text-slate-500">下一段：生成后的 .offline-repo 可以通过 settings.xml 指定为本地仓库路径。</p>
          </div>

          <div class="mt-6 grid grid-cols-2 gap-3">
            <button class="flex items-center justify-center gap-2 rounded-2xl bg-slate-900 px-4 py-3 text-sm font-medium text-white" @click="openSelectedFile">
              <ExternalLink :size="16" />
              打开文件
            </button>
            <button class="flex items-center justify-center gap-2 rounded-2xl border border-slate-200 bg-white px-4 py-3 text-sm font-medium text-slate-700" @click="viewChunks">
              <FileText :size="16" />
              查看切片
            </button>
          </div>
        </div>
        <div v-else class="rounded-3xl border border-dashed border-slate-300 bg-slate-50 p-6 text-sm text-slate-500">
          请输入关键词开始搜索。
        </div>
      </aside>
    </main>
  </div>
</template>
