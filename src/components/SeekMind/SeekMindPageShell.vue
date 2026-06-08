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
      <!-- 深色主题下工作区背景必须跟随主题 token，避免遗留浅色渐变。 -->
      <div class="seekmind-workspace-shell flex min-w-0 flex-1 flex-col overflow-hidden">
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
