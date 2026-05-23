<script setup lang="ts">
import { computed } from "vue";
import { Clock, Heart } from "lucide-vue-next";
import DocMindBadge from "./DocMindBadge.vue";
import DocMindHighlightedText from "./DocMindHighlightedText.vue";
import DocMindFileIcon from "./DocMindFileIcon.vue";
import type { SearchResultView } from "../../types/docmind";

interface Props {
  item: SearchResultView;
  selected?: boolean;
  query?: string;
  favorited?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  selected: false,
  query: "",
  favorited: false,
});

const emit = defineEmits<{
  select: [];
  toggleFavorite: [];
}>();

const locationLabel = computed(() => (props.item.page ? `第 ${props.item.page} 页` : `第 ${props.item.paragraph ?? 0} 段`));

const matchedFieldLabel = computed(() => {
  return props.item.match_origin || "正文摘要命中";
});
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
          <div class="truncate text-sm font-semibold text-slate-900">
            <DocMindHighlightedText :text="item.fileName" :query="props.query" />
          </div>
          <div class="flex items-center gap-2">
            <button
              class="rounded-lg p-1 text-slate-400 hover:bg-slate-100 hover:text-slate-950"
              type="button"
              :title="props.favorited ? '取消收藏' : '收藏结果'"
              @click.stop="emit('toggleFavorite')"
            >
              <Heart :size="14" :class="props.favorited ? 'fill-rose-500 text-rose-500' : ''" />
            </button>
            <div class="text-xs text-slate-400">{{ Math.round(item.score * 100) }}%</div>
          </div>
        </div>
        <div class="mt-1 truncate text-xs text-slate-400">{{ item.path }}</div>
        <div class="mt-2 text-sm leading-6 text-slate-700">
          <DocMindHighlightedText :text="item.snippet" :query="props.query" :spans="item.highlight_spans" />
        </div>
        <div class="mt-3 flex flex-wrap items-center gap-2 text-xs text-slate-500">
          <DocMindBadge>
            <DocMindHighlightedText :text="item.heading" :query="props.query" />
          </DocMindBadge>
          <span>命中：{{ matchedFieldLabel }}</span>
          <span>·</span>
          <span>{{ item.snippet_window_start }}-{{ item.snippet_window_end }} / {{ item.snippet_source_len }}</span>
          <span>·</span>
          <span>{{ locationLabel }}</span>
          <span>·</span>
          <span>命中片段</span>
          <span>·</span>
          <span><Clock class="mr-1 inline" :size="12" />{{ item.modified }}</span>
        </div>
      </div>
    </div>
  </button>
</template>
