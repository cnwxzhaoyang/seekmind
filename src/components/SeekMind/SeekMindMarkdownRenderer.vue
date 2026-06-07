<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import MarkdownIt from "markdown-it";
import DOMPurify from "dompurify";
import type { HighlightSpan, PreviewBlockView } from "../../types/SeekMind";
import SeekMindHighlightedText from "./SeekMindHighlightedText.vue";

const props = defineProps<{
  block: PreviewBlockView;
  query?: string;
  highlightText?: string;
  highlightSpans?: HighlightSpan[];
  markdown?: string;
  citationSourceIds?: string[];
}>();

const emit = defineEmits<{
  (event: "citation-click", sourceId: string): void;
}>();

const markdownIt = new MarkdownIt({
  html: false,
  linkify: true,
  breaks: true,
});

const highlightText = computed(() => props.highlightText ?? props.block.text);
const highlightSpans = computed(() => props.highlightSpans ?? []);
const hasHighlight = computed(() => highlightSpans.value.length > 0 || Boolean(props.query?.trim()));
const citationSourceIdSet = computed(() => new Set((props.citationSourceIds ?? []).map((item) => item.trim()).filter(Boolean)));
const contentRef = ref<HTMLElement | null>(null);

const escapeFence = (value: string) => value.replace(/```/g, "\\`\\`\\`");

const sourceMarkdown = computed(() => {
  if (props.markdown !== undefined) {
    return props.markdown.trimEnd();
  }

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

const citationPattern = /\[(\d+)\]/g;

const decorateCitationTokens = async () => {
  await nextTick();
  const root = contentRef.value;
  if (!root || citationSourceIdSet.value.size === 0) {
    return;
  }

  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const textNodes: Text[] = [];
  while (walker.nextNode()) {
    const node = walker.currentNode as Text;
    const parent = node.parentElement;
    if (!parent || parent.closest("pre, code, .seekmind-citation-chip")) {
      continue;
    }
    if (!citationPattern.test(node.textContent || "")) {
      citationPattern.lastIndex = 0;
      continue;
    }
    citationPattern.lastIndex = 0;
    textNodes.push(node);
  }

  textNodes.forEach((node) => {
    const text = node.textContent || "";
    citationPattern.lastIndex = 0;
    const matches = [...text.matchAll(citationPattern)];
    if (matches.length === 0) {
      return;
    }

    const fragment = document.createDocumentFragment();
    let lastIndex = 0;
    for (const match of matches) {
      const [token, sourceId] = match;
      const start = match.index ?? 0;
      if (start > lastIndex) {
        fragment.appendChild(document.createTextNode(text.slice(lastIndex, start)));
      }

      if (citationSourceIdSet.value.has(sourceId)) {
        const button = document.createElement("button");
        button.type = "button";
        button.className =
          "seekmind-citation-chip inline-flex items-center rounded-full border border-accent/20 bg-accent-soft px-2 py-0.5 text-[11px] font-medium text-accent transition hover:bg-accent-soft/80";
        button.textContent = token;
        button.dataset.sourceId = sourceId;
        // Fix: citation chips are rendered from markdown text and must not be reprocessed
        // on the next decoration pass, otherwise they keep nesting inside themselves.
        button.addEventListener("click", () => {
          emit("citation-click", sourceId);
        });
        fragment.appendChild(button);
      } else {
        fragment.appendChild(document.createTextNode(token));
      }

      lastIndex = start + token.length;
    }

    if (lastIndex < text.length) {
      fragment.appendChild(document.createTextNode(text.slice(lastIndex)));
    }

    node.parentNode?.replaceChild(fragment, node);
  });
};

watch(renderedHtml, () => {
  void decorateCitationTokens();
});

watch(citationSourceIdSet, () => {
  void decorateCitationTokens();
});

onMounted(() => {
  void decorateCitationTokens();
});
</script>

<template>
  <div class="seekmind-markdown-renderer markdown-body" ref="contentRef">
    <div v-if="props.markdown !== undefined" v-html="renderedHtml"></div>
    <div
      v-else-if="block.block_type === 'paragraph' && hasHighlight"
      class="text-sm leading-7 text-secondary"
    >
      <SeekMindHighlightedText
        :text="highlightText"
        :query="query"
        :spans="highlightSpans"
      />
    </div>
    <div v-else v-html="renderedHtml"></div>
  </div>
</template>

<style scoped>
.seekmind-markdown-renderer {
  color: var(--color-text-secondary);
  font-size: 14px;
  line-height: 1.75;
}

.seekmind-citation-chip {
  margin: 0 0.125rem;
  transform: translateY(-0.05em);
  box-shadow: none;
}

.seekmind-markdown-renderer :deep(p) {
  margin: 0 0 0.75rem;
}

.seekmind-markdown-renderer :deep(p:last-child) {
  margin-bottom: 0;
}

.seekmind-markdown-renderer :deep(blockquote) {
  margin: 0;
  padding-left: 0.875rem;
  border-left: 3px solid var(--color-border);
  color: var(--color-text-dim);
}

.seekmind-markdown-renderer :deep(ul),
.seekmind-markdown-renderer :deep(ol) {
  margin: 0.25rem 0 0.75rem;
  padding-left: 1.25rem;
}

.seekmind-markdown-renderer :deep(li + li) {
  margin-top: 0.25rem;
}

.seekmind-markdown-renderer :deep(pre) {
  overflow-x: auto;
  margin: 0.5rem 0 0;
  padding: 0.875rem 1rem;
  border-radius: 0.5rem;
  background: rgb(15 23 42);
  color: rgb(191 219 254);
}

.seekmind-markdown-renderer :deep(code) {
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace;
  font-size: 0.9em;
}

.seekmind-markdown-renderer :deep(pre code) {
  display: block;
  white-space: pre;
  color: inherit;
}

.seekmind-markdown-renderer :deep(table) {
  width: 100%;
  border-collapse: collapse;
  table-layout: fixed;
  margin: 0.5rem 0 0;
  font-size: 13px;
}

.seekmind-markdown-renderer :deep(th),
.seekmind-markdown-renderer :deep(td) {
  border: 1px solid var(--color-border);
  padding: 0.5rem 0.75rem;
  vertical-align: top;
  word-break: break-word;
}

.seekmind-markdown-renderer :deep(th) {
  background: var(--color-panel-bg);
  font-weight: 600;
  color: var(--color-text-secondary);
}
</style>
