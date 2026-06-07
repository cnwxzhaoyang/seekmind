<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { listen } from "@tauri-apps/api/event";
import { ChevronDown, ChevronUp, NotebookText, FileText, Database, Sparkles, Trash2 } from "lucide-vue-next";
import SeekMindBadge from "./SeekMindBadge.vue";
import { seekMindApi } from "../../services/seekMindApi";
import type {
  DocumentRefreshProgressView,
  IndexRefreshProgressView,
  IndexSettingsView,
  IndexStatusView,
  SemanticRebuildProgressView,
} from "../../types/SeekMind";

const HEADER_H = 28;
const DIVIDER_H = 3;
const HEIGHT_COLLAPSED = HEADER_H + DIVIDER_H;
const HEIGHT_MIN = 120;
const HEIGHT_MAX = 500;
const HEIGHT_DEFAULT = 200;
const STORAGE_KEY = "seekmind.logPanel.height";

const { t } = useI18n();

type LogScope = "index" | "document" | "semantic";
type LogLevel = "info" | "success" | "warning" | "error";

interface LogEntry {
  id: string;
  scope: LogScope;
  level: LogLevel;
  title: string;
  message: string;
  details: string;
  warning?: string;
  timestamp: string;
}

const expanded = ref(false);
const entries = ref<LogEntry[]>([]);
const panelHeight = ref(loadSavedHeight());
const dragging = ref(false);
const dragStartY = ref(0);
const dragStartHeight = ref(0);
const indexStatus = ref<IndexStatusView | null>(null);
const indexSettings = ref<IndexSettingsView | null>(null);
const maxEntries = 120;
let unlistenIndex: null | (() => void) = null;
let unlistenDocument: null | (() => void) = null;
let unlistenSemantic: null | (() => void) = null;

function loadSavedHeight(): number {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved) {
      const h = parseInt(saved, 10);
      if (h >= HEIGHT_MIN && h <= HEIGHT_MAX) return h;
    }
  } catch {}
  return HEIGHT_DEFAULT;
}

function saveHeight(h: number) {
  try { localStorage.setItem(STORAGE_KEY, String(h)); } catch {}
}

const scopeMeta: Record<LogScope, { label: string; taskLabel: string; icon: typeof Database }> = {
  index: { label: "logPanel.scope.index", taskLabel: "logPanel.scopeLabel.index", icon: Database },
  document: { label: "logPanel.scope.document", taskLabel: "logPanel.scopeLabel.document", icon: FileText },
  semantic: { label: "logPanel.scope.semantic", taskLabel: "logPanel.scopeLabel.semantic", icon: Sparkles },
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
  return date.toLocaleTimeString([], { hour12: false });
};

const loadMetrics = async () => {
  const [status, settings] = await Promise.all([
    seekMindApi.getIndexStatus(),
    seekMindApi.getIndexSettings(),
  ]);
  indexStatus.value = status;
  indexSettings.value = settings;
};

const semanticWeightLabel = computed(() => Math.round((indexSettings.value?.semantic_weight ?? 0.25) * 100));
const sqliteLabel = computed(() => `SQLite: ${indexStatus.value?.indexed_docs ?? 0}/${indexStatus.value?.scanned_docs ?? 0}`);
const tantivyLabel = computed(() => `Tantivy: ${indexStatus.value?.indexed_chunks ?? 0}`);

const installListeners = async () => {
  if (unlistenIndex || unlistenDocument || unlistenSemantic) return;

  unlistenIndex = await listen<IndexRefreshProgressView>("seekmind:index-refresh-progress", (event) => {
    const payload = event.payload;
    indexStatus.value = payload.status;
    const scope: LogScope = "index";
    const level: LogLevel = payload.state === "failed" ? "error" : payload.state === "completed" ? "success" : "info";
    pushLog({
      scope, level,
      title: t(scopeMeta[scope].taskLabel),
      message: payload.message,
      details: payload.scope === "fulltext-repair"
        ? (payload.path || t("logPanel.details.fulltextRepair"))
        : payload.scope === "dir" && payload.path
          ? t("logPanel.details.dir", { path: payload.path })
          : t("logPanel.details.fullIndex"),
    });
  });

  unlistenDocument = await listen<DocumentRefreshProgressView>("seekmind:document-refresh-progress", (event) => {
    const payload = event.payload;
    const scope: LogScope = "document";
    const level: LogLevel =
      payload.state === "failed" ? "error"
        : payload.warning ? "warning"
          : payload.state === "completed" ? "success" : "info";
    pushLog({
      scope, level,
      title: t(scopeMeta[scope].taskLabel),
      message: payload.message,
      details: payload.state === "failed"
        ? `${payload.file_name} · ${payload.parser_source.toUpperCase()} · ${payload.message}`
        : `${payload.file_name} · ${payload.parser_source.toUpperCase()}`,
      warning: payload.warning || undefined,
    });
  });

  unlistenSemantic = await listen<SemanticRebuildProgressView>("seekmind:semantic:rebuild-progress", (event) => {
    const payload = event.payload;
    const scope: LogScope = "semantic";
    const level: LogLevel = payload.state === "failed" ? "error" : payload.state === "completed" ? "success" : "info";
    pushLog({
      scope, level,
      title: t(scopeMeta[scope].taskLabel),
      message: payload.message,
      details: payload.current_document || t("logPanel.details.rebuilding"),
    });
  });
};

const clearLogs = () => { entries.value = []; };

const toggleExpanded = () => { expanded.value = !expanded.value; };

const onResizeStart = (event: MouseEvent) => {
  dragging.value = true;
  dragStartY.value = event.clientY;
  dragStartHeight.value = panelHeight.value;
  if (!expanded.value) {
    expanded.value = true;
    panelHeight.value = HEIGHT_DEFAULT;
    dragStartHeight.value = HEIGHT_DEFAULT;
  }
  document.addEventListener("mousemove", onResizeMove);
  document.addEventListener("mouseup", onResizeEnd);
};

const onResizeMove = (event: MouseEvent) => {
  if (!dragging.value) return;
  const delta = dragStartY.value - event.clientY;
  const newHeight = Math.max(HEIGHT_MIN, Math.min(HEIGHT_MAX, dragStartHeight.value + delta));
  panelHeight.value = newHeight;
};

const onResizeEnd = () => {
  dragging.value = false;
  saveHeight(panelHeight.value);
  document.removeEventListener("mousemove", onResizeMove);
  document.removeEventListener("mouseup", onResizeEnd);
};

const panelStyle = computed(() => {
  if (!expanded.value) return { height: `${HEIGHT_COLLAPSED}px` };
  return { height: `${panelHeight.value}px` };
});

const contentStyle = computed(() => {
  if (!expanded.value) return {};
  return { height: `calc(100% - ${HEADER_H + DIVIDER_H}px)` };
});

onMounted(() => {
  void installListeners();
  void loadMetrics();
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
  <div
    class="relative shrink-0 bg-panel"
    :class="dragging ? 'select-none' : ''"
    :style="panelStyle"
  >
    <div
      class="h-[3px] w-full cursor-ns-resize shrink-0 transition-colors"
      :class="dragging ? 'bg-accent' : 'bg-border hover:bg-accent active:bg-accent'"
      @mousedown.prevent="onResizeStart"
    />

    <button
      class="flex h-7 w-full items-center justify-between gap-3 px-3 text-left"
      @click="toggleExpanded"
    >
      <div class="flex items-center gap-2">
        <NotebookText class="text-dim" :size="14" />
        <div class="text-[11px] font-medium text-secondary">{{ t("logPanel.title") }}</div>
        <div class="hidden text-[11px] text-dim sm:block">{{ t("logPanel.desc") }}</div>
      </div>
      <div class="flex items-center gap-2 text-[11px] text-muted">
        <SeekMindBadge tone="success">{{ sqliteLabel }}</SeekMindBadge>
        <SeekMindBadge tone="default">{{ tantivyLabel }}</SeekMindBadge>
        <SeekMindBadge tone="default">{{ t("page.appSearch.semanticWeight", { weight: semanticWeightLabel }) }}</SeekMindBadge>
        <span class="ml-1">{{ t("logPanel.events", { count: entries.length }) }}</span>
        <component :is="expanded ? ChevronDown : ChevronUp" :size="16" class="text-muted" />
      </div>
    </button>

      <div v-if="expanded" class="flex flex-col" :style="contentStyle">
      <div class="flex items-center justify-end border-t border-light px-4 py-2 text-xs text-dim shrink-0">
        <button class="inline-flex items-center gap-1 hover:text-secondary" @click="clearLogs">
          <Trash2 :size="13" /> {{ t("logPanel.clear") }}
        </button>
      </div>
      <div class="flex-1 overflow-y-auto p-2">
        <div v-if="entries.length === 0" class="rounded-2xl bg-panel px-4 py-6 text-sm text-dim">
          {{ t("logPanel.empty") }}
        </div>
        <div v-else class="space-y-2">
          <div
            v-for="entry in entries"
            :key="entry.id"
            class="rounded-2xl border border-light bg-panel px-3 py-2"
          >
            <div class="flex items-start justify-between gap-2">
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-2">
                  <SeekMindBadge :tone="levelTone[entry.level]">{{ t(scopeMeta[entry.scope].label) }}</SeekMindBadge>
                  <div class="truncate text-sm font-medium text-primary">{{ entry.title }}</div>
                </div>
                <div class="mt-1 text-xs leading-5 text-secondary">{{ entry.message }}</div>
                <div class="mt-1 truncate text-[11px] text-muted">{{ entry.details }}</div>
                <div
                  v-if="entry.warning"
                  class="mt-1 rounded-xl border border-amber-soft bg-amber-soft px-2 py-1 text-[11px] leading-4 text-warning"
                >
                  {{ entry.warning }}
                </div>
              </div>
              <div class="shrink-0 text-[11px] text-muted">{{ formatTime(entry.timestamp) }}</div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
