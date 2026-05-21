<script setup lang="ts">
import { computed } from "vue";
import { Clock } from "lucide-vue-next";
import DocMindBadge from "./DocMindBadge.vue";
import DocMindFileIcon from "./DocMindFileIcon.vue";
import type { SearchResultView } from "../../types/docmind";

interface Props {
  item: SearchResultView;
  selected?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  selected: false,
});

const emit = defineEmits<{
  select: [];
}>();

const locationLabel = computed(() => (props.item.page ? `第 ${props.item.page} 页` : `第 ${props.item.paragraph ?? 0} 段`));
</script>

<template>
  <button
    class="w-full rounded-3xl border p-4 text-left shadow-sm transition"
    :class="props.selected ? 'border-slate-300 bg-white ring-2 ring-slate-200' : 'border-slate-200 bg-white hover:border-slate-300'"
    @click="emit('select')"
  >
    <div class="flex gap-3">
      <DocMindFileIcon :ext="item.ext" />
      <div class="min-w-0 flex-1">
        <div class="flex items-start justify-between gap-2">
          <div class="truncate text-sm font-semibold text-slate-900">{{ item.fileName }}</div>
          <div class="text-xs text-slate-400">{{ Math.round(item.score * 100) }}%</div>
        </div>
        <div class="mt-1 truncate text-xs text-slate-400">{{ item.path }}</div>
        <div class="mt-2 text-sm leading-6 text-slate-700">{{ item.snippet }}</div>
        <div class="mt-3 flex items-center gap-2 text-xs text-slate-500">
          <DocMindBadge>{{ item.heading }}</DocMindBadge>
          <span>{{ locationLabel }}</span>
          <span>·</span>
          <span><Clock class="mr-1 inline" :size="12" />{{ item.modified }}</span>
        </div>
      </div>
    </div>
  </button>
</template>
