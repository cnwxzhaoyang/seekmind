<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";
import { ChevronLeft, ChevronRight, Database, FileText, FolderOpen, History, Layers3, Search, Settings, Star, Trash2 } from "lucide-vue-next";
import DocMindIndexTree from "./DocMindIndexTree.vue";
import { useIndexDirTree } from "../../composables/useIndexDirTree";
import { useQuickAccessData } from "../../composables/useQuickAccessData";
import { useSidebarState } from "../../composables/useSidebarState";
import { docmindApi } from "../../services/docmindApi";

const { t } = useI18n();
const route = useRoute();
const router = useRouter();
const { sidebarCollapsed, toggleSidebar } = useSidebarState();
const { quickDirs, searchHistory, recentDocuments, favorites, loadQuickAccessData } = useQuickAccessData();
const { visibleRows: visibleQuickDirRows, setExpanded: setQuickDirExpanded } = useIndexDirTree(quickDirs);
const panelActionTarget = ref("");

const items = computed(() => [
  { key: "search", label: t("sidebar.search"), icon: Search, to: "/" },
  { key: "chunks", label: t("sidebar.chunks"), icon: Layers3, to: "/chunks" },
  { key: "status", label: t("sidebar.status"), icon: Database, to: "/status" },
  { key: "settings", label: t("sidebar.settings"), icon: Settings, to: "/settings" },
]);

const activeKey = computed(() => {
  const path = route.path;
  if (path.startsWith("/chunks")) return "chunks";
  if (path.startsWith("/status")) return "status";
  if (path.startsWith("/settings")) return "settings";
  return "search";
});

const favoriteResults = computed(() => favorites.value.filter((favorite) => favorite.favorite_type === "result"));
const selectedIndexDirPath = computed(() => {
  if (route.path !== "/chunks") {
    return "";
  }

  const targetPath = typeof route.query.path === "string" ? route.query.path : "";
  const candidate = quickDirs.value
    .map((dir) => dir.path)
    .filter((dirPath) => targetPath.startsWith(dirPath))
    .sort((a, b) => b.length - a.length)[0];

  return candidate ?? "";
});

const openSearchQuery = async (query: string) => {
  await router.push({ path: "/", query: { q: query } });
};

const openIndexDir = async (path: string) => {
  await router.push({ path: "/chunks", query: { path } });
};

const openRecentDocument = async (path: string) => {
  await docmindApi.openFile(path);
  await loadQuickAccessData();
};

const openFavoriteDocument = async (path: string) => {
  await docmindApi.openFile(path);
  await loadQuickAccessData();
};

const removeSearchHistory = async (query: string) => {
  panelActionTarget.value = `history:${query}`;
  try {
    await docmindApi.removeSearchHistory(query);
    await loadQuickAccessData();
  } finally {
    panelActionTarget.value = "";
  }
};

const removeRecentDocument = async (path: string) => {
  panelActionTarget.value = `recent:${path}`;
  try {
    await docmindApi.removeRecentDocument(path);
    await loadQuickAccessData();
  } finally {
    panelActionTarget.value = "";
  }
};

const removeFavorite = async (target: string) => {
  panelActionTarget.value = `favorite:${target}`;
  try {
    await docmindApi.removeFavorite(target);
    await loadQuickAccessData();
  } finally {
    panelActionTarget.value = "";
  }
};

onMounted(() => {
  void loadQuickAccessData();
});
</script>

<template>
  <aside
    class="flex h-full flex-col overflow-hidden border-r border-default bg-sidebar p-2 transition-[width] duration-200"
    :class="sidebarCollapsed ? 'w-[68px]' : 'w-[240px]'"
  >
    <div class="mb-3 flex items-center justify-between gap-2 px-2 py-2">
      <div class="flex h-6 w-6 items-center justify-center rounded bg-accent text-xs font-bold text-white shadow-card">dm</div>
      <div v-if="!sidebarCollapsed" class="min-w-0 flex-1">
        <div class="text-base font-semibold text-primary">docMind</div>
      </div>
      <button
        class="inline-flex h-6 w-6 items-center justify-center rounded-md text-muted transition hover:bg-surface-active hover:text-primary"
        :title="sidebarCollapsed ? t('sidebar.expand') : t('sidebar.collapse')"
        :aria-label="sidebarCollapsed ? t('sidebar.expand') : t('sidebar.collapse')"
        @click="toggleSidebar"
      >
        <ChevronRight v-if="sidebarCollapsed" :size="15" />
        <ChevronLeft v-else :size="15" />
      </button>
    </div>

    <nav
      :class="sidebarCollapsed
        ? 'space-y-1'
        : 'grid grid-cols-2 gap-1 rounded-lg border border-default bg-surface/75 p-1 shadow-card'"
    >
      <RouterLink
        v-for="item in items"
        :key="item.key"
        :to="item.to"
        class="group relative flex h-9 w-full items-center rounded-md text-[13px] transition"
        :class="[
          sidebarCollapsed ? 'justify-center px-2' : 'gap-2 px-2',
          activeKey === item.key
            ? 'bg-accent-soft !text-accent-text font-medium ring-1 ring-inset ring-accent/35 shadow-sm shadow-accent/10'
            : '!text-secondary hover:bg-surface-hover hover:!text-primary',
        ]"
        :title="sidebarCollapsed ? item.label : undefined"
        :aria-label="item.label"
      >
        <component
          :is="item.icon"
          :size="17"
          class="shrink-0"
          :class="activeKey === item.key ? 'text-accent' : 'text-current'"
        />
        <span v-if="!sidebarCollapsed" class="min-w-0 flex-1 text-left leading-none">{{ item.label }}</span>
      </RouterLink>
    </nav>

    <div v-if="!sidebarCollapsed" class="mt-3 min-h-0 flex-1 overflow-hidden">
      <div class="grid h-full min-h-0 grid-rows-[minmax(0,1.35fr)_minmax(0,0.75fr)_minmax(0,0.75fr)_minmax(0,0.75fr)] gap-2.5 overflow-hidden pr-1">
        <section class="flex min-h-0 flex-col overflow-hidden rounded-lg border border-accent bg-surface p-2 shadow-card">
          <div class="docmind-section-label flex items-center gap-1.5 border-b border-default px-1 pb-2">
            <FolderOpen :size="13" />
            {{ t("sidebar.indexDirs") }}
          </div>
          <div class="min-h-0 flex-1 overflow-y-auto pt-2">
            <div v-if="quickDirs.length === 0" class="rounded-md border border-dashed border-default bg-surface px-3 py-3 text-[11px] text-muted">
              {{ t("page.appSearch.section.noDirs") }}
            </div>
            <DocMindIndexTree
              v-else
              :rows="visibleQuickDirRows"
              :selected-path="selectedIndexDirPath"
              :selectable="true"
              :path-tooltip="true"
              :virtual-label="t('common.virtualDir')"
              :expand-title="t('sidebar.expand')"
              :collapse-title="t('sidebar.collapse')"
              :node-padding-base="0"
              :node-padding-step="12"
              density="compact"
              @node-select="openIndexDir"
              @toggle="setQuickDirExpanded"
            />
          </div>
        </section>

        <section class="flex min-h-0 flex-col overflow-hidden rounded-lg border border-default bg-panel/35 p-2">
          <div class="docmind-section-label flex items-center gap-1.5 border-b border-default px-1 pb-2">
            <History :size="13" />
            {{ t("page.appSearch.section.recentSearch") }}
          </div>
          <div class="min-h-0 flex-1 overflow-y-auto pt-2">
            <div v-if="searchHistory.length === 0" class="rounded-md border border-dashed border-default bg-surface px-3 py-3 text-[11px] text-muted">
              {{ t("page.appSearch.section.noHistory") }}
            </div>
            <div v-else class="space-y-1">
              <div
                v-for="item in searchHistory"
                :key="item.query"
                class="group flex items-center gap-1"
              >
                <button
                  class="min-w-0 flex-1 rounded-md px-1.5 py-1 text-left text-[10px] leading-4 text-secondary transition hover:bg-panel hover:text-primary"
                  :title="item.query"
                  @click="openSearchQuery(item.query)"
                >
                  <div class="truncate text-[11px] font-medium leading-4 text-primary">{{ item.query }}</div>
                  <div class="mt-0.5 truncate text-[10px] leading-4 text-muted">{{ item.last_hit_at }}</div>
                </button>
                <button
                  class="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-muted opacity-0 transition hover:bg-surface-hover hover:text-danger group-hover:opacity-100"
                  :title="t('page.appSearch.section.remove')"
                  :disabled="panelActionTarget === `history:${item.query}`"
                  @click.stop="removeSearchHistory(item.query)"
                >
                  <Trash2 :size="13" />
                </button>
              </div>
            </div>
          </div>
        </section>

        <section class="flex min-h-0 flex-col overflow-hidden rounded-lg border border-default bg-panel/35 p-2">
          <div class="docmind-section-label flex items-center gap-1.5 border-b border-default px-1 pb-2">
            <FileText :size="13" />
            {{ t("page.appSearch.section.recentOpen") }}
          </div>
          <div class="min-h-0 flex-1 overflow-y-auto pt-2">
            <div v-if="recentDocuments.length === 0" class="rounded-md border border-dashed border-default bg-surface px-3 py-3 text-[11px] text-muted">
              {{ t("page.appSearch.section.noRecent") }}
            </div>
            <div v-else class="space-y-1">
              <div
                v-for="item in recentDocuments"
                :key="item.path"
                class="group flex items-start gap-1"
              >
                <button
                  class="min-w-0 flex-1 rounded-md px-1.5 py-1 text-left text-[10px] leading-4 text-secondary transition hover:bg-panel hover:text-primary"
                  :title="t('page.appSearch.section.recentOpenTip', { title: item.title, path: item.path })"
                  @click="openRecentDocument(item.path)"
                >
                  <div class="truncate text-[11px] font-medium leading-4 text-primary">{{ item.title }}</div>
                  <div class="mt-0.5 truncate text-[10px] leading-4 text-muted">{{ item.path }}</div>
                </button>
                <button
                  class="mt-0.5 inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-muted opacity-0 transition hover:bg-surface-hover hover:text-danger group-hover:opacity-100"
                  :title="t('page.appSearch.section.remove')"
                  :disabled="panelActionTarget === `recent:${item.path}`"
                  @click.stop="removeRecentDocument(item.path)"
                >
                  <Trash2 :size="13" />
                </button>
              </div>
            </div>
          </div>
        </section>

        <section class="flex min-h-0 flex-col overflow-hidden rounded-lg border border-default bg-panel/35 p-2">
          <div class="docmind-section-label flex items-center gap-1.5 border-b border-default px-1 pb-2">
            <Star :size="13" />
            {{ t("page.appSearch.section.favorites") }}
          </div>
          <div class="min-h-0 flex-1 overflow-y-auto pt-2">
            <div v-if="favoriteResults.length === 0" class="rounded-md border border-dashed border-default bg-surface px-3 py-3 text-[11px] text-muted">
              {{ t("page.appSearch.section.noFavorites") }}
            </div>
            <div v-else class="space-y-1">
              <div
                v-for="item in favoriteResults"
                :key="item.target"
                class="group flex items-start gap-1"
              >
                <button
                  class="min-w-0 flex-1 rounded-md px-1.5 py-1 text-left text-[10px] leading-4 text-secondary transition hover:bg-panel hover:text-primary"
                  :title="t('page.appSearch.section.favoriteTip', { title: item.title, path: item.path })"
                  @click="openFavoriteDocument(item.path)"
                >
                  <div class="truncate text-[11px] font-medium leading-4 text-primary">{{ item.title }}</div>
                  <div class="mt-0.5 truncate text-[10px] leading-4 text-muted">{{ item.path }}</div>
                </button>
                <button
                  class="mt-0.5 inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-muted opacity-0 transition hover:bg-surface-hover hover:text-danger group-hover:opacity-100"
                  :title="t('page.appSearch.section.remove')"
                  :disabled="panelActionTarget === `favorite:${item.target}`"
                  @click.stop="removeFavorite(item.target)"
                >
                  <Trash2 :size="13" />
                </button>
              </div>
            </div>
          </div>
        </section>

      </div>
    </div>

  </aside>
</template>
