<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { ChevronDown, ChevronRight } from "lucide-vue-next";
import DocMindBadge from "./DocMindBadge.vue";
import DocMindFileIcon from "./DocMindFileIcon.vue";
import DocMindHighlightedText from "./DocMindHighlightedText.vue";
import DocMindSearchResultCard from "./DocMindSearchResultCard.vue";
import type { SearchResultView } from "../../types/docmind";

const { t } = useI18n();

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
  <section
    class="rounded-lg border bg-white p-2.5 transition hover:border-indigo-400 hover:shadow-sm"
    :class="isSelected ? 'border-indigo-300 ring-1 ring-indigo-100' : 'border-slate-200'"
  >
    <div class="flex items-start gap-2.5">
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
          <div class="flex items-start justify-between gap-2.5">
            <div class="min-w-0">
              <div class="truncate text-[13px] font-semibold text-indigo-600">
                <DocMindHighlightedText :text="group.fileName" :query="props.query" />
              </div>
              <div class="mt-1 break-all text-[11px] text-slate-400">{{ group.path }}</div>
            </div>
            <div class="text-right text-[11px] text-slate-400">
              <div class="font-medium text-slate-700">{{ Math.round(group.topResult.score * 100) }}%</div>
              <div class="mt-1">{{ t("searchResultGroupCard.segments", { count: group.count }) }}</div>
            </div>
          </div>
        </div>

        <div class="mt-2 flex flex-wrap items-center gap-1.5 text-[11px] text-slate-500">
          <DocMindBadge>{{ group.ext.toUpperCase() }}</DocMindBadge>
          <span>{{ t("searchResultGroupCard.docGroup") }}</span>
          <span>·</span>
          <span>{{ t("searchResultGroupCard.hitSnippets", { count: group.count }) }}</span>
          <span>·</span>
          <span>{{ t("searchResultGroupCard.totalScore", { score: Math.round(group.totalScore * 100) }) }}</span>
        </div>

        <div class="mt-2 text-sm leading-6 text-slate-700">
          <DocMindHighlightedText :text="group.topResult.snippet" :query="props.query" :spans="group.topResult.highlight_spans" />
        </div>
        <div class="mt-1.5 text-[11px] text-slate-500">
          {{ t("searchResultCard.rankReason") }}: {{ group.topResult.rank_reason.summary || t("common.none") }}
        </div>

        <div class="mt-2.5 flex items-center justify-between">
          <button
            class="inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium text-slate-600 transition hover:bg-slate-50"
            type="button"
            @click="emit('toggle', group.path)"
          >
            <ChevronDown v-if="props.expanded" :size="14" />
            <ChevronRight v-else :size="14" />
            {{ props.expanded ? t("searchResultGroupCard.collapse") : t("searchResultGroupCard.expand") }}
          </button>
          <button
            class="rounded-md px-2 py-1 text-xs font-medium text-slate-500 transition hover:bg-slate-50"
            type="button"
            @click="emit('select', group.topResult.id)"
          >
            {{ t("searchResultGroupCard.openFirst") }}
          </button>
        </div>
      </div>
    </div>

  <div v-if="props.expanded" class="mt-4 space-y-2 border-t border-slate-100 pt-3">
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
