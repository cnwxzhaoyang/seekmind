<script setup lang="ts">
import { computed } from "vue";
import type { HighlightSpan } from "../../types/SeekMind";

interface Props {
  text: string;
  query?: string;
  spans?: HighlightSpan[];
}

interface Segment {
  text: string;
  highlighted: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  query: "",
  spans: () => [],
});

const tokenizeQuery = (value: string) =>
  value
    .split(/[\s,，.。;；:：、|/\\]+/)
    .map((item) => item.trim())
    .filter(Boolean);

const querySpans = (text: string, query: string) => {
  const lowerText = text.toLowerCase();
  const spans: HighlightSpan[] = [];

  for (const term of tokenizeQuery(query)) {
    const needle = term.toLowerCase();
    const pattern = new RegExp(escapeRegExp(needle), "gi");
    let match: RegExpExecArray | null;
    while ((match = pattern.exec(lowerText)) !== null) {
      spans.push({ start: match.index, end: match.index + needle.length });
    }
  }

  return spans;
};

const escapeRegExp = (value: string) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

const mergedSpans = computed(() => {
  const spans = [...props.spans, ...querySpans(props.text, props.query)];
  if (!spans.length) {
    return [] as HighlightSpan[];
  }

  const sorted = spans
    .filter((span) => span.end > span.start)
    .sort((a, b) => a.start - b.start || a.end - b.end);

  const merged: HighlightSpan[] = [];
  for (const span of sorted) {
    const last = merged[merged.length - 1];
    if (last && span.start <= last.end) {
      last.end = Math.max(last.end, span.end);
      continue;
    }
    merged.push({ start: span.start, end: span.end });
  }

  return merged;
});

const segments = computed<Segment[]>(() => {
  const text = props.text ?? "";
  if (!text) {
    return [];
  }

  if (!mergedSpans.value.length) {
    return [{ text, highlighted: false }];
  }

  const result: Segment[] = [];
  let cursor = 0;
  for (const span of mergedSpans.value) {
    const start = Math.max(0, Math.min(span.start, text.length));
    const end = Math.max(start, Math.min(span.end, text.length));
    if (start > cursor) {
      result.push({ text: text.slice(cursor, start), highlighted: false });
    }
    if (end > start) {
      result.push({ text: text.slice(start, end), highlighted: true });
    }
    cursor = end;
  }
  if (cursor < text.length) {
    result.push({ text: text.slice(cursor), highlighted: false });
  }

  return result;
});
</script>

<template>
  <span class="whitespace-pre-wrap break-words">
    <template v-for="(segment, index) in segments" :key="`${index}-${segment.text}`">
      <mark
        v-if="segment.highlighted"
        class="rounded-sm bg-highlight px-0.5 py-0.5 font-medium text-highlight-text"
      >
        {{ segment.text }}
      </mark>
      <span v-else>{{ segment.text }}</span>
    </template>
  </span>
</template>
