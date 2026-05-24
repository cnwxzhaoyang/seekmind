<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { Database, FolderOpen, Layers3, Search, Settings } from "lucide-vue-next";
import { useRoute } from "vue-router";

const { t } = useI18n();
const route = useRoute();

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
</script>

<template>
  <aside class="flex h-full w-[200px] flex-col border-r border-slate-200 bg-slate-100 p-2">
    <div class="mb-4 flex items-center gap-2 px-3 py-2">
      <div class="flex h-6 w-6 items-center justify-center rounded bg-indigo-600 text-xs font-bold text-white shadow-sm">dm</div>
      <div>
        <div class="text-sm font-semibold text-slate-900">docMind</div>
      </div>
    </div>

    <nav class="space-y-1">
      <RouterLink
        v-for="item in items"
        :key="item.key"
        :to="item.to"
        class="flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm transition"
        :class="
          activeKey === item.key
            ? 'bg-slate-200 !text-indigo-600 font-medium'
            : '!text-slate-600 hover:bg-slate-200 hover:!text-slate-900'
        "
      >
        <component :is="item.icon" :size="17" />
        {{ item.label }}
      </RouterLink>
    </nav>

    <div class="mt-auto rounded-lg border border-indigo-100 bg-indigo-50 p-3 text-[10px] leading-snug text-indigo-400">
      <div class="mb-1 font-semibold text-indigo-600">{{ t("sidebar.localFirst") }}</div>
      {{ t("sidebar.localFirstDesc") }}
    </div>
  </aside>
</template>
