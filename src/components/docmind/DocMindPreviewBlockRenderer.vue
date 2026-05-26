<script setup lang="ts">
import { computed } from "vue";
import type { PreviewBlockView } from "../../types/docmind";

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
    cells.map((cell) => `<${tag} class="border border-slate-200 px-3 py-2 align-top">${escapeHtml(cell)}</${tag}>`).join("");

  const thead = header
    ? `<thead class="bg-slate-50 text-slate-700"><tr>${renderCells(normalizeRow(header), "th")}</tr></thead>`
    : "";

  const tbodyRows = bodyRows
    .map((row) => `<tr>${renderCells(normalizeRow(row), "td")}</tr>`)
    .join("");

  return `<table class="w-full border-collapse text-left text-[13px] leading-6 text-slate-700">${thead}<tbody>${tbodyRows}</tbody></table>`;
});
</script>

<template>
  <div class="preview-block">
    <div v-if="block.block_type === 'heading'" class="preview-heading" :class="`preview-heading--${block.level || 1}`">
      <span class="inline-flex items-center gap-1 font-semibold" :class="headingSize">
        <span v-if="block.heading" class="text-[11px] font-normal text-slate-400">{{ block.heading }} ›</span>
        {{ block.text }}
      </span>
    </div>

    <div v-else-if="block.block_type === 'paragraph'" class="preview-paragraph text-sm leading-7 text-slate-700">
      {{ block.text }}
    </div>

    <div
      v-else-if="block.block_type === 'code'"
      class="preview-code overflow-x-auto rounded-md bg-slate-900 px-3 py-2 text-[13px] leading-6 text-green-300"
    >
      <pre class="m-0 whitespace-pre-wrap font-mono">{{ block.text }}</pre>
    </div>

    <div v-else-if="block.block_type === 'table'" class="preview-table overflow-x-auto rounded-md border border-slate-200 bg-white">
      <div v-if="tableHtml" class="min-w-full" v-html="tableHtml"></div>
      <div v-else class="whitespace-pre-wrap px-3 py-2 text-sm leading-7 text-slate-700">{{ block.text }}</div>
    </div>

    <blockquote
      v-else-if="block.block_type === 'quote'"
      class="preview-quote border-l-2 border-slate-300 pl-3 text-sm leading-7 text-slate-600 italic"
    >
      {{ block.text }}
    </blockquote>

    <div v-else-if="block.block_type === 'list'" class="preview-list text-sm leading-7 text-slate-700">
      <span class="mr-1.5 text-slate-400">•</span>{{ block.text }}
    </div>

    <div v-else class="preview-fallback text-sm leading-7 text-slate-700">
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
  background: rgb(248 250 252);
}

.preview-table :deep(th),
.preview-table :deep(td) {
  border: 1px solid rgb(226 232 240);
  padding: 0.5rem 0.75rem;
  vertical-align: top;
  white-space: normal;
  word-break: break-word;
}

.preview-table :deep(th) {
  font-weight: 600;
  color: rgb(51 65 85);
}

.preview-table :deep(tbody tr:nth-child(even)) {
  background: rgb(248 250 252);
}
</style>
