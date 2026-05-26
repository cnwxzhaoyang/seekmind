<script setup lang="ts">
import { computed } from "vue";
import MarkdownIt from "markdown-it";
import DOMPurify from "dompurify";
import type { HighlightSpan, PreviewBlockView } from "../../types/docmind";
import DocMindHighlightedText from "./DocMindHighlightedText.vue";

const props = defineProps<{
  block: PreviewBlockView;
  query?: string;
  highlightText?: string;
  highlightSpans?: HighlightSpan[];
}>();

const markdownIt = new MarkdownIt({
  html: false,
  linkify: true,
  breaks: true,
});

const highlightText = computed(() => props.highlightText ?? props.block.text);
const highlightSpans = computed(() => props.highlightSpans ?? []);
const hasHighlight = computed(() => highlightSpans.value.length > 0 || Boolean(props.query?.trim()));

const escapeFence = (value: string) => value.replace(/```/g, "\\`\\`\\`");

const sourceMarkdown = computed(() => {
  const text = (props.block.text || "").replace(/\r\n/g, "\n").trimEnd();
  switch (props.block.block_type) {
    case "quote":
      return text
        .split("\n")
        .map((line) => `> ${line || " "}`)
        .join("\n");
    case "list":
      return props.block.markdown?.trim() || text
        .split("\n")
        .map((line) => `- ${line || " "}`)
        .join("\n");
    case "code": {
      const language = (props.block.language || "").trim();
      const fence = "```";
      return `${fence}${language}\n${escapeFence(text)}\n${fence}`;
    }
    case "table":
      return props.block.markdown?.trim() || text;
    case "paragraph":
    default:
      return text;
  }
});

const renderedHtml = computed(() => {
  const rawHtml = markdownIt.render(sourceMarkdown.value || "");
  return DOMPurify.sanitize(rawHtml, {
    USE_PROFILES: { html: true },
  });
});
</script>

<template>
  <div class="docmind-markdown-renderer markdown-body">
    <div
      v-if="block.block_type === 'paragraph' && hasHighlight"
      class="text-sm leading-7 text-slate-700"
    >
      <DocMindHighlightedText
        :text="highlightText"
        :query="query"
        :spans="highlightSpans"
      />
    </div>
    <div v-else v-html="renderedHtml"></div>
  </div>
</template>

<style scoped>
.docmind-markdown-renderer {
  color: rgb(51 65 85);
  font-size: 14px;
  line-height: 1.75;
}

.docmind-markdown-renderer :deep(p) {
  margin: 0 0 0.75rem;
}

.docmind-markdown-renderer :deep(p:last-child) {
  margin-bottom: 0;
}

.docmind-markdown-renderer :deep(blockquote) {
  margin: 0;
  padding-left: 0.875rem;
  border-left: 3px solid rgb(203 213 225);
  color: rgb(71 85 105);
}

.docmind-markdown-renderer :deep(ul),
.docmind-markdown-renderer :deep(ol) {
  margin: 0.25rem 0 0.75rem;
  padding-left: 1.25rem;
}

.docmind-markdown-renderer :deep(li + li) {
  margin-top: 0.25rem;
}

.docmind-markdown-renderer :deep(pre) {
  overflow-x: auto;
  margin: 0.5rem 0 0;
  padding: 0.875rem 1rem;
  border-radius: 0.5rem;
  background: rgb(15 23 42);
  color: rgb(191 219 254);
}

.docmind-markdown-renderer :deep(code) {
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace;
  font-size: 0.9em;
}

.docmind-markdown-renderer :deep(pre code) {
  display: block;
  white-space: pre;
  color: inherit;
}

.docmind-markdown-renderer :deep(table) {
  width: 100%;
  border-collapse: collapse;
  table-layout: fixed;
  margin: 0.5rem 0 0;
  font-size: 13px;
}

.docmind-markdown-renderer :deep(th),
.docmind-markdown-renderer :deep(td) {
  border: 1px solid rgb(226 232 240);
  padding: 0.5rem 0.75rem;
  vertical-align: top;
  word-break: break-word;
}

.docmind-markdown-renderer :deep(th) {
  background: rgb(248 250 252);
  font-weight: 600;
  color: rgb(51 65 85);
}
</style>
