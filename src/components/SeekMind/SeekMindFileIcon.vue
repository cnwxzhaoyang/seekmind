<script setup lang="ts">
/**
 * @author MorningSun
 * @CreatedDate 2026/06/12
 * @Description SeekMind 文件类型标识，按统一图标系统展示文档、图片、Markdown、PDF 等类型。
 */
import { computed } from "vue";
import SeekMindIcon from "./SeekMindIcon.vue";

interface Props {
  ext?: string;
}

const props = defineProps<Props>();

const normalizedExt = computed(() => props.ext?.trim().toLowerCase().replace(/^\./, "") || "");

const iconName = computed(() => {
  switch (normalizedExt.value) {
    case "md":
    case "markdown":
      return "icon-markdown";
    case "pdf":
      return "icon-pdf";
    case "png":
    case "jpg":
    case "jpeg":
    case "webp":
    case "bmp":
    case "gif":
    case "tif":
    case "tiff":
    case "heic":
      return "icon-image";
    default:
      return "icon-file";
  }
});

const label = computed(() => {
  if (!normalizedExt.value) {
    return "DOC";
  }
  if (normalizedExt.value === "markdown") {
    return "MD";
  }
  return normalizedExt.value.toUpperCase();
});
</script>

<template>
  <div class="SeekMind-file-icon flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-badge text-dim">
    <SeekMindIcon :icon="iconName" :size="17" />
    <span class="sr-only">{{ label }}</span>
  </div>
</template>
