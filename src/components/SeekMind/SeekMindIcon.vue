<script setup lang="ts">
/**
 * @author MorningSun
 * @CreatedDate 2026/06/11
 * @Description SeekMind 统一中图标组件，直接渲染美工交付的 SVG 资源并跟随主题色继承。
 */
import { computed } from "vue";

defineOptions({
  name: "SeekMindIcon",
});

const iconModules = import.meta.glob<string>("../../../ui-prototype/seekmind_icons/svg/*.svg", {
  eager: true,
  query: "?raw",
  import: "default",
});

const iconMap = Object.fromEntries(
  Object.entries(iconModules).map(([path, svg]) => {
    const match = path.match(/\/(icon-[^/]+)\.svg$/);
    return [match?.[1] ?? path, svg];
  }),
) as Record<string, string>;

const props = withDefaults(defineProps<{
  icon: string;
  size?: number;
}>(), {
  size: 20,
});

const svg = computed(() => iconMap[props.icon] ?? "");

const style = computed(() => ({
  width: `${props.size}px`,
  height: `${props.size}px`,
}));
</script>

<template>
  <span class="seekmind-icon-root" :style="style" aria-hidden="true" v-html="svg" />
</template>

<style scoped>
.seekmind-icon-root {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: inherit;
  flex-shrink: 0;
}

.seekmind-icon-root :deep(svg) {
  width: 100%;
  height: 100%;
  display: block;
}
</style>
