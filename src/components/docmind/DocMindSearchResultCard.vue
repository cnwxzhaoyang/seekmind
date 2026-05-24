<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { Clock, Heart } from "lucide-vue-next";
import DocMindBadge from "./DocMindBadge.vue";
import DocMindHighlightedText from "./DocMindHighlightedText.vue";
import DocMindFileIcon from "./DocMindFileIcon.vue";
import type { SearchResultView } from "../../types/docmind";

const { t } = useI18n();

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

const locationLabel = computed(() => {
  if (props.item.page) {
    return t("searchResultCard.page", { page: props.item.page });
  }
  return t("searchResultCard.paragraph", { para: props.item.paragraph ?? 0 });
});

const matchedFieldLabel = computed(() => {
  return props.item.match_origin || t("searchResultCard.hitParagraph");
});

const favoriteTitle = computed(() => props.favorited ? t("searchResultCard.unfavorite") : t("searchResultCard.favorite"));
</script>

<template>
  <button
    class="w-full rounded-lg border bg-white p-2.5 text-left transition"
    :class="props.selected ? 'border-indigo-300 ring-1 ring-indigo-100' : 'border-slate-200 hover:border-indigo-300'"
    @click="emit('select')"
  >
    <div class="flex gap-2.5">
      <DocMindFileIcon :ext="item.ext" />
      <div class="min-w-0 flex-1">
        <div class="flex items-start justify-between gap-2">
          <div class="truncate text-[13px] font-semibold text-indigo-600">
            <DocMindHighlightedText :text="item.fileName" :query="props.query" />
          </div>
          <div class="flex items-center gap-2">
            <button
              class="rounded-md p-1 text-slate-400 transition hover:bg-slate-100 hover:text-slate-950"
              type="button"
              :title="favoriteTitle"
              @click.stop="emit('toggleFavorite')"
            >
              <Heart :size="14" :class="props.favorited ? 'fill-rose-500 text-rose-500' : ''" />
            </button>
            <div class="text-[11px] font-medium text-slate-500">{{ Math.round(item.score * 100) }}%</div>
          </div>
        </div>
        <div class="mt-1 truncate text-[11px] text-slate-400">{{ item.path }}</div>
        <div class="mt-2 text-sm leading-6 text-slate-700">
          <DocMindHighlightedText :text="item.snippet" :query="props.query" :spans="item.highlight_spans" />
        </div>
        <div class="mt-2 flex flex-wrap items-center gap-1.5 text-[11px] text-slate-500">
          <DocMindBadge>
            <DocMindHighlightedText :text="item.heading" :query="props.query" />
          </DocMindBadge>
          <span>{{ t("searchResultCard.matchField", { field: matchedFieldLabel }) }}</span>
          <span>·</span>
          <span>{{ t("searchResultCard.rankReason") }}</span>
          <span>·</span>
          <span class="text-slate-700">{{ item.rank_reason.summary || t("common.none") }}</span>
          <span>·</span>
          <span>{{ item.snippet_window_start }}-{{ item.snippet_window_end }} / {{ item.snippet_source_len }}</span>
          <span>·</span>
          <span>{{ locationLabel }}</span>
          <span>·</span>
          <span>{{ t("searchResultCard.hitSnippet") }}</span>
          <span>·</span>
          <span><Clock class="mr-1 inline" :size="12" />{{ item.modified }}</span>
        </div>
        <div class="mt-1.5 flex flex-wrap items-center gap-1.5 text-[11px] text-slate-500">
          <span>{{ t("searchResultCard.scoreKeywords", { score: item.rank_reason.keyword_score.toFixed(2) }) }}</span>
          <span>·</span>
          <span>{{ t("searchResultCard.scoreSemantic", { score: item.rank_reason.semantic_score.toFixed(2) }) }}</span>
          <span>·</span>
          <span>{{ t("searchResultCard.scoreTitle", { score: item.rank_reason.title_score.toFixed(2) }) }}</span>
          <span>·</span>
          <span>{{ t("searchResultCard.scoreFilename", { score: item.rank_reason.filename_score.toFixed(2) }) }}</span>
          <span>·</span>
          <span>{{ t("searchResultCard.scorePreference", { score: item.rank_reason.preference_score.toFixed(2) }) }}</span>
        </div>
        <div v-if="item.rank_reason.boosts.length > 0" class="mt-1.5 text-[11px] text-slate-500">
          {{ item.rank_reason.boosts.join(" · ") }}
        </div>
        <div class="mt-1 text-[11px] text-slate-500">
          {{ t("searchResultCard.rankChange", { original: item.rank_reason.original_rank, final: item.rank_reason.final_rank, delta: item.rank_reason.rank_delta }) }}
        </div>
      </div>
    </div>
  </button>
</template>
