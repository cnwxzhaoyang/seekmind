<script setup lang="ts">
import { computed } from "vue";
import { ChevronDown, ChevronRight } from "lucide-vue-next";
import DocMindBadge from "./DocMindBadge.vue";
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
}>();

const densityClasses = computed(() => {
  if (props.density === "compact") {
    return "px-2 py-1.5 text-[11px]";
  }
  if (props.density === "relaxed") {
    return "px-3 py-2.5 text-sm";
  }
  return "px-2.5 py-2 text-sm";
});

const handleSelect = (path: string) => {
  if (props.selectable === false) {
    return;
  }

  emit("node-select", path);
};

const handleKeydown = (event: KeyboardEvent, path: string) => {
  if (props.selectable === false) {
    return;
  }

  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    emit("node-select", path);
  }
};
</script>

<template>
  <div v-if="rows.length === 0" class="rounded-md border border-dashed border-slate-200 bg-white px-4 py-6 text-xs text-slate-400">
    <slot name="empty">
      {{ emptyText || "No items" }}
    </slot>
  </div>
  <div v-else class="space-y-1">
    <div
      v-for="row in rows"
      :key="row.dir.path"
      class="group w-full rounded-md border bg-white text-left transition"
      :class="[
        densityClasses,
        selectedPath === row.dir.path ? 'border-indigo-300 bg-indigo-50 ring-1 ring-indigo-100' : 'border-slate-200 hover:bg-slate-50',
        selectable === false ? 'cursor-default' : 'cursor-pointer select-none',
      ]"
      :style="{ paddingLeft: `${(nodePaddingBase ?? 10) + row.depth * (nodePaddingStep ?? 14)}px` }"
      :title="pathTooltip === false ? '' : row.fullPath"
      :role="selectable === false ? undefined : 'button'"
      :tabindex="selectable === false ? undefined : 0"
      @click="handleSelect(row.dir.path)"
      @keydown="handleKeydown($event, row.dir.path)"
    >
      <div class="flex items-start gap-2">
        <button
          v-if="row.hasChildren"
          class="mt-0.5 inline-flex h-4 w-4 shrink-0 items-center justify-center rounded text-slate-500 hover:bg-slate-100"
          type="button"
          :title="row.expanded ? (collapseTitle || 'Collapse') : (expandTitle || 'Expand')"
          :aria-expanded="row.expanded"
          @click.stop="emit('toggle', row.dir.path, !row.expanded)"
        >
          <ChevronDown v-if="row.expanded" :size="12" />
          <ChevronRight v-else :size="12" />
        </button>
        <span v-else class="inline-flex h-4 w-4 shrink-0" />

        <div class="min-w-0 flex-1">
          <div class="truncate font-medium text-slate-950" :class="props.density === 'compact' ? 'text-[13px]' : 'text-sm'">
            <slot name="label" :row="row">
              {{ row.displayName }}
            </slot>
          </div>
          <div class="mt-1 flex items-center justify-between gap-2 text-[11px] text-slate-500">
            <div class="min-w-0 truncate">
              <slot name="meta" :row="row">
                {{ row.fullPath }}
              </slot>
            </div>
            <div class="flex shrink-0 items-center gap-1.5">
              <span
                v-if="row.isVirtual"
                class="rounded-full bg-violet-50 px-1.5 py-0.5 text-[10px] text-violet-700"
              >
                {{ virtualLabel || "Virtual Directory" }}
              </span>
              <slot name="status" :row="row">
                <DocMindBadge :tone="row.dir.enabled ? 'success' : 'default'">
                  {{ row.dir.enabled ? "Enabled" : "Disabled" }}
                </DocMindBadge>
              </slot>
            </div>
          </div>
        </div>

        <div class="shrink-0">
          <slot name="actions" :row="row" />
        </div>
      </div>
    </div>
  </div>
</template>
