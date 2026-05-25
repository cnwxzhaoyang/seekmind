<script setup lang="ts">
import { computed } from "vue";
import { ChevronDown, ChevronRight, Folder } from "lucide-vue-next";
import type { VisibleIndexDirRow } from "../../composables/useIndexDirTree";

const props = withDefaults(defineProps<{
  rows: VisibleIndexDirRow[];
  selectedPath?: string;
  emptyText?: string;
  nodePaddingBase?: number;
  nodePaddingStep?: number;
  pathTooltip?: boolean;
  expandTitle?: string;
  collapseTitle?: string;
  selectable?: boolean;
  virtualLabel?: string;
  density?: "compact" | "normal" | "relaxed";
}>(), {
  density: "normal",
  selectable: true,
});

const emit = defineEmits<{
  "node-select": [path: string];
  toggle: [path: string, expanded: boolean];
  contextmenu: [row: VisibleIndexDirRow, event: MouseEvent];
}>();

const rowHeight = computed(() => {
  if (props.density === "compact") return "h-7 text-[13px]";
  if (props.density === "relaxed") return "h-10 text-sm";
  return "h-8.5 text-sm";
});

const handleSelect = (path: string) => {
  if (props.selectable === false) return;
  emit("node-select", path);
};

const handleKeydown = (event: KeyboardEvent, path: string) => {
  if (props.selectable === false) return;
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    emit("node-select", path);
  }
};

const handleContextMenu = (row: VisibleIndexDirRow, event: MouseEvent) => {
  event.preventDefault();
  emit("contextmenu", row, event);
};
</script>

<template>
  <div v-if="rows.length === 0" class="px-3 py-4 text-xs text-slate-400">
    <slot name="empty">
      {{ emptyText || "No items" }}
    </slot>
  </div>
  <div v-else>
    <div
      v-for="row in rows"
      :key="row.dir.path"
      class="flex items-center gap-1.5 transition-colors"
      :class="[
        rowHeight,
        selectedPath === row.dir.path ? 'bg-indigo-50 text-indigo-700' : 'hover:bg-slate-100 text-slate-700',
        selectable === false ? 'cursor-default' : 'cursor-pointer select-none',
      ]"
      :style="{ paddingLeft: `${(nodePaddingBase ?? 8) + row.depth * (nodePaddingStep ?? 14)}px`, paddingRight: '12px' }"
      :title="pathTooltip === false ? '' : row.fullPath"
      :role="selectable === false ? undefined : 'button'"
      :tabindex="selectable === false ? undefined : 0"
      @click="handleSelect(row.dir.path)"
      @keydown="handleKeydown($event, row.dir.path)"
      @contextmenu="handleContextMenu(row, $event)"
    >
      <button
        v-if="row.hasChildren"
        class="inline-flex h-4 w-4 shrink-0 items-center justify-center rounded text-slate-400 hover:text-slate-600"
        type="button"
        :title="row.expanded ? (collapseTitle || 'Collapse') : (expandTitle || 'Expand')"
        :aria-expanded="row.expanded"
        @click.stop="emit('toggle', row.dir.path, !row.expanded)"
      >
        <ChevronDown v-if="row.expanded" :size="12" />
        <ChevronRight v-else :size="12" />
      </button>
      <span v-else class="inline-flex h-4 w-4 shrink-0 items-center justify-center text-slate-400">
        <Folder :size="14" />
      </span>

      <div class="min-w-0 flex-1 truncate leading-none">
        <slot name="label" :row="row">
          {{ row.displayName }}
        </slot>
      </div>

      <div class="flex shrink-0 items-center gap-2">
        <span
          v-if="row.isVirtual"
          class="rounded bg-violet-50 px-1.5 py-0.5 text-[10px] text-violet-600"
        >
          {{ virtualLabel || "Virtual" }}
        </span>
        <slot name="meta" :row="row" />
        <slot name="status" :row="row" />
      </div>
    </div>
  </div>
</template>
