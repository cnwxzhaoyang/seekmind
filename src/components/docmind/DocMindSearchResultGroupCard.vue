<script setup lang="ts">
import { computed } from "vue";
import { ChevronDown, ChevronRight } from "lucide-vue-next";
import DocMindBadge from "./DocMindBadge.vue";
import DocMindFileIcon from "./DocMindFileIcon.vue";
import DocMindHighlightedText from "./DocMindHighlightedText.vue";
import DocMindSearchResultCard from "./DocMindSearchResultCard.vue";
import type { SearchResultView } from "../../types/docmind";

interface SearchResultGroup {
  path: string;
  fileName: string;
  ext: string;
  topResult: SearchResultView;
  results: SearchResultView[];
  count: number;
  totalScore: number;
}

interface Props {
  group: SearchResultGroup;
  query?: string;
  selectedId?: string;
  expanded?: boolean;
  isFavorited?: (path: string, heading: string, paragraph?: number | null, page?: number | null) => boolean;
}

const props = withDefaults(defineProps<Props>(), {
  query: "",
  selectedId: "",
  expanded: false,
  isFavorited: () => false,
});

const emit = defineEmits<{
  select: [id: string];
  toggle: [path: string];
  toggleFavorite: [item: SearchResultView];
}>();

const isSelected = computed(() => props.group.results.some((item) => item.id === props.selectedId));
</script>

<template>
  <section class="rounded-3xl border bg-white p-4 shadow-sm transition" :class="isSelected ? 'border-slate-300 ring-2 ring-slate-200' : 'border-slate-200'">
    <div class="flex items-start gap-3">
      <DocMindFileIcon :ext="group.ext" />
      <div class="min-w-0 flex-1">
        <div
          class="w-full cursor-pointer text-left"
          role="button"
          tabindex="0"
          @click="emit('select', group.topResult.id)"
          @keydown.enter.prevent="emit('select', group.topResult.id)"
          @keydown.space.prevent="emit('select', group.topResult.id)"
        >
          <div class="flex items-start justify-between gap-3">
            <div class="min-w-0">
              <div class="truncate text-sm font-semibold text-slate-900">
                <DocMindHighlightedText :text="group.fileName" :query="props.query" />
              </div>
              <div class="mt-1 break-all text-xs text-slate-400">{{ group.path }}</div>
            </div>
            <div class="text-right text-xs text-slate-400">
              <div>{{ Math.round(group.topResult.score * 100) }}%</div>
              <div class="mt-1">{{ group.count }} 段</div>
            </div>
          </div>
        </div>

        <div class="mt-3 flex flex-wrap items-center gap-2 text-xs text-slate-500">
          <DocMindBadge>{{ group.ext.toUpperCase() }}</DocMindBadge>
          <span>文档聚合</span>
          <span>·</span>
          <span>命中片段 {{ group.count }} 个</span>
          <span>·</span>
          <span>总分 {{ Math.round(group.totalScore * 100) }}%</span>
        </div>

        <div class="mt-3 rounded-2xl bg-slate-50 px-4 py-3 text-sm leading-6 text-slate-700">
          <DocMindHighlightedText :text="group.topResult.snippet" :query="props.query" :spans="group.topResult.highlight_spans" />
        </div>

        <div class="mt-3 flex items-center justify-between">
          <button
            class="inline-flex items-center gap-1 rounded-xl border border-slate-200 bg-white px-3 py-1.5 text-xs font-medium text-slate-600 hover:bg-slate-50"
            type="button"
            @click="emit('toggle', group.path)"
          >
            <ChevronDown v-if="props.expanded" :size="14" />
            <ChevronRight v-else :size="14" />
            {{ props.expanded ? "收起片段" : "展开片段" }}
          </button>
          <button
            class="rounded-xl px-3 py-1.5 text-xs font-medium text-slate-500 hover:bg-slate-50"
            type="button"
            @click="emit('select', group.topResult.id)"
          >
            打开首段
          </button>
        </div>
      </div>
    </div>

    <div v-if="props.expanded" class="mt-4 space-y-3 border-t border-slate-100 pt-4">
      <DocMindSearchResultCard
        v-for="item in group.results"
        :key="item.id"
        :item="item"
        :selected="item.id === props.selectedId"
        :query="props.query"
        :favorited="props.isFavorited(item.path, item.heading, item.paragraph, item.page)"
        @select="emit('select', item.id)"
        @toggle-favorite="emit('toggleFavorite', item)"
      />
    </div>
  </section>
</template>
