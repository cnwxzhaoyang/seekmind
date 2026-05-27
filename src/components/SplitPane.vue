<script setup lang="ts">
import { ref, computed, onMounted, watch, nextTick } from "vue";

interface PanelConfig {
  key: string;
  initialSize?: number;
  minSize: number;
  flex?: boolean;
}

interface Props {
  panels: PanelConfig[];
}

const props = defineProps<Props>();

const container = ref<HTMLElement | null>(null);
const panelRefs = ref<Map<string, HTMLElement>>(new Map());
const dragging = ref<{ dividerIndex: number; startX: number; sizes: string[] } | null>(null);

const setPanelRef = (key: string) => (el: any) => {
  if (el) panelRefs.value.set(key, el as HTMLElement);
  else panelRefs.value.delete(key);
};

const initSizes = () => {
  if (!container.value) return;
  const totalWidth = container.value.getBoundingClientRect().width;
  if (totalWidth === 0) return;
  const flexPanels = props.panels.filter((p) => p.flex);

  const fixedWidth = props.panels
    .filter((p) => !p.flex)
    .reduce((sum, p) => sum + (p.initialSize ?? p.minSize), 0);

  const flexWidth = Math.max(0, totalWidth - fixedWidth);
  const perFlexPanel = flexPanels.length > 0 ? flexWidth / flexPanels.length : 0;

  for (const panel of props.panels) {
    const el = panelRefs.value.get(panel.key);
    if (!el) continue;
    if (panel.flex) {
      el.style.flex = `${perFlexPanel} 1 0`;
      el.style.width = "auto";
    } else {
      el.style.width = `${panel.initialSize ?? panel.minSize}px`;
      el.style.flex = "0 0 auto";
    }
  }
};

const getCurrentSizes = (): string[] => {
  return props.panels.map((p) => {
    const el = panelRefs.value.get(p.key);
    if (!el) return "0";
    return `${el.getBoundingClientRect().width}px`;
  });
};

const applyResize = (dividerIndex: number, startSizes: string[], dx: number) => {
  const leftIdx = dividerIndex;
  const rightIdx = dividerIndex + 1;
  const leftPanel = props.panels[leftIdx];
  const rightPanel = props.panels[rightIdx];
  const leftEl = panelRefs.value.get(leftPanel.key);
  const rightEl = panelRefs.value.get(rightPanel.key);
  if (!leftPanel || !rightPanel || !leftEl || !rightEl) return;

  const leftStart = parseFloat(startSizes[leftIdx]);
  const rightStart = parseFloat(startSizes[rightIdx]);
  const totalWidth = leftStart + rightStart;

  let leftSize = leftStart + dx;
  let rightSize = rightStart - dx;

  leftSize = Math.max(leftPanel.minSize, Math.min(leftSize, totalWidth - rightPanel.minSize));
  rightSize = totalWidth - leftSize;

  if (!leftPanel.flex) {
    leftEl.style.width = `${leftSize}px`;
    leftEl.style.flex = "0 0 auto";
  } else {
    leftEl.style.flex = `${leftSize} 1 0`;
  }
  if (!rightPanel.flex) {
    rightEl.style.width = `${rightSize}px`;
    rightEl.style.flex = "0 0 auto";
  } else {
    rightEl.style.flex = `${rightSize} 1 0`;
  }
};

const onDividerMouseDown = (index: number, event: MouseEvent) => {
  event.preventDefault();
  const sizes = getCurrentSizes();
  dragging.value = { dividerIndex: index, startX: event.clientX, sizes };

  const onMouseMove = (e: MouseEvent) => {
    if (!dragging.value) return;
    applyResize(dragging.value.dividerIndex, dragging.value.sizes, e.clientX - dragging.value.startX);
  };

  const onMouseUp = () => {
    dragging.value = null;
    document.removeEventListener("mousemove", onMouseMove);
    document.removeEventListener("mouseup", onMouseUp);
  };

  document.addEventListener("mousemove", onMouseMove);
  document.addEventListener("mouseup", onMouseUp);
};

const containerStyle = computed(() => {
  if (dragging.value) {
    return { cursor: "col-resize", userSelect: "none" as const };
  }
  return {};
});

onMounted(initSizes);

watch(() => props.panels.length, () => {
  nextTick(initSizes);
});
</script>

<template>
  <div ref="container" class="flex min-h-0 flex-1 overflow-hidden" :style="containerStyle">
    <template v-for="(panel, index) in panels" :key="panel.key">
      <div
        :ref="setPanelRef(panel.key)"
        class="flex min-h-0 overflow-hidden"
      >
        <slot :name="panel.key" />
      </div>
      <div
        v-if="index < panels.length - 1"
        class="relative w-[3px] cursor-col-resize shrink-0 bg-transparent hover:bg-accent active:bg-accent transition-colors"
        :class="{ 'bg-accent': dragging?.dividerIndex === index }"
        @mousedown.prevent="onDividerMouseDown(index, $event)"
      >
        <div class="absolute inset-y-0 -left-1 -right-1" />
      </div>
    </template>
  </div>
</template>
