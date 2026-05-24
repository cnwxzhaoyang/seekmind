<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ChevronLeft, ChevronRight, Database, FolderOpen, Layers3, Search, Settings } from "lucide-vue-next";
import { useRoute } from "vue-router";

const { t } = useI18n();
const route = useRoute();
const sidebarCollapsed = ref(false);
const storageKey = "docmind.sidebar.collapsed";

const items = computed(() => [
  { key: "search", label: t("sidebar.search"), icon: Search, to: "/" },
  { key: "chunks", label: t("sidebar.chunks"), icon: Layers3, to: "/chunks" },
  { key: "library", label: t("sidebar.library"), icon: FolderOpen, to: "/library" },
  { key: "status", label: t("sidebar.status"), icon: Database, to: "/status" },
  { key: "settings", label: t("sidebar.settings"), icon: Settings, to: "/settings" },
]);

const activeKey = computed(() => {
  const path = route.path;
  if (path.startsWith("/chunks")) return "chunks";
  if (path.startsWith("/library")) return "library";
  if (path.startsWith("/status")) return "status";
  if (path.startsWith("/settings")) return "settings";
  return "search";
});

const toggleSidebar = () => {
  sidebarCollapsed.value = !sidebarCollapsed.value;
  window.localStorage.setItem(storageKey, sidebarCollapsed.value ? "1" : "0");
};

onMounted(() => {
  sidebarCollapsed.value = window.localStorage.getItem(storageKey) === "1";
});
</script>

<template>
  <aside
    class="flex h-full flex-col overflow-hidden border-r border-slate-200 bg-slate-100 p-2 transition-[width] duration-200"
    :class="sidebarCollapsed ? 'w-[68px]' : 'w-[200px]'"
  >
    <div class="mb-4 flex items-center justify-between gap-2 px-2 py-2">
      <div class="flex h-6 w-6 items-center justify-center rounded bg-indigo-600 text-xs font-bold text-white shadow-sm">dm</div>
      <div v-if="!sidebarCollapsed" class="min-w-0 flex-1">
        <div class="text-sm font-semibold text-slate-900">docMind</div>
      </div>
      <button
        class="inline-flex h-6 w-6 items-center justify-center rounded-md text-slate-500 transition hover:bg-slate-200 hover:text-slate-900"
        :title="sidebarCollapsed ? t('sidebar.expand') : t('sidebar.collapse')"
        :aria-label="sidebarCollapsed ? t('sidebar.expand') : t('sidebar.collapse')"
        @click="toggleSidebar"
      >
        <ChevronRight v-if="sidebarCollapsed" :size="15" />
        <ChevronLeft v-else :size="15" />
      </button>
    </div>

    <nav class="space-y-1">
      <RouterLink
        v-for="item in items"
        :key="item.key"
        :to="item.to"
        class="group flex w-full items-center rounded-md py-2 text-sm transition"
        :class="[
          sidebarCollapsed ? 'justify-center px-2' : 'gap-3 px-3',
          activeKey === item.key
            ? 'bg-slate-200 !text-indigo-600 font-medium'
            : '!text-slate-600 hover:bg-slate-200 hover:!text-slate-900',
        ]"
        :title="sidebarCollapsed ? item.label : undefined"
        :aria-label="item.label"
      >
        <component :is="item.icon" :size="17" />
        <span v-if="!sidebarCollapsed">{{ item.label }}</span>
      </RouterLink>
    </nav>

    <div v-if="!sidebarCollapsed" class="mt-auto rounded-lg border border-indigo-100 bg-indigo-50 p-3 text-[10px] leading-snug text-indigo-400">
      <div class="mb-1 font-semibold text-indigo-600">{{ t("sidebar.localFirst") }}</div>
      {{ t("sidebar.localFirstDesc") }}
    </div>
    <div
      v-else
      class="mt-auto flex items-center justify-center rounded-lg border border-indigo-100 bg-indigo-50 p-2 text-indigo-600"
      :title="t('sidebar.localFirst')"
    >
      <div class="flex h-6 w-6 items-center justify-center rounded bg-indigo-600 text-[10px] font-bold text-white shadow-sm">dm</div>
    </div>
  </aside>
</template>
