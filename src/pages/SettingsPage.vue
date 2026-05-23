<script setup lang="ts">
import { ref } from "vue";
import DocMindBadge from "../components/docmind/DocMindBadge.vue";
import { docmindApi, formatDocmindError } from "../services/docmindApi";

const loading = ref(false);
const infoMessage = ref("");
const errorMessage = ref("");

const clearAllIndexes = async () => {
  const confirmed = window.confirm("确认清空全部索引？这只会删除 docMind 的本地索引数据，不会删除原始文件。");
  if (!confirmed) {
    return;
  }

  loading.value = true;
  infoMessage.value = "";
  errorMessage.value = "";

  try {
    await docmindApi.clearAllIndexes();
    infoMessage.value = "已清空全部索引。";
  } catch (error) {
    errorMessage.value = formatDocmindError(error, "清空索引失败");
    console.error("[DocMind] clearAllIndexes failed", error);
  } finally {
    loading.value = false;
  }
};
</script>

<template>
  <div class="h-full overflow-y-auto p-8">
    <div class="mb-7">
      <h1 class="text-2xl font-semibold tracking-tight text-slate-950">设置</h1>
      <p class="mt-1 text-sm text-slate-500">控制文件过滤、索引策略和本地数据。</p>
    </div>

    <div class="space-y-5">
      <div v-if="errorMessage" class="rounded-3xl border border-red-100 bg-red-50 p-4 text-sm text-red-700">
        {{ errorMessage }}
      </div>

      <div v-if="infoMessage" class="rounded-3xl border border-emerald-100 bg-emerald-50 p-4 text-sm text-emerald-700">
        {{ infoMessage }}
      </div>

      <div class="rounded-3xl border border-slate-200 bg-white p-6 shadow-sm">
        <div class="mb-4 text-sm font-semibold text-slate-900">索引规则</div>
        <div class="space-y-4">
          <label class="flex items-center justify-between text-sm text-slate-700">
            <span>最大文件大小</span>
            <select class="rounded-xl border border-slate-200 bg-white px-3 py-2">
              <option>50 MB</option>
              <option>100 MB</option>
              <option>200 MB</option>
            </select>
          </label>
          <label class="flex items-center justify-between text-sm text-slate-700">
            <span>隐藏文件</span>
            <DocMindBadge>默认排除</DocMindBadge>
          </label>
          <label class="flex items-center justify-between text-sm text-slate-700">
            <span>支持格式</span>
            <span class="text-xs text-slate-500">TXT, MD, HTML, DOCX, PDF</span>
          </label>
        </div>
      </div>

      <div class="rounded-3xl border border-slate-200 bg-white p-6 shadow-sm">
        <div class="mb-4 text-sm font-semibold text-slate-900">排除目录</div>
        <div class="flex flex-wrap gap-2">
          <DocMindBadge v-for="x in ['node_modules', '.git', 'target', 'Library', 'Applications', 'System']" :key="x">
            {{ x }}
          </DocMindBadge>
        </div>
      </div>

      <div class="rounded-3xl border border-red-100 bg-red-50 p-6">
        <div class="text-sm font-semibold text-red-800">危险操作</div>
        <div class="mt-1 text-xs text-red-600">清空索引不会删除原始文件，只会删除 docMind 的本地索引数据。</div>
        <button
          class="mt-4 rounded-2xl bg-white px-4 py-2 text-sm font-medium text-red-700 shadow-sm disabled:cursor-not-allowed disabled:opacity-60"
          :disabled="loading"
          @click="clearAllIndexes"
        >
          {{ loading ? "清空中..." : "清空全部索引" }}
        </button>
      </div>
    </div>
  </div>
</template>
