<script setup lang="ts">
import { computed } from "vue";
import { Folder, FolderOpen, FileText } from "lucide-vue-next";
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

const rowPadding = computed(() => {
  if (props.density === "compact") return "py-0.5";
  if (props.density === "relaxed") return "py-1.5";
  return "py-1";
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
  <div v-if="rows.length === 0" class="px-3 py-4 text-xs text-muted">
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
        rowPadding,
        selectedPath === row.dir.path ? 'bg-accent-soft' : 'hover:bg-surface-hover',
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
        class="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded text-muted hover:text-secondary"
        type="button"
        :title="row.expanded ? (collapseTitle || 'Collapse') : (expandTitle || 'Expand')"
        :aria-expanded="row.expanded"
        @click.stop="emit('toggle', row.dir.path, !row.expanded)"
      >
        <FolderOpen v-if="row.expanded" :size="15" />
        <Folder v-else :size="15" />
      </button>
      <span v-else class="inline-flex h-5 w-5 shrink-0 items-center justify-center text-muted">
        <Folder :size="15" />
      </span>

      <div class="docmind-item-title min-w-0 flex-1 truncate">
        <slot name="label" :row="row">
          {{ row.displayName }}
        </slot>
      </div>

      <div class="flex shrink-0 items-center gap-2 text-[11px] text-muted">
        <span v-if="row.isVirtual" class="rounded bg-badge px-1 py-px text-[10px] text-accent-text">
          {{ virtualLabel || "Virtual" }}
        </span>
        <slot name="meta" :row="row">
          <span class="inline-flex items-center gap-0.5" title="Documents">
            <FileText :size="10" />
            {{ row.dir.docs }}
          </span>
          <span class="inline-flex items-center gap-0.5" title="Chunks">
            <FileText :size="10" class="opacity-50" />
            {{ row.dir.chunks }}
          </span>
        </slot>
        <slot name="status" :row="row">
          <span
            class="inline-block h-1.5 w-1.5 rounded-full"
            :class="row.dir.enabled ? 'bg-success' : 'bg-muted'"
            :title="row.dir.enabled ? 'Enabled' : 'Disabled'"
          />
        </slot>
      </div>
    </div>
  </div>
</template>
