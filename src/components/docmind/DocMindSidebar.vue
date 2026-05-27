<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { ChevronLeft, ChevronRight, Database, Layers3, Search, Settings } from "lucide-vue-next";
import { useRoute } from "vue-router";
import { useSidebarState } from "../../composables/useSidebarState";

const { t } = useI18n();
const route = useRoute();
const { sidebarCollapsed, toggleSidebar } = useSidebarState();

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

</script>

<template>
  <aside
    class="flex h-full flex-col overflow-hidden border-r border-default bg-sidebar p-2 transition-[width] duration-200"
    :class="sidebarCollapsed ? 'w-[68px]' : 'w-[200px]'"
  >
    <div class="mb-4 flex items-center justify-between gap-2 px-2 py-2">
      <div class="flex h-6 w-6 items-center justify-center rounded bg-accent text-xs font-bold text-white shadow-card">dm</div>
      <div v-if="!sidebarCollapsed" class="min-w-0 flex-1">
        <div class="text-sm font-semibold text-primary">docMind</div>
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

    <nav class="space-y-1">
      <RouterLink
        v-for="item in items"
        :key="item.key"
        :to="item.to"
        class="group flex w-full items-center rounded-md py-2 text-sm transition"
        :class="[
          sidebarCollapsed ? 'justify-center px-2' : 'gap-3 px-3',
          activeKey === item.key
            ? 'bg-surface-active !text-accent font-medium'
            : '!text-secondary hover:bg-surface-hover hover:!text-primary',
        ]"
        :title="sidebarCollapsed ? item.label : undefined"
        :aria-label="item.label"
      >
        <component :is="item.icon" :size="17" />
        <span v-if="!sidebarCollapsed">{{ item.label }}</span>
      </RouterLink>
    </nav>

    <div v-if="!sidebarCollapsed" class="mt-auto rounded-lg border border-accent-soft bg-accent-soft p-3 text-[10px] leading-snug text-muted">
      <div class="mb-1 font-semibold text-accent-text">{{ t("sidebar.localFirst") }}</div>
      {{ t("sidebar.localFirstDesc") }}
    </div>
    <div
      v-else
      class="mt-auto flex items-center justify-center rounded-lg border border-accent-soft bg-accent-soft p-2 text-accent-text"
      :title="t('sidebar.localFirst')"
    >
      <div class="flex h-6 w-6 items-center justify-center rounded bg-accent text-[10px] font-bold text-white shadow-card">dm</div>
    </div>
  </aside>
</template>
