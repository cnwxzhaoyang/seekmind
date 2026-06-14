<script setup lang="ts">
/**
 * @author MorningSun
 * @CreatedDate 2026/06/12
 * @Description SeekMind 文件类型图标组件，按统一样式展示 Markdown、PDF、Office、图片等文档类型。
 */
import { computed } from "vue";
import { File, FileCode2, FileImage, FileSpreadsheet, FileText, Presentation } from "lucide-vue-next";

interface Props {
  ext?: string;
}

type FileIconComponent = typeof File;

type FileIconVisual = {
  component: FileIconComponent;
  toneClass: string;
  accentClass: string;
  badgeText?: string;
};

const props = defineProps<Props>();

const normalizedExt = computed(() => props.ext?.trim().toLowerCase().replace(/^\./, "") || "");

const iconVisual = computed<FileIconVisual>(() => {
  switch (normalizedExt.value) {
    case "md":
    case "markdown":
      return {
        component: FileCode2,
        toneClass: "seekmind-file-icon--code",
        accentClass: "seekmind-file-badge--code",
        badgeText: "MD",
      };
    case "txt":
    case "json":
    case "yaml":
    case "yml":
    case "xml":
    case "html":
    case "js":
    case "ts":
    case "rs":
    case "py":
      return {
        component: FileCode2,
        toneClass: "seekmind-file-icon--code",
        accentClass: "seekmind-file-badge--code",
      };
    case "pdf":
      return {
        component: FileText,
        toneClass: "seekmind-file-icon--pdf",
        accentClass: "seekmind-file-badge--pdf",
        badgeText: "PDF",
      };
    case "doc":
    case "docx":
    case "rtf":
      return {
        component: FileText,
        toneClass: "seekmind-file-icon--word",
        accentClass: "seekmind-file-badge--word",
        badgeText: "DOC",
      };
    case "xls":
    case "xlsx":
    case "csv":
      return {
        component: FileSpreadsheet,
        toneClass: "seekmind-file-icon--excel",
        accentClass: "seekmind-file-badge--excel",
        badgeText: "XLS",
      };
    case "ppt":
    case "pptx":
      return {
        component: Presentation,
        toneClass: "seekmind-file-icon--ppt",
        accentClass: "seekmind-file-badge--ppt",
        badgeText: "PPT",
      };
    case "png":
    case "jpg":
    case "jpeg":
    case "webp":
    case "bmp":
    case "gif":
    case "tif":
    case "tiff":
    case "heic":
      return {
        component: FileImage,
        toneClass: "seekmind-file-icon--image",
        accentClass: "seekmind-file-badge--image",
        badgeText: "IMG",
      };
    default:
      // 修复说明：Windows WebView 下 raw SVG 文件图标偶现空白，这里统一改为 lucide 组件渲染，并补充更接近 macOS 的色彩层次。
      return {
        component: File,
        toneClass: "seekmind-file-icon--default",
        accentClass: "seekmind-file-badge--default",
      };
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

const badgeLabel = computed(() => iconVisual.value.badgeText ?? label.value.slice(0, 3));
</script>

<template>
  <div class="SeekMind-file-icon" :class="iconVisual.toneClass">
    <component :is="iconVisual.component" class="seekmind-file-icon__glyph" :stroke-width="1.9" />
    <span class="seekmind-file-icon__badge" :class="iconVisual.accentClass">{{ badgeLabel }}</span>
    <span class="sr-only">{{ label }}</span>
  </div>
</template>

<style scoped>
.SeekMind-file-icon {
  position: relative;
  display: inline-flex;
  height: 34px;
  width: 34px;
  flex-shrink: 0;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  border-radius: 11px;
  border: 1px solid var(--file-icon-border, var(--color-border));
  background:
    linear-gradient(180deg, color-mix(in srgb, var(--file-icon-bg, var(--color-badge-bg)) 92%, #ffffff 8%), var(--file-icon-bg, var(--color-badge-bg)));
  box-shadow:
    inset 0 1px 0 color-mix(in srgb, #ffffff 55%, transparent),
    0 1px 2px color-mix(in srgb, var(--file-icon-fg, var(--color-text-dim)) 10%, transparent);
  color: var(--file-icon-fg, var(--color-text-dim));
}

.seekmind-file-icon__glyph {
  height: 18px;
  width: 18px;
}

.seekmind-file-icon__badge {
  position: absolute;
  right: 3px;
  bottom: 3px;
  border-radius: 999px;
  padding: 0 4px;
  font-size: 8px;
  font-weight: 700;
  letter-spacing: 0.04em;
  line-height: 12px;
  background: var(--file-badge-bg, var(--color-surface));
  color: var(--file-badge-fg, var(--file-icon-fg, var(--color-text-dim)));
  box-shadow: 0 1px 2px color-mix(in srgb, var(--file-badge-fg, var(--color-text-dim)) 12%, transparent);
}

.seekmind-file-icon--default {
  --file-icon-fg: var(--color-text-dim);
  --file-icon-bg: color-mix(in srgb, var(--color-badge-bg) 88%, var(--color-surface) 12%);
  --file-icon-border: color-mix(in srgb, var(--color-border) 78%, transparent);
  --file-badge-bg: color-mix(in srgb, var(--color-surface) 88%, transparent);
  --file-badge-fg: var(--color-text-dim);
}

.seekmind-file-icon--code {
  --file-icon-fg: #0f6cbd;
  --file-icon-bg: color-mix(in srgb, var(--color-accent-soft) 82%, #ffffff 18%);
  --file-icon-border: color-mix(in srgb, #0f6cbd 18%, var(--color-border) 82%);
  --file-badge-bg: color-mix(in srgb, #0f6cbd 16%, #ffffff 84%);
  --file-badge-fg: #0f6cbd;
}

.seekmind-file-icon--pdf {
  --file-icon-fg: #c0392b;
  --file-icon-bg: color-mix(in srgb, var(--color-rose-soft) 84%, #ffffff 16%);
  --file-icon-border: color-mix(in srgb, #c0392b 20%, var(--color-border) 80%);
  --file-badge-bg: color-mix(in srgb, #c0392b 14%, #ffffff 86%);
  --file-badge-fg: #c0392b;
}

.seekmind-file-icon--word {
  --file-icon-fg: #2563eb;
  --file-icon-bg: color-mix(in srgb, var(--color-indigo-soft) 84%, #ffffff 16%);
  --file-icon-border: color-mix(in srgb, #2563eb 18%, var(--color-border) 82%);
  --file-badge-bg: color-mix(in srgb, #2563eb 14%, #ffffff 86%);
  --file-badge-fg: #2563eb;
}

.seekmind-file-icon--excel {
  --file-icon-fg: #1f8b4c;
  --file-icon-bg: color-mix(in srgb, var(--color-emerald-soft) 86%, #ffffff 14%);
  --file-icon-border: color-mix(in srgb, #1f8b4c 20%, var(--color-border) 80%);
  --file-badge-bg: color-mix(in srgb, #1f8b4c 14%, #ffffff 86%);
  --file-badge-fg: #1f8b4c;
}

.seekmind-file-icon--ppt {
  --file-icon-fg: #d97706;
  --file-icon-bg: color-mix(in srgb, var(--color-amber-soft) 84%, #ffffff 16%);
  --file-icon-border: color-mix(in srgb, #d97706 18%, var(--color-border) 82%);
  --file-badge-bg: color-mix(in srgb, #d97706 14%, #ffffff 86%);
  --file-badge-fg: #d97706;
}

.seekmind-file-icon--image {
  --file-icon-fg: #7c3aed;
  --file-icon-bg: color-mix(in srgb, var(--color-accent-soft) 60%, var(--color-rose-soft) 22%, #ffffff 18%);
  --file-icon-border: color-mix(in srgb, #7c3aed 18%, var(--color-border) 82%);
  --file-badge-bg: color-mix(in srgb, #7c3aed 12%, #ffffff 88%);
  --file-badge-fg: #7c3aed;
}

.dark .SeekMind-file-icon {
  background:
    linear-gradient(180deg, color-mix(in srgb, var(--file-icon-bg, var(--color-badge-bg)) 92%, #ffffff 3%), var(--file-icon-bg, var(--color-badge-bg)));
  box-shadow:
    inset 0 1px 0 color-mix(in srgb, #ffffff 10%, transparent),
    0 1px 2px color-mix(in srgb, #000000 30%, transparent);
}

.dark .seekmind-file-icon--code {
  --file-icon-fg: #79c0ff;
  --file-icon-bg: color-mix(in srgb, #0f2f55 58%, var(--color-surface) 42%);
  --file-icon-border: color-mix(in srgb, #79c0ff 14%, var(--color-border) 86%);
  --file-badge-bg: color-mix(in srgb, #79c0ff 10%, var(--color-surface) 90%);
  --file-badge-fg: #9dd6ff;
}

.dark .seekmind-file-icon--pdf {
  --file-icon-fg: #ff938a;
  --file-icon-bg: color-mix(in srgb, #4b1617 60%, var(--color-surface) 40%);
  --file-icon-border: color-mix(in srgb, #ff938a 14%, var(--color-border) 86%);
  --file-badge-bg: color-mix(in srgb, #ff938a 10%, var(--color-surface) 90%);
  --file-badge-fg: #ffb2ab;
}

.dark .seekmind-file-icon--word {
  --file-icon-fg: #8ab4ff;
  --file-icon-bg: color-mix(in srgb, #14294f 60%, var(--color-surface) 40%);
  --file-icon-border: color-mix(in srgb, #8ab4ff 14%, var(--color-border) 86%);
  --file-badge-bg: color-mix(in srgb, #8ab4ff 10%, var(--color-surface) 90%);
  --file-badge-fg: #b3ceff;
}

.dark .seekmind-file-icon--excel {
  --file-icon-fg: #6ee7a4;
  --file-icon-bg: color-mix(in srgb, #10351f 62%, var(--color-surface) 38%);
  --file-icon-border: color-mix(in srgb, #6ee7a4 14%, var(--color-border) 86%);
  --file-badge-bg: color-mix(in srgb, #6ee7a4 10%, var(--color-surface) 90%);
  --file-badge-fg: #9ef0c2;
}

.dark .seekmind-file-icon--ppt {
  --file-icon-fg: #ffb86b;
  --file-icon-bg: color-mix(in srgb, #3d240f 60%, var(--color-surface) 40%);
  --file-icon-border: color-mix(in srgb, #ffb86b 14%, var(--color-border) 86%);
  --file-badge-bg: color-mix(in srgb, #ffb86b 10%, var(--color-surface) 90%);
  --file-badge-fg: #ffd1a0;
}

.dark .seekmind-file-icon--image {
  --file-icon-fg: #c4a1ff;
  --file-icon-bg: color-mix(in srgb, #2d1a4f 58%, var(--color-surface) 42%);
  --file-icon-border: color-mix(in srgb, #c4a1ff 14%, var(--color-border) 86%);
  --file-badge-bg: color-mix(in srgb, #c4a1ff 10%, var(--color-surface) 90%);
  --file-badge-fg: #dcc3ff;
}
</style>
