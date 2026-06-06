<!--
  @author MorningSun
  @CreatedDate 2026/06/03
  @Description 搜索结果片段卡片，负责列表展示、选中和收藏操作。
-->
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
  nested?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  selected: false,
  query: "",
  favorited: false,
  nested: false,
});

const emit = defineEmits<{
  select: [event: MouseEvent | KeyboardEvent];
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

const debugClick = (event: MouseEvent | KeyboardEvent) => {
  if (globalThis.localStorage?.getItem("DOCMIND_DEBUG_SEARCH_CLICK") !== "1") {
    return;
  }

  console.debug("[DocMindSearchResultCard] row click", {
    id: props.item.id,
    path: props.item.path,
    selected: props.selected,
    eventType: event.type,
    target: event.target instanceof HTMLElement ? event.target.tagName : null,
  });
};

const emitSelect = (event: MouseEvent | KeyboardEvent) => {
  debugClick(event);
  emit("select", event);
};
</script>

<template>
  <div
    class="w-full cursor-pointer rounded-lg border p-2.5 text-left transition"
    :class="[
      props.nested
        ? props.selected
          ? 'border-transparent bg-accent-soft'
          : 'border-transparent bg-transparent hover:bg-surface-hover/50'
        : 'border-default bg-surface hover:border-accent',
      !props.nested && props.selected ? 'bg-accent-soft' : '',
    ]"
    role="button"
    tabindex="0"
    @click="emitSelect($event)"
    @keydown.enter.prevent="emitSelect($event)"
    @keydown.space.prevent="emitSelect($event)"
  >
    <div class="flex gap-2.5">
      <DocMindFileIcon :ext="item.ext" />
      <div class="min-w-0 flex-1">
        <div class="flex items-start justify-between gap-2">
          <div class="truncate text-[13px] font-semibold" :class="props.nested ? 'text-primary' : 'text-accent-text'">
            <DocMindHighlightedText :text="item.file_name" :query="props.query" />
          </div>
          <div class="flex items-center gap-2">
            <button
              class="rounded-md p-1 text-muted transition hover:bg-surface-hover hover:text-primary"
              type="button"
              :title="favoriteTitle"
              @click.stop="emit('toggleFavorite')"
            >
              <Heart :size="14" :class="props.favorited ? 'fill-danger text-danger' : ''" />
            </button>
            <div class="text-[11px] font-medium text-dim">{{ Math.round(item.score * 100) }}%</div>
          </div>
        </div>
        <div class="mt-1 truncate text-[11px] text-muted">{{ item.path }}</div>
        <div v-if="item.title_path || item.heading" class="mt-1 text-[11px] text-dim">
          {{ t("page.appSearch.detail.titlePath") }}：<DocMindHighlightedText :text="item.title_path || item.heading" :query="props.query" />
        </div>
        <div class="mt-2 text-sm leading-6" :class="props.nested ? 'text-secondary' : 'text-secondary'">
          <DocMindHighlightedText :text="item.snippet" :query="props.query" :spans="item.highlight_spans" />
        </div>
        <div class="mt-2 flex flex-wrap items-center gap-1.5 text-[11px] text-dim">
          <DocMindBadge>{{ props.nested ? "片段" : t("page.appSearch.detail.titlePath") }}</DocMindBadge>
          <span>{{ t("searchResultCard.matchField", { field: matchedFieldLabel }) }}</span>
          <span>·</span>
          <span>{{ t("searchResultCard.rankReason") }}</span>
          <span>·</span>
          <span class="text-secondary">{{ item.rank_reason.summary || t("common.none") }}</span>
          <span>·</span>
          <span>{{ item.snippet_window_start }}-{{ item.snippet_window_end }} / {{ item.snippet_source_len }}</span>
          <span>·</span>
          <span>{{ locationLabel }}</span>
          <span>·</span>
          <span>{{ t("searchResultCard.hitSnippet") }}</span>
          <span>·</span>
          <span><Clock class="mr-1 inline" :size="12" />{{ item.modified }}</span>
        </div>
        <div class="mt-1.5 flex flex-wrap items-center gap-1.5 text-[11px] text-dim">
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
        <div v-if="item.rank_reason.boosts.length > 0" class="mt-1.5 text-[11px] text-dim">
          {{ item.rank_reason.boosts.join(" · ") }}
        </div>
        <div class="mt-1 text-[11px] text-dim">
          {{ t("searchResultCard.rankChange", { original: item.rank_reason.original_rank, final: item.rank_reason.final_rank, delta: item.rank_reason.rank_delta }) }}
        </div>
      </div>
    </div>
  </div>
</template>
