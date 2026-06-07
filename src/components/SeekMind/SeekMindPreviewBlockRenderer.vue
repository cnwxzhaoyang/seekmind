<!--
  @author MorningSun
  @CreatedDate 2026/06/03
  @Description 结构化预览块渲染组件，支持段落、表格、图片等内容。
-->
<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import type { PreviewBlockView } from "../../types/SeekMind";
import SeekMindMarkdownRenderer from "./SeekMindMarkdownRenderer.vue";

const props = defineProps<{
  block: PreviewBlockView;
}>();

const headingSize = computed(() => {
  const level = props.block.level || 1;
  if (level <= 1) return "text-base";
  if (level <= 2) return "text-sm";
  return "text-sm";
});

const escapeHtml = (value: string) =>
  value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/\"/g, "&quot;")
    .replace(/'/g, "&#39;");

const normalizeCell = (value: string) => value.replace(/\s+/g, " ").trim();

const splitTableCells = (line: string) =>
  line
    .trim()
    .replace(/^\|/, "")
    .replace(/\|$/, "")
    .split(/\s*\|\s*/)
    .map((cell) => normalizeCell(cell));

const isMarkdownSeparator = (line: string) => /^\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)+\|?$/.test(line.trim());

const tableHtml = computed(() => {
  if (props.block.block_type !== "table") return "";

  const source = props.block.html?.trim() || props.block.markdown?.trim() || props.block.text.trim();
  if (!source) return "";

  if (props.block.html?.trim()) {
    return props.block.html;
  }

  const lines = source
    .replace(/\r\n/g, "\n")
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);

  if (lines.length === 0) return "";

  const rows = lines.filter((line) => !isMarkdownSeparator(line)).map(splitTableCells).filter((row) => row.length > 0);
  if (rows.length === 0) return "";

  const hasExplicitMarkdownHeader = Boolean(props.block.markdown && lines.some(isMarkdownSeparator));
  const header = hasExplicitMarkdownHeader && rows.length > 0 ? rows[0] : null;
  const bodyRows = hasExplicitMarkdownHeader && rows.length > 1 ? rows.slice(1) : rows;
  const maxCols = Math.max(...rows.map((row) => row.length), 1);

  const normalizeRow = (row: string[]) => {
    const filled = [...row];
    while (filled.length < maxCols) filled.push("");
    return filled;
  };

  const renderCells = (cells: string[], tag: "th" | "td") =>
    cells.map((cell) => `<${tag} style="border:1px solid var(--color-border);padding:0.5rem 0.75rem;vertical-align:top">${escapeHtml(cell)}</${tag}>`).join("");

  const thead = header
    ? `<thead style="background:var(--color-panel-bg);color:var(--color-text-secondary)"><tr>${renderCells(normalizeRow(header), "th")}</tr></thead>`
    : "";

  const tbodyRows = bodyRows
    .map((row) => `<tr>${renderCells(normalizeRow(row), "td")}</tr>`)
    .join("");

  return `<table style="width:100%;border-collapse:collapse;text-align:left;font-size:13px;line-height:1.5rem;color:var(--color-text-secondary)">${thead}<tbody>${tbodyRows}</tbody></table>`;
});

const imageTitle = computed(() => props.block.alt_text?.trim() || props.block.caption?.trim() || props.block.text.trim());
const imageSrc = ref("");

const loadImageSrc = async () => {
  imageSrc.value = "";

  const source = props.block.asset_path?.trim();
  if (!source) return;
  if (/^(https?:|data:|blob:)/i.test(source)) {
    imageSrc.value = source;
    return;
  }

  try {
    imageSrc.value = await invoke<string>("read_preview_image_data_url", { path: source });
    console.debug("[SeekMind] preview image data url loaded", {
      assetPath: source,
      length: imageSrc.value.length,
    });
    return;
  } catch (error) {
    console.warn("[SeekMind] preview image data url failed, fallback to convertFileSrc", {
      assetPath: source,
      error,
    });
  }

  imageSrc.value = convertFileSrc(source);
};

watch(
  () => props.block.asset_path,
  () => {
    void loadImageSrc();
  },
  { immediate: true }
);

const logImagePreview = (state: "load" | "error", event: Event) => {
  const target = event.target as HTMLImageElement | null;
  const payload = {
    state,
    blockIndex: props.block.block_index,
    title: imageTitle.value,
    assetPath: props.block.asset_path || "",
    resolvedSrc: imageSrc.value,
    currentSrc: target?.currentSrc || "",
    naturalWidth: target?.naturalWidth || 0,
    naturalHeight: target?.naturalHeight || 0,
  };
  if (state === "error") {
    console.warn("[SeekMind] preview image failed", payload);
    return;
  }
  console.debug("[SeekMind] preview image loaded", payload);
};
</script>

<template>
  <div class="preview-block">
    <div v-if="block.page" class="mb-2 flex items-center justify-end gap-2 text-[11px] text-muted">
      <span class="rounded-full bg-badge px-2 py-0.5">第 {{ block.page }} 页</span>
    </div>
    <div v-if="block.block_type === 'heading'" class="preview-heading" :class="`preview-heading--${block.level || 1}`">
      <span class="inline-flex items-center gap-1 font-semibold" :class="headingSize">
        <span v-if="block.heading" class="text-[11px] font-normal text-muted">{{ block.heading }} ›</span>
        {{ block.text }}
      </span>
    </div>

    <div v-else-if="block.block_type === 'image'" class="preview-image rounded-lg border border-default bg-surface p-3">
      <div class="flex items-start gap-3">
        <div class="flex-1 min-w-0">
          <div class="rounded-md bg-panel p-2">
            <img
              v-if="imageSrc"
              :src="imageSrc"
              :alt="block.alt_text || imageTitle"
              class="max-h-72 w-full rounded object-contain"
              @load="logImagePreview('load', $event)"
              @error="logImagePreview('error', $event)"
            />
            <div v-else class="flex min-h-24 items-center justify-center rounded border border-dashed border-default text-sm text-muted">
              图片预览不可用
            </div>
          </div>
          <div class="mt-2 space-y-1 text-xs text-dim">
            <div v-if="block.asset_path" class="break-all">源文件：{{ block.asset_path }}</div>
          </div>
        </div>
      </div>
    </div>

    <SeekMindMarkdownRenderer
      v-else-if="block.block_type === 'paragraph' || block.block_type === 'quote' || block.block_type === 'list' || block.block_type === 'code'"
      :block="block"
    />

    <div v-else-if="block.block_type === 'table'" class="preview-table overflow-x-auto rounded-md border border-default bg-surface">
      <div v-if="tableHtml" class="min-w-full" v-html="tableHtml"></div>
      <SeekMindMarkdownRenderer v-else :block="{ ...block, block_type: 'table' }" />
    </div>

    <div v-else class="preview-fallback text-sm leading-7 text-secondary">
      {{ block.text }}
    </div>
  </div>
</template>

<style scoped>
.preview-table :deep(table) {
  width: 100%;
  border-collapse: collapse;
  table-layout: fixed;
}

.preview-table :deep(thead) {
  background: var(--color-panel-bg);
}

.preview-table :deep(th),
.preview-table :deep(td) {
  border: 1px solid var(--color-border);
  padding: 0.5rem 0.75rem;
  vertical-align: top;
  white-space: normal;
  word-break: break-word;
}

.preview-table :deep(th) {
  font-weight: 600;
  color: var(--color-text-secondary);
}

.preview-table :deep(tbody tr:nth-child(even)) {
  background: var(--color-panel-bg);
}
</style>
