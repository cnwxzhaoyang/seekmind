<!--
  @author MorningSun
  @CreatedDate 2026/06/03
  @Description 搜索结果文档分组卡片，负责文档级命中展示和展开片段列表。
-->
<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { PanelTopClose, PanelTopOpen } from "lucide-vue-next";
import SeekMindBadge from "./SeekMindBadge.vue";
import SeekMindFileIcon from "./SeekMindFileIcon.vue";
import SeekMindHighlightedText from "./SeekMindHighlightedText.vue";
import SeekMindSearchResultCard from "./SeekMindSearchResultCard.vue";
import type { SearchResultView } from "../../types/SeekMind";

const { t } = useI18n();

interface SearchResultGroup {
  path: string;
  file_name: string;
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
  contextmenu: [event: MouseEvent];
}>();

const isSelected = computed(() => props.group.results.some((item) => item.id === props.selectedId));

const debugClick = (event: MouseEvent | KeyboardEvent, source: string, id: string) => {
  if (globalThis.localStorage?.getItem("SEEKMIND_DEBUG_SEARCH_CLICK") !== "1") {
    return;
  }

  console.debug("[SeekMindSearchResultGroupCard] select", {
    source,
    id,
    path: props.group.path,
    selectedId: props.selectedId,
    eventType: event.type,
    target: event.target instanceof HTMLElement ? event.target.tagName : null,
  });
};

const emitSelect = (id: string, source: string, event: MouseEvent | KeyboardEvent) => {
  debugClick(event, source, id);
  emit("select", id);
};

const emitContextMenu = (event: MouseEvent) => {
  emit("select", props.group.topResult.id);
  emit("contextmenu", event);
};
</script>

<template>
  <section
    class="cursor-pointer rounded-lg border-t border-l border-light bg-surface p-3 shadow-card transition hover:shadow-card-hover"
    :class="isSelected ? 'bg-accent-soft' : 'hover:bg-surface-hover'"
    role="button"
    tabindex="0"
    @click="emitSelect(group.topResult.id, 'group-card', $event)"
    @keydown.enter.prevent="emitSelect(group.topResult.id, 'group-card-key', $event)"
    @keydown.space.prevent="emitSelect(group.topResult.id, 'group-card-key', $event)"
    @contextmenu.prevent="emitContextMenu"
  >
    <div class="flex items-start gap-2.5">
      <SeekMindFileIcon :ext="group.ext" />
      <div class="min-w-0 flex-1">
        <div class="w-full text-left">
          <div class="flex items-start justify-between gap-2.5">
            <div class="min-w-0">
              <div class="truncate text-[13px] font-semibold text-accent-text">
                <SeekMindHighlightedText :text="group.file_name" :query="props.query" />
              </div>
              <div class="mt-1 break-all text-[11px] text-muted">{{ group.path }}</div>
              <div v-if="group.topResult.title_path || group.topResult.heading" class="mt-1 text-[11px] text-dim">
                {{ t("page.appSearch.detail.titlePath") }}：<SeekMindHighlightedText :text="group.topResult.title_path || group.topResult.heading" :query="props.query" />
              </div>
            </div>
            <div class="text-right text-[11px] text-muted">
              <div class="font-medium text-secondary">{{ Math.round(group.topResult.score * 100) }}%</div>
              <div class="mt-1">{{ t("searchResultGroupCard.segments", { count: group.count }) }}</div>
            </div>
          </div>
        </div>

        <div class="mt-2 flex flex-wrap items-center gap-1.5 text-[11px] text-dim">
          <SeekMindBadge>{{ group.ext.toUpperCase() }}</SeekMindBadge>
          <span>{{ t("searchResultGroupCard.docGroup") }}</span>
          <span>·</span>
          <span>{{ t("searchResultGroupCard.hitSnippets", { count: group.count }) }}</span>
          <span>·</span>
          <span>{{ t("searchResultGroupCard.totalScore", { score: Math.round(group.totalScore * 100) }) }}</span>
        </div>

        <div class="mt-2 text-sm leading-6 text-secondary">
          <SeekMindHighlightedText :text="group.topResult.snippet" :query="props.query" :spans="group.topResult.highlight_spans" />
        </div>

        <div class="mt-3 flex flex-wrap items-center justify-between gap-3 border-t border-light pt-3">
          <div class="flex flex-wrap items-center gap-2 text-[11px] text-dim">
            <SeekMindBadge>{{ group.ext.toUpperCase() }}</SeekMindBadge>
            <span>{{ t("searchResultGroupCard.segments", { count: group.count }) }}</span>
            <span>·</span>
            <span>{{ t("searchResultGroupCard.totalScore", { score: Math.round(group.totalScore * 100) }) }}</span>
            <span>·</span>
            <span>{{ t("searchResultCard.rankReason") }}: {{ group.topResult.rank_reason.summary || t("common.none") }}</span>
          </div>
          <div class="flex items-center gap-1.5">
            <button
              class="inline-flex h-8 w-8 items-center justify-center rounded-md border border-default bg-surface text-secondary transition hover:bg-surface-hover hover:text-primary"
              type="button"
              :title="props.expanded ? t('searchResultGroupCard.collapse') : t('searchResultGroupCard.expand')"
              :aria-label="props.expanded ? t('searchResultGroupCard.collapse') : t('searchResultGroupCard.expand')"
              @click.stop="emit('toggle', group.path)"
            >
              <PanelTopClose v-if="props.expanded" :size="16" />
              <PanelTopOpen v-else :size="16" />
            </button>
          </div>
        </div>
      </div>
    </div>

    <div v-if="props.expanded" class="mt-4 border-l border-light pl-3" @click.stop>
      <div class="mb-2 flex items-center justify-between text-[11px] text-dim">
        <div class="flex items-center gap-2">
          <span class="font-semibold uppercase tracking-[0.16em] text-secondary">片段列表</span>
          <span class="text-dim">·</span>
          <span>{{ t("searchResultGroupCard.segments", { count: group.count }) }}</span>
        </div>
        <span class="text-dim">{{ t("searchResultGroupCard.expand") }}</span>
      </div>
      <div class="space-y-1.5">
        <SeekMindSearchResultCard
          v-for="item in group.results"
          :key="item.id"
          :item="item"
          :selected="item.id === props.selectedId"
          :query="props.query"
          :favorited="props.isFavorited(item.path, item.heading, item.paragraph, item.page)"
          nested
          @select="emitSelect(item.id, 'child-card', $event)"
          @toggle-favorite="emit('toggleFavorite', item)"
        />
      </div>
    </div>
  </section>
</template>
