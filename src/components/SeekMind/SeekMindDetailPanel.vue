<!--
  @author MorningSun
  @CreatedDate 2026/06/11
  @Description 统一右侧详情面板的标题区与滚动内容容器，保证各页面阅读层级一致。
-->
<script setup lang="ts">
defineOptions({
  name: "SeekMindDetailPanel",
});
</script>

<template>
  <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
    <div class="shrink-0 border-b border-default bg-panel px-5 py-3">
      <slot name="header" />
    </div>
    <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
      <!-- 修复：右栏内容统一拆成固定阅读段落，避免不同页面在同一视觉层级里互相抢注意力。 -->
      <div class="space-y-4">
        <slot />
      </div>
    </div>
  </div>
</template>
