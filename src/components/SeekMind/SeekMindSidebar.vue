<script setup lang="ts">
/**
 * @author MorningSun
 * @CreatedDate 2026/06/02
 * @Description SeekMind 侧边栏，承载全局导航、快捷访问与品牌入口。
 */
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";
import { BookMarked, ChevronLeft, Database, FileText, History, Layers3, MessageSquareText, Search, Settings, Star, Trash2 } from "lucide-vue-next";
import { useQuickAccessData } from "../../composables/useQuickAccessData";
import { useSidebarState } from "../../composables/useSidebarState";
import { seekMindApi } from "../../services/seekMindApi";
import { listenQuickAccessUpdated } from "../../utils/quickAccessEvents";
import brandIconUrl from "../../assets/app_icon_64x64.png";

const { t } = useI18n();
const route = useRoute();
const router = useRouter();
const { sidebarCollapsed, toggleSidebar } = useSidebarState();
const { quickDirs, searchHistory, recentDocuments, favorites, loadQuickAccessData } = useQuickAccessData();
const panelActionTarget = ref("");
let unlistenQuickAccessUpdated: null | (() => void) = null;
const sidebarStats = computed(() => [
  { label: t("sidebar.statsDirs"), value: quickDirs.value.length },
  { label: t("sidebar.statsRecent"), value: recentDocuments.value.length },
  { label: t("sidebar.statsFavorites"), value: favorites.value.length },
]);

const items = computed(() => [
  { key: "search", label: t("sidebar.search"), icon: Search, to: "/" },
  { key: "qa", label: t("sidebar.qa"), icon: MessageSquareText, to: "/qa" },
  { key: "collections", label: t("sidebar.collections"), icon: BookMarked, to: "/collections" },
  { key: "chunks", label: t("sidebar.chunks"), icon: Layers3, to: "/chunks" },
  { key: "status", label: t("sidebar.status"), icon: Database, to: "/status" },
  { key: "settings", label: t("sidebar.settings"), icon: Settings, to: "/settings" },
]);

const activeKey = computed(() => {
  const path = route.path;
  if (path.startsWith("/qa")) return "qa";
  if (path.startsWith("/collections")) return "collections";
  if (path.startsWith("/chunks")) return "chunks";
  if (path.startsWith("/status")) return "status";
  if (path.startsWith("/settings")) return "settings";
  return "search";
});

const favoriteResults = computed(() => favorites.value.filter((favorite) => favorite.favorite_type === "result"));

const openSearchQuery = async (query: string) => {
  await router.push({ path: "/", query: { q: query } });
};

const openRecentDocument = async (path: string) => {
  await seekMindApi.openFile(path);
  await loadQuickAccessData();
};

const openFavoriteDocument = async (path: string) => {
  await seekMindApi.openFile(path);
  await loadQuickAccessData();
};

const removeSearchHistory = async (query: string) => {
  panelActionTarget.value = `history:${query}`;
  try {
    await seekMindApi.removeSearchHistory(query);
    await loadQuickAccessData();
  } finally {
    panelActionTarget.value = "";
  }
};

const removeRecentDocument = async (path: string) => {
  panelActionTarget.value = `recent:${path}`;
  try {
    await seekMindApi.removeRecentDocument(path);
    await loadQuickAccessData();
  } finally {
    panelActionTarget.value = "";
  }
};

const removeFavorite = async (target: string) => {
  panelActionTarget.value = `favorite:${target}`;
  try {
    await seekMindApi.removeFavorite(target);
    await loadQuickAccessData();
  } finally {
    panelActionTarget.value = "";
  }
};

onMounted(() => {
  void loadQuickAccessData();
  unlistenQuickAccessUpdated = listenQuickAccessUpdated(() => {
    void loadQuickAccessData();
  });
});

onBeforeUnmount(() => {
  unlistenQuickAccessUpdated?.();
  unlistenQuickAccessUpdated = null;
});
</script>

<template>
  <aside
    class="seekmind-sidebar-shell flex h-full flex-col overflow-hidden border-r border-default p-2 transition-[width] duration-200 backdrop-blur-xl"
    :class="sidebarCollapsed ? 'w-[68px]' : 'w-[240px]'"
  >
    <div class="mb-3 rounded-[16px] border border-default bg-surface/80 px-2.5 py-2 shadow-card">
      <div :class="sidebarCollapsed ? 'flex items-center justify-center' : 'flex items-center justify-between gap-2'">
        <button
          v-if="sidebarCollapsed"
          class="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] transition hover:bg-surface-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/20"
          type="button"
          :title="t('sidebar.expand')"
          :aria-label="t('sidebar.expand')"
          @click="toggleSidebar"
        >
          <img
            :src="brandIconUrl"
            alt="SeekMind"
            class="h-9 w-9 shrink-0 rounded-[10px] object-contain shadow-card"
          >
        </button>
        <div v-else class="flex min-w-0 items-center gap-2">
          <img
            :src="brandIconUrl"
            alt="SeekMind"
            class="h-8 w-8 shrink-0 rounded-[10px] object-contain shadow-card"
          >
          <div v-if="!sidebarCollapsed" class="min-w-0">
            <div class="truncate text-[14px] font-semibold leading-5 text-primary">SeekMind</div>
            <div class="truncate text-[10px] leading-4 text-muted">{{ t("sidebar.brandSubtitle") }}</div>
          </div>
        </div>
        <button
          v-if="!sidebarCollapsed"
          class="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted transition hover:bg-surface-hover hover:text-primary"
          :title="t('sidebar.collapse')"
          :aria-label="t('sidebar.collapse')"
          @click="toggleSidebar"
        >
          <ChevronLeft :size="15" />
        </button>
      </div>
    </div>

    <nav :class="sidebarCollapsed ? 'space-y-1' : 'space-y-1.5'">
      <RouterLink
        v-for="item in items"
        :key="item.key"
        :to="item.to"
        class="group relative flex h-10 w-full items-center rounded-[12px] text-[13px] transition"
        :class="[
          sidebarCollapsed ? 'justify-center px-2' : 'gap-2.5 px-3',
          activeKey === item.key
            ? 'bg-[#007AFF] !text-white shadow-card'
            : '!text-secondary hover:bg-surface-hover hover:!text-primary',
        ]"
        :title="sidebarCollapsed ? item.label : undefined"
        :aria-label="item.label"
      >
        <component
          :is="item.icon"
          :size="17"
          class="shrink-0"
          :class="activeKey === item.key ? 'text-white' : 'text-current'"
        />
        <span v-if="!sidebarCollapsed" class="min-w-0 flex-1 truncate text-left leading-none">{{ item.label }}</span>
      </RouterLink>
    </nav>

    <div v-if="!sidebarCollapsed" class="mt-1 min-h-0 flex-1 overflow-hidden">
      <!-- 侧栏内容区压缩密度，改成扁平分区，避免每块都像独立卡片。 -->
      <div class="grid h-full min-h-0 grid-rows-[minmax(0,0.68fr)_minmax(0,0.68fr)_minmax(0,0.68fr)_minmax(0,0.56fr)] gap-2 overflow-hidden pr-1">
        <section class="flex min-h-0 flex-col overflow-hidden">
          <div class="flex items-center gap-1.5 border-b border-default pb-2 text-[11px] font-medium text-primary">
            <History :size="12" />
            {{ t("page.appSearch.section.recentSearch") }}
          </div>
          <div class="min-h-0 flex-1 overflow-y-auto pt-2">
            <div v-if="searchHistory.length === 0" class="rounded-md border border-dashed border-default bg-surface px-3 py-3 text-[12px] text-muted">
              {{ t("page.appSearch.section.noHistory") }}
            </div>
            <div v-else class="space-y-1">
              <div
                v-for="item in searchHistory"
                :key="item.query"
                class="group flex items-center gap-1"
              >
                <button
                  class="min-w-0 flex-1 rounded-md px-2 py-1 text-left text-[12px] leading-5 text-secondary transition hover:bg-panel hover:text-primary"
                  :title="item.query"
                  @click="openSearchQuery(item.query)"
                >
                  <div class="truncate text-[12px] font-medium leading-5 text-primary">{{ item.query }}</div>
                  <div class="mt-0.5 truncate text-[11px] leading-4 text-muted">{{ item.last_hit_at }}</div>
                </button>
                <button
                  class="inline-flex h-[22px] w-[22px] shrink-0 items-center justify-center rounded-md text-muted opacity-0 transition hover:bg-surface-hover hover:text-danger group-hover:opacity-100"
                  :title="t('page.appSearch.section.remove')"
                  :disabled="panelActionTarget === `history:${item.query}`"
                  @click.stop="removeSearchHistory(item.query)"
                >
                  <Trash2 :size="12" />
                </button>
              </div>
            </div>
          </div>
        </section>

        <section class="flex min-h-0 flex-col overflow-hidden">
          <div class="flex items-center gap-1.5 border-b border-default pb-2 text-[11px] font-medium text-primary">
            <FileText :size="12" />
            {{ t("page.appSearch.section.recentOpen") }}
          </div>
          <div class="min-h-0 flex-1 overflow-y-auto pt-2">
            <div v-if="recentDocuments.length === 0" class="rounded-md border border-dashed border-default bg-surface px-3 py-3 text-[12px] text-muted">
              {{ t("page.appSearch.section.noRecent") }}
            </div>
            <div v-else class="space-y-1">
              <div
                v-for="item in recentDocuments"
                :key="item.path"
                class="group flex items-start gap-1"
              >
                <button
                  class="min-w-0 flex-1 rounded-md px-2 py-1 text-left text-[12px] leading-5 text-secondary transition hover:bg-panel hover:text-primary"
                  :title="t('page.appSearch.section.recentOpenTip', { title: item.title, path: item.path })"
                  @click="openRecentDocument(item.path)"
                >
                  <div class="truncate text-[12px] font-medium leading-5 text-primary">{{ item.title }}</div>
                  <div class="mt-0.5 truncate text-[11px] leading-4 text-muted">{{ item.path }}</div>
                </button>
                <button
                  class="mt-0.5 inline-flex h-[22px] w-[22px] shrink-0 items-center justify-center rounded-md text-muted opacity-0 transition hover:bg-surface-hover hover:text-danger group-hover:opacity-100"
                  :title="t('page.appSearch.section.remove')"
                  :disabled="panelActionTarget === `recent:${item.path}`"
                  @click.stop="removeRecentDocument(item.path)"
                >
                  <Trash2 :size="12" />
                </button>
              </div>
            </div>
          </div>
        </section>

        <section class="flex min-h-0 flex-col overflow-hidden">
          <div class="flex items-center gap-1.5 border-b border-default pb-2 text-[11px] font-medium text-primary">
            <Star :size="12" />
            {{ t("page.appSearch.section.favorites") }}
          </div>
          <div class="min-h-0 flex-1 overflow-y-auto pt-2">
            <div v-if="favoriteResults.length === 0" class="rounded-md border border-dashed border-default bg-surface px-3 py-3 text-[12px] text-muted">
              {{ t("page.appSearch.section.noFavorites") }}
            </div>
            <div v-else class="space-y-1">
              <div
                v-for="item in favoriteResults"
                :key="item.target"
                class="group flex items-start gap-1"
              >
                <button
                  class="min-w-0 flex-1 rounded-md px-2 py-1 text-left text-[12px] leading-5 text-secondary transition hover:bg-panel hover:text-primary"
                  :title="t('page.appSearch.section.favoriteTip', { title: item.title, path: item.path })"
                  @click="openFavoriteDocument(item.path)"
                >
                  <div class="truncate text-[12px] font-medium leading-5 text-primary">{{ item.title }}</div>
                  <div class="mt-0.5 truncate text-[11px] leading-4 text-muted">{{ item.path }}</div>
                </button>
                <button
                  class="mt-0.5 inline-flex h-[22px] w-[22px] shrink-0 items-center justify-center rounded-md text-muted opacity-0 transition hover:bg-surface-hover hover:text-danger group-hover:opacity-100"
                  :title="t('page.appSearch.section.remove')"
                  :disabled="panelActionTarget === `favorite:${item.target}`"
                  @click.stop="removeFavorite(item.target)"
                >
                  <Trash2 :size="12" />
                </button>
              </div>
            </div>
          </div>
        </section>

        <div class="mt-auto border-t border-default pt-2">
          <div class="flex items-center justify-between gap-2 text-[11px] text-muted">
            <div class="flex min-w-0 items-center gap-3">
              <span class="inline-flex items-center gap-1">
                <span class="h-1.5 w-1.5 rounded-full bg-success" />
                {{ t("sidebar.statusRunning") }}
              </span>
            </div>
            <div class="flex items-center gap-2">
              <span class="inline-flex items-center gap-1 rounded-full bg-surface/70 px-2 py-0.5">
                {{ sidebarStats[0].value }}
                <span class="text-muted">{{ sidebarStats[0].label }}</span>
              </span>
              <span class="inline-flex items-center gap-1 rounded-full bg-surface/70 px-2 py-0.5">
                {{ sidebarStats[1].value }}
                <span class="text-muted">{{ sidebarStats[1].label }}</span>
              </span>
              <span class="inline-flex items-center gap-1 rounded-full bg-surface/70 px-2 py-0.5">
                {{ sidebarStats[2].value }}
                <span class="text-muted">{{ sidebarStats[2].label }}</span>
              </span>
            </div>
          </div>
        </div>
      </div>
    </div>

  </aside>
</template>
