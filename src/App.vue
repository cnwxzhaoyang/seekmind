<script setup lang="ts">
/**
 * @author MorningSun
 * @CreatedDate 2026/06/02
 * @Description 应用入口，注入全局 SVG 图标 sprite 并挂载页面壳。
 */
import { onMounted } from "vue";
import DocMindPageShell from "./components/docmind/DocMindPageShell.vue";
import iconsSprite from "./assets/icons.svg?raw";

onMounted(() => {
  // 修复：SVG 图标依赖 symbol sprite，必须随应用启动注入到 DOM 中。
  console.info("[DocMind] SVG icon sprite injected");
});
</script>

<template>
  <div class="svg-sprite" aria-hidden="true" v-html="iconsSprite"></div>
  <DocMindPageShell />
</template>

<style scoped>
.svg-sprite {
  display: none;
}
</style>
