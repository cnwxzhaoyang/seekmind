<script setup lang="ts">
import { onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderPlus, FolderOpen, CheckCircle2, Loader2, RefreshCw, X, ToggleLeft, ToggleRight } from "lucide-vue-next";
import DocMindBadge from "../components/docmind/DocMindBadge.vue";
import { docmindApi } from "../services/docmindApi";
import type { IndexDirView } from "../types/docmind";

const dirs = ref<IndexDirView[]>([]);
const loading = ref(false);
const refreshing = ref(false);
const busyPath = ref<string | null>(null);
const errorMessage = ref("");
const infoMessage = ref("");

const loadDirs = async () => {
  loading.value = true;
  errorMessage.value = "";

  try {
    dirs.value = await docmindApi.listIndexDirs();
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : "目录加载失败";
  } finally {
    loading.value = false;
  }
};

const chooseAndAddDir = async () => {
  errorMessage.value = "";
  infoMessage.value = "";

  const selected = await open({
    directory: true,
    multiple: false,
    title: "选择要索引的目录",
  });

  if (typeof selected !== "string" || selected.trim().length === 0) {
    return;
  }

  busyPath.value = selected;
  try {
    await docmindApi.addIndexDir(selected);
    infoMessage.value = `已添加目录: ${selected}`;
    await loadDirs();
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : "添加目录失败";
  } finally {
    busyPath.value = null;
  }
};

const refreshIndex = async () => {
  refreshing.value = true;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    await docmindApi.refreshIndex();
    await loadDirs();
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : "重新索引失败";
  } finally {
    refreshing.value = false;
  }
};

const refreshSingleDir = async (path: string) => {
  busyPath.value = path;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    await docmindApi.refreshIndexDir(path);
    infoMessage.value = `已重新索引: ${path}`;
    await loadDirs();
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : "目录重建失败";
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
    infoMessage.value = dir.enabled ? `已禁用: ${dir.path}` : `已启用: ${dir.path}`;
    await loadDirs();
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : "目录状态更新失败";
  } finally {
    busyPath.value = null;
  }
};

const removeDir = async (path: string) => {
  if (!window.confirm(`确认删除索引目录？\n${path}`)) {
    return;
  }

  busyPath.value = path;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    await docmindApi.removeIndexDir(path);
    infoMessage.value = `已删除目录: ${path}`;
    await loadDirs();
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : "删除目录失败";
  } finally {
    busyPath.value = null;
  }
};

onMounted(loadDirs);
</script>

<template>
  <div class="h-full overflow-y-auto p-8">
    <div class="mb-7 flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-semibold tracking-tight text-slate-950">文档目录</h1>
        <p class="mt-1 text-sm text-slate-500">添加需要索引的文件夹，docMind 会在本地解析和建立索引。</p>
      </div>
      <button
        class="flex items-center gap-2 rounded-2xl bg-slate-900 px-4 py-2.5 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-70"
        :disabled="refreshing"
        @click="chooseAndAddDir"
      >
        <FolderPlus :size="17" />
        添加目录
      </button>
    </div>

    <div v-if="errorMessage" class="mb-4 rounded-2xl border border-red-100 bg-red-50 px-4 py-3 text-sm text-red-700">
      {{ errorMessage }}
    </div>

    <div v-if="infoMessage" class="mb-4 rounded-2xl border border-emerald-100 bg-emerald-50 px-4 py-3 text-sm text-emerald-700">
      {{ infoMessage }}
    </div>

    <div v-if="loading" class="rounded-3xl border border-dashed border-slate-300 bg-white p-6 text-sm text-slate-500">
      正在读取索引目录...
    </div>

    <div v-else class="space-y-3">
      <div v-for="dir in dirs" :key="dir.path" class="rounded-3xl border border-slate-200 bg-white p-5 shadow-sm">
        <div class="flex items-center justify-between gap-4">
          <div class="flex min-w-0 items-center gap-4">
            <div class="flex h-11 w-11 items-center justify-center rounded-2xl bg-slate-100">
              <FolderOpen :size="20" class="text-slate-600" />
            </div>
            <div class="min-w-0">
              <div class="truncate text-sm font-semibold text-slate-900">{{ dir.path }}</div>
              <div class="mt-1 text-xs text-slate-500">{{ dir.docs }} 个文档 · {{ dir.chunks.toLocaleString() }} 个段落</div>
            </div>
          </div>

          <div class="flex items-center gap-2">
            <DocMindBadge v-if="dir.status === 'indexing'" tone="warning">
              <Loader2 class="mr-1 animate-spin" :size="13" />
              索引中
            </DocMindBadge>
            <DocMindBadge v-else tone="success">
              <CheckCircle2 class="mr-1" :size="13" />
              已完成
            </DocMindBadge>
            <button
              class="rounded-xl border border-slate-200 p-2 text-slate-500 hover:bg-slate-50"
              :disabled="busyPath === dir.path"
              @click="refreshSingleDir(dir.path)"
            >
              <RefreshCw :size="16" />
            </button>
            <button
              class="rounded-xl border border-slate-200 p-2 text-slate-500 hover:bg-slate-50"
              :disabled="busyPath === dir.path"
              @click="toggleDir(dir)"
            >
              <component :is="dir.enabled ? ToggleRight : ToggleLeft" :size="16" />
            </button>
            <button
              class="rounded-xl border border-slate-200 p-2 text-slate-500 hover:bg-slate-50"
              :disabled="busyPath === dir.path"
              @click="removeDir(dir.path)"
            >
              <X :size="16" />
            </button>
          </div>
        </div>
      </div>
    </div>

    <div class="mt-8 rounded-3xl border border-dashed border-slate-300 bg-slate-50 p-8 text-center">
      <FolderPlus class="mx-auto mb-3 text-slate-400" :size="28" />
      <div class="text-sm font-medium text-slate-700">支持添加、启停和删除索引目录</div>
      <div class="mt-1 text-xs text-slate-500">建议优先添加 Documents、Downloads 或你的项目资料目录。</div>
    </div>
  </div>
</template>
