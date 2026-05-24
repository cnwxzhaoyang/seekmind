<template>
  <div class="h-screen w-full overflow-hidden bg-slate-100 text-slate-900">
    <div class="flex h-full">
      <DocMindSidebar />
      <div class="relative min-w-0 flex-1 bg-slate-50">
        <RouterView />
        <DocMindLogPanel />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import DocMindSidebar from "./DocMindSidebar.vue";
import DocMindLogPanel from "./DocMindLogPanel.vue";
</script>
