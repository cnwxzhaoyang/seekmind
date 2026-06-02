<script setup lang="ts">
/**
 * @author MorningSun
 * @CreatedDate 2026/06/02
 * @Description 通用右键菜单组件，负责菜单项展示、鼠标高亮和键盘导航。
 */
import { ref, onMounted, onBeforeUnmount } from "vue";

export interface ContextMenuItem {
  key: string;
  label: string;
  icon?: any;
  disabled?: boolean;
  divider?: boolean;
  danger?: boolean;
  handler?: () => void;
}

interface Props {
  items: ContextMenuItem[];
  x: number;
  y: number;
}

const props = defineProps<Props>();
const emit = defineEmits<{ close: [] }>();

const menu = ref<HTMLElement | null>(null);
const activeIndex = ref(-1);
const itemRefs = ref<(HTMLButtonElement | null)[]>([]);
const adjustedPosition = ref({ x: props.x, y: props.y });

const getEnabledItemIndexes = () =>
  props.items
    .map((item, index) => ({ item, index }))
    .filter(({ item }) => !item.divider && !item.disabled)
    .map(({ index }) => index);

const setActiveIndex = (index: number) => {
  if (props.items[index]?.divider || props.items[index]?.disabled) {
    return;
  }
  activeIndex.value = index;
};

const focusItem = (index: number) => {
  setActiveIndex(index);
  itemRefs.value[index]?.focus();
};

onMounted(() => {
  if (!menu.value) return;
  const rect = menu.value.getBoundingClientRect();
  const { innerWidth, innerHeight } = window;
  if (props.x + rect.width > innerWidth) {
    adjustedPosition.value.x = innerWidth - rect.width - 8;
  }
  if (props.y + rect.height > innerHeight) {
    adjustedPosition.value.y = innerHeight - rect.height - 8;
  }

  const enabledIndexes = getEnabledItemIndexes();
  if (enabledIndexes.length > 0) {
    // 修复：菜单打开后预选第一项，鼠标和键盘都能获得明确的高亮态。
    activeIndex.value = enabledIndexes[0];
  }
});

const closeOnClick = () => emit("close");

const handleItemClick = (item: ContextMenuItem) => {
  if (item.disabled || !item.handler) return;
  item.handler();
  emit("close");
};

const handleKeydown = (e: KeyboardEvent) => {
  if (e.key === "Escape") {
    emit("close");
    return;
  }

  const enabledIndexes = getEnabledItemIndexes();
  if (!enabledIndexes.length) {
    return;
  }

  const currentIndex = enabledIndexes.indexOf(activeIndex.value);

  if (e.key === "ArrowDown") {
    e.preventDefault();
    const nextIndex = currentIndex >= 0 ? enabledIndexes[(currentIndex + 1) % enabledIndexes.length] : enabledIndexes[0];
    focusItem(nextIndex);
    return;
  }

  if (e.key === "ArrowUp") {
    e.preventDefault();
    const nextIndex = currentIndex >= 0
      ? enabledIndexes[(currentIndex - 1 + enabledIndexes.length) % enabledIndexes.length]
      : enabledIndexes[enabledIndexes.length - 1];
    focusItem(nextIndex);
    return;
  }

  if (e.key === "Enter" || e.key === " ") {
    e.preventDefault();
    const activeItem = props.items[activeIndex.value];
    if (activeItem && !activeItem.disabled && !activeItem.divider) {
      handleItemClick(activeItem);
    }
  }
};

onMounted(() => {
  document.addEventListener("keydown", handleKeydown);
});

onBeforeUnmount(() => {
  document.removeEventListener("keydown", handleKeydown);
});
</script>

<template>
  <teleport to="body">
    <div
      class="fixed inset-0 z-[9999]"
      @click="closeOnClick"
      @contextmenu.prevent="closeOnClick"
    />
    <div
      ref="menu"
      class="fixed z-[10000] min-w-[180px] rounded-lg border border-default bg-surface py-1 shadow-lg shadow-card"
      :style="{ left: `${adjustedPosition.x}px`, top: `${adjustedPosition.y}px` }"
      @click.stop
    >
      <template v-for="(item, itemIndex) in items" :key="item.key">
        <div v-if="item.divider" class="my-1 border-t border-light" />
        <button
          v-else
          :ref="(el) => { itemRefs[itemIndex] = el as HTMLButtonElement | null; }"
          class="docmind-context-menu-item flex w-full items-center gap-2.5 rounded-md px-3 py-1.5 text-left text-xs transition focus-visible:outline-none"
          :class="[
            item.danger
              ? activeIndex === itemIndex
                ? 'docmind-context-menu-item-danger-active text-danger'
                : 'text-danger hover:bg-danger-soft active:bg-danger-soft focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-danger-soft'
              : activeIndex === itemIndex
                ? 'docmind-context-menu-item-active text-primary'
                : 'text-secondary hover:bg-surface-hover active:bg-surface-hover focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent-soft',
            item.disabled ? 'cursor-not-allowed opacity-40' : 'cursor-pointer',
          ]"
          :disabled="item.disabled"
          @mouseenter="setActiveIndex(itemIndex)"
          @focus="setActiveIndex(itemIndex)"
          @click="handleItemClick(item)"
        >
          <span v-if="item.icon" class="inline-flex h-4 w-4 items-center justify-center">
            <component :is="item.icon" :size="14" />
          </span>
          <span v-else class="inline-flex h-4 w-4" />
          {{ item.label }}
        </button>
      </template>
    </div>
  </teleport>
</template>

<style scoped>
.docmind-context-menu-item-active {
  background: var(--color-accent-soft);
  box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--color-accent) 20%, transparent);
}

.docmind-context-menu-item-danger-active {
  background: var(--color-danger-soft);
  box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--color-danger) 18%, transparent);
}
</style>
