<template>
  <div class="h-screen w-full overflow-hidden bg-page text-primary">
    <div class="flex h-full">
      <DocMindSidebar />
      <div class="flex min-w-0 flex-1 flex-col bg-panel">
        <RouterView class="flex-1 min-h-0" />
        <DocMindLogPanel />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import DocMindSidebar from "./DocMindSidebar.vue";
import DocMindLogPanel from "./DocMindLogPanel.vue";
import { useTheme } from "../../composables/useTheme";

useTheme();
</script>
