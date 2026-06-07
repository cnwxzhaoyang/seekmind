<script setup lang="ts">
/**
 * @author MorningSun
 * @CreatedDate 2026/06/07
 * @Description SeekMind 全局桌面壳层，承载侧边栏、工作区与底部日志面板。
 */
import SeekMindSidebar from "./SeekMindSidebar.vue";
import SeekMindLogPanel from "./SeekMindLogPanel.vue";
import { useTheme } from "../../composables/useTheme";

useTheme();
</script>

<template>
  <div class="flex h-screen w-full flex-col overflow-hidden bg-page text-primary">
    <div class="flex min-h-0 flex-1 overflow-hidden">
      <SeekMindSidebar />
      <div class="flex min-w-0 flex-1 flex-col overflow-hidden bg-[linear-gradient(180deg,rgba(255,255,255,0.78)_0%,rgba(245,245,247,0.92)_100%)]">
        <div class="flex-1 min-h-0 overflow-hidden">
          <RouterView v-slot="{ Component }">
            <KeepAlive include="AppSearchPage,AppQaPage,AppCollectionsPage,ChunksPage,StatusPage,SettingsPage,LibraryPage">
              <component :is="Component" class="h-full min-h-0" />
            </KeepAlive>
          </RouterView>
        </div>
        <SeekMindLogPanel />
      </div>
    </div>
  </div>
</template>
