<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { listen } from "@tauri-apps/api/event";
import { ChevronDown, ChevronUp, RefreshCw, NotebookText, FileText, Database, Sparkles, Trash2 } from "lucide-vue-next";
import DocMindBadge from "./DocMindBadge.vue";
import type { DocumentRefreshProgressView, IndexRefreshProgressView, SemanticRebuildProgressView } from "../../types/docmind";

type LogScope = "index" | "document" | "semantic";
type LogLevel = "info" | "success" | "warning" | "error";

interface LogEntry {
  id: string;
  scope: LogScope;
  level: LogLevel;
  title: string;
  message: string;
  details: string;
  timestamp: string;
}

const expanded = ref(false);
const entries = ref<LogEntry[]>([]);
const maxEntries = 120;
let unlistenIndex: null | (() => void) = null;
let unlistenDocument: null | (() => void) = null;
let unlistenSemantic: null | (() => void) = null;

const scopeMeta: Record<LogScope, { label: string; icon: typeof Database }> = {
  index: { label: "索引", icon: Database },
  document: { label: "切片", icon: FileText },
  semantic: { label: "语义", icon: Sparkles },
};

const levelTone: Record<LogLevel, "default" | "success" | "warning" | "danger"> = {
  info: "default",
  success: "success",
  warning: "warning",
  error: "danger",
};

const pushLog = (entry: Omit<LogEntry, "id" | "timestamp">) => {
  const now = new Date().toISOString();
  entries.value = [
    { id: `${now}-${Math.random().toString(16).slice(2)}`, timestamp: now, ...entry },
    ...entries.value,
  ].slice(0, maxEntries);
};

const formatTime = (timestamp: string) => {
  const date = new Date(timestamp);
  return date.toLocaleTimeString("zh-CN", { hour12: false });
};

const installListeners = async () => {
  if (unlistenIndex || unlistenDocument || unlistenSemantic) {
    return;
  }

  unlistenIndex = await listen<IndexRefreshProgressView>("docmind:index-refresh-progress", (event) => {
    const payload = event.payload;
    const scope: LogScope = "index";
    const level: LogLevel = payload.state === "failed" ? "error" : payload.state === "completed" ? "success" : "info";
    pushLog({
      scope,
      level,
      title: `${scopeMeta[scope].label}任务`,
      message: payload.message,
      details: payload.scope === "dir" && payload.path ? `目录：${payload.path}` : "全量索引",
    });
  });

  unlistenDocument = await listen<DocumentRefreshProgressView>("docmind:document-refresh-progress", (event) => {
    const payload = event.payload;
    const scope: LogScope = "document";
    const level: LogLevel = payload.state === "failed" ? "error" : payload.warning ? "warning" : "success";
    pushLog({
      scope,
      level,
      title: `${scopeMeta[scope].label}任务`,
      message: payload.message,
      details: `${payload.file_name}${payload.warning ? ` · ${payload.parser_source.toUpperCase()} 回退` : ` · ${payload.parser_source.toUpperCase()}`}`,
    });
  });

  unlistenSemantic = await listen<SemanticRebuildProgressView>("docmind:semantic:rebuild-progress", (event) => {
    const payload = event.payload;
    const scope: LogScope = "semantic";
    const level: LogLevel = payload.state === "failed" ? "error" : payload.state === "completed" ? "success" : "info";
    pushLog({
      scope,
      level,
      title: `${scopeMeta[scope].label}任务`,
      message: payload.message,
      details: payload.current_document || "正在重建向量",
    });
  });
};

const clearLogs = () => {
  entries.value = [];
};

const toggleExpanded = () => {
  expanded.value = !expanded.value;
};

const displayedEntries = computed(() => entries.value.slice(0, 20));

onMounted(() => {
  void installListeners();
});

onBeforeUnmount(() => {
  unlistenIndex?.();
  unlistenDocument?.();
  unlistenSemantic?.();
  unlistenIndex = null;
  unlistenDocument = null;
  unlistenSemantic = null;
});
</script>

<template>
  <div class="pointer-events-none fixed bottom-4 right-4 z-50">
    <div
      class="pointer-events-auto overflow-hidden rounded-3xl border border-slate-200 bg-white shadow-2xl transition-all duration-200"
      :class="expanded ? 'w-[420px] max-h-[520px]' : 'w-[180px]'"
    >
      <button
        class="flex w-full items-center justify-between gap-3 border-b border-slate-100 px-4 py-3 text-left"
        @click="toggleExpanded"
      >
        <div class="flex items-center gap-2">
          <NotebookText class="text-slate-700" :size="16" />
          <div>
            <div class="text-sm font-semibold text-slate-900">日志</div>
            <div class="text-[11px] text-slate-500">索引 / 切片 / 语义事件</div>
          </div>
        </div>
        <component :is="expanded ? ChevronDown : ChevronUp" :size="16" class="text-slate-500" />
      </button>

      <div v-if="expanded" class="max-h-[460px] overflow-hidden">
        <div class="flex items-center justify-between border-b border-slate-100 px-4 py-2 text-xs text-slate-500">
          <div>{{ entries.length }} 条事件</div>
          <button class="inline-flex items-center gap-1 hover:text-slate-700" @click="clearLogs">
            <Trash2 :size="13" /> 清空
          </button>
        </div>
        <div class="max-h-[400px] overflow-y-auto p-2">
          <div v-if="entries.length === 0" class="rounded-2xl bg-slate-50 px-4 py-6 text-sm text-slate-500">
            暂无日志，执行索引、切片或语义任务后会自动显示事件。
          </div>
          <div v-else class="space-y-2">
            <div
              v-for="entry in displayedEntries"
              :key="entry.id"
              class="rounded-2xl border border-slate-100 bg-slate-50 px-3 py-2"
            >
              <div class="flex items-start justify-between gap-2">
                <div class="min-w-0 flex-1">
                  <div class="flex items-center gap-2">
                    <DocMindBadge :tone="levelTone[entry.level]">{{ scopeMeta[entry.scope].label }}</DocMindBadge>
                    <div class="truncate text-sm font-medium text-slate-900">{{ entry.title }}</div>
                  </div>
                  <div class="mt-1 text-xs leading-5 text-slate-600">{{ entry.message }}</div>
                  <div class="mt-1 truncate text-[11px] text-slate-400">{{ entry.details }}</div>
                </div>
                <div class="shrink-0 text-[11px] text-slate-400">{{ formatTime(entry.timestamp) }}</div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
