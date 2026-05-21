<script setup lang="ts">
type BadgeTone = "default" | "success" | "warning" | "danger";

interface Props {
  tone?: BadgeTone;
}

const props = withDefaults(defineProps<Props>(), {
  tone: "default",
});

const toneClassMap: Record<BadgeTone, string> = {
  default: "bg-slate-50 text-slate-600 border-slate-100",
  success: "bg-emerald-50 text-emerald-700 border-emerald-100",
  warning: "bg-amber-50 text-amber-700 border-amber-100",
  danger: "bg-red-50 text-red-700 border-red-100",
};
</script>

<template>
  <span
    class="docmind-badge inline-flex items-center rounded-full border px-2 py-0.5 text-xs"
    :class="toneClassMap[props.tone]"
  >
    <slot />
  </span>
</template>
