<template>
  <svg
    :class="['svg-icon', `icon-${size}`, className]"
    :width="iconSize"
    :height="iconSize"
    :style="{ color }"
    aria-hidden="true"
  >
    <use :xlink:href="`#${icon}`" />
  </svg>
</template>

<script setup lang="ts">
import { computed } from 'vue'

interface Props {
  /**
   * 图标名称（对应 icons.svg 中的 symbol id）
   * 例如：'icon-search', 'icon-document', 'icon-folder'
   */
  icon: string
  /**
   * 图标大小
   * sm: 16px, md: 20px, lg: 24px, xl: 32px
   */
  size?: 'sm' | 'md' | 'lg' | 'xl'
  /**
   * 图标颜色
   */
  color?: string
  /**
   * 自定义 CSS 类名
   */
  className?: string
}

const props = withDefaults(defineProps<Props>(), {
  size: 'md',
  color: 'currentColor',
  className: ''
})

const iconSize = computed(() => {
  const sizes: Record<string, number> = {
    sm: 16,
    md: 20,
    lg: 24,
    xl: 32
  }
  return sizes[props.size]
})
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

/* 悬停效果 */
.svg-icon:hover {
  opacity: 0.8;
}

/* 禁用状态 */
.svg-icon:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
