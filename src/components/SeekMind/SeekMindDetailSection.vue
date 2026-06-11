<!--
  @author MorningSun
  @CreatedDate 2026/06/11
  @Description 统一右侧详情的分段标题样式，承载概览、原文、上下文等阅读区块。
-->
<script setup lang="ts">
defineOptions({
  name: "SeekMindDetailSection",
});

interface Props {
  title?: string;
  subtitle?: string;
}

defineProps<Props>();
</script>

<template>
  <section class="seekmind-detail-section min-w-0">
    <div v-if="title || subtitle" class="mb-2 min-w-0">
      <div v-if="title" class="SeekMind-section-label">{{ title }}</div>
      <div v-if="subtitle" class="SeekMind-section-subtitle mt-1 break-all">{{ subtitle }}</div>
    </div>
    <div class="space-y-2">
      <slot />
    </div>
  </section>
</template>
