<script setup lang="ts">
import { computed } from "vue";
import { Database, FolderOpen, Search, Settings } from "lucide-vue-next";
import { useRoute } from "vue-router";

const route = useRoute();

const items = [
  { key: "search", label: "搜索", icon: Search, to: "/" },
  { key: "library", label: "文档目录", icon: FolderOpen, to: "/library" },
  { key: "status", label: "索引状态", icon: Database, to: "/status" },
  { key: "settings", label: "设置", icon: Settings, to: "/settings" },
];

const activeKey = computed(() => {
  const path = route.path;
  if (path.startsWith("/library")) return "library";
  if (path.startsWith("/status")) return "status";
  if (path.startsWith("/settings")) return "settings";
  return "search";
});
</script>

<template>
  <aside class="flex h-full w-56 flex-col border-r border-slate-200 bg-white/80 p-4 backdrop-blur-xl">
    <div class="mb-8 flex items-center gap-3 px-2">
      <div class="flex h-9 w-9 items-center justify-center rounded-2xl bg-slate-900 text-white shadow-sm">dM</div>
      <div>
        <div class="text-sm font-semibold text-slate-900">docMind</div>
        <div class="text-xs text-slate-500">Local document search</div>
      </div>
    </div>

    <nav class="space-y-1">
      <RouterLink
        v-for="item in items"
        :key="item.key"
        :to="item.to"
        class="flex w-full items-center gap-3 rounded-2xl px-3 py-2.5 text-sm transition"
        :class="activeKey === item.key ? 'bg-slate-900 text-white shadow-sm' : 'text-slate-600 hover:bg-slate-100'"
      >
        <component :is="item.icon" :size="17" />
        {{ item.label }}
      </RouterLink>
    </nav>

    <div class="mt-auto rounded-2xl bg-slate-50 p-3 text-xs text-slate-500">
      <div class="mb-1 font-medium text-slate-700">本地优先</div>
      文档只在当前 Mac 上解析和索引，不上传云端。
    </div>
  </aside>
</template>
