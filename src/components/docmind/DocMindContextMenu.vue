<script setup lang="ts">
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

const adjustedPosition = ref({ x: props.x, y: props.y });

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
      class="fixed z-[10000] min-w-[180px] rounded-lg border border-slate-200 bg-white py-1 shadow-lg"
      :style="{ left: `${adjustedPosition.x}px`, top: `${adjustedPosition.y}px` }"
      @click.stop
    >
      <template v-for="item in items" :key="item.key">
        <div v-if="item.divider" class="my-1 border-t border-slate-100" />
        <button
          v-else
          class="flex w-full items-center gap-2.5 px-3 py-1.5 text-left text-xs transition"
          :class="[
            item.danger ? 'text-red-600 hover:bg-red-50' : 'text-slate-700 hover:bg-slate-100',
            item.disabled ? 'cursor-not-allowed opacity-40' : 'cursor-pointer',
          ]"
          :disabled="item.disabled"
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
