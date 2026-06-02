<template>
  <svg
    :class="['svg-icon', `icon-${size}`, className]"
    :width="iconSize"
    :height="iconSize"
    aria-hidden="true"
  >
    <use :href="`#${icon}`" />
  </svg>
</template>

<script setup lang="ts">
import { computed } from "vue";

interface Props {
  icon: string;
  size?: "sm" | "md" | "lg" | "xl";
  className?: string;
}

const props = withDefaults(defineProps<Props>(), {
  size: "md",
  className: "",
});

const iconSize = computed(() => {
  const sizes: Record<string, number> = {
    sm: 16,
    md: 20,
    lg: 24,
    xl: 32,
  };
  return sizes[props.size];
});
</script>

<style scoped>
.svg-icon {
  display: inline-block;
  vertical-align: -0.125em;
  fill: currentColor;
  overflow: visible;
  transition: all 0.2s ease;
}

.icon-sm {
  width: 16px;
  height: 16px;
}

.icon-md {
  width: 20px;
  height: 20px;
}

.icon-lg {
  width: 24px;
  height: 24px;
}

.icon-xl {
  width: 32px;
  height: 32px;
}

.svg-icon:hover {
  opacity: 0.8;
}

.svg-icon:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
