<script setup lang="ts">
/**
 * @author MorningSun
 * @CreatedDate 2026/06/07
 * @Description SeekMind 全局桌面壳层，承载侧边栏、工作区与底部日志面板。
 */
import { computed } from "vue";
import { useRoute } from "vue-router";
import SeekMindFirstLaunchGuide from "./SeekMindFirstLaunchGuide.vue";
import SeekMindSidebar from "./SeekMindSidebar.vue";
import SeekMindLogPanel from "./SeekMindLogPanel.vue";
import { useTheme } from "../../composables/useTheme";

useTheme();
const route = useRoute();
const isSettingsRoute = computed(() => route.path.startsWith("/settings"));
</script>

<template>
  <div class="flex h-screen w-full flex-col overflow-hidden bg-page text-primary">
    <div class="flex min-h-0 flex-1 overflow-hidden">
      <SeekMindSidebar v-if="!isSettingsRoute" />
      <!-- 深色主题下工作区背景必须跟随主题 token，避免遗留浅色渐变。 -->
      <div class="seekmind-workspace-shell flex min-w-0 flex-1 flex-col overflow-hidden">
        <SeekMindFirstLaunchGuide />
        <div class="flex-1 min-h-0 overflow-hidden">
          <RouterView v-slot="{ Component }">
            <KeepAlive include="AppSearchPage,AppQaPage,AppCollectionsPage,ChunksPage,StatusPage,SettingsPage,LibraryPage">
              <component :is="Component" class="h-full min-h-0" />
            </KeepAlive>
          </RouterView>
        </div>
        <!-- 修复：设置页采用独立设置工作区，隐藏底部日志栏以贴近桌面设置面板的专注布局。 -->
        <SeekMindLogPanel v-if="!isSettingsRoute" />
      </div>
    </div>
  </div>
</template>
