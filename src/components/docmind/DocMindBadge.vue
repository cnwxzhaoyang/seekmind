<script setup lang="ts">
type BadgeTone = "default" | "success" | "warning" | "danger";

interface Props {
  tone?: BadgeTone;
}

const props = withDefaults(defineProps<Props>(), {
  tone: "default",
});

const toneClassMap: Record<BadgeTone, string> = {
  default: "bg-badge text-secondary border-default",
  success: "bg-emerald-soft text-success border-emerald-soft",
  warning: "bg-amber-soft text-warning border-amber-soft",
  danger: "bg-danger-soft text-danger border-danger-soft",
};
</script>

<template>
  <span
    class="docmind-badge inline-flex items-center rounded-full border px-2 py-0.5 text-[12px] leading-none"
    :class="toneClassMap[props.tone]"
  >
    <slot />
  </span>
</template>
