<script setup lang="ts">
/**
 * @author MorningSun
 * @CreatedDate 2026/06/11
 * @Description SeekMind 浮动提示条，固定在视口右上角，不占用页面布局。
 */
import { computed } from "vue";
import { AlertCircle, CheckCircle2, TriangleAlert } from "lucide-vue-next";

defineOptions({
  name: "SeekMindToast",
});

const props = withDefaults(defineProps<{
  message: string;
  tone?: "info" | "success" | "error";
}>(), {
  tone: "info",
});

const toneConfig = computed(() => {
  switch (props.tone) {
    case "success":
      return {
        container: "border-emerald-200 bg-emerald-50 text-success shadow-[0_10px_30px_rgba(16,185,129,0.12)]",
        icon: CheckCircle2,
      };
    case "error":
      return {
        container: "border-danger-soft bg-danger-soft text-danger shadow-[0_10px_30px_rgba(239,68,68,0.12)]",
        icon: TriangleAlert,
      };
    default:
      return {
        container: "border-accent/20 bg-white text-primary shadow-[0_10px_30px_rgba(15,23,42,0.12)]",
        icon: AlertCircle,
      };
  }
});
</script>

<template>
  <Teleport to="body">
    <div class="pointer-events-none fixed right-4 top-4 z-[80] w-[min(28rem,calc(100vw-2rem))]">
      <div
        class="pointer-events-auto flex items-start gap-2 rounded-[16px] border px-4 py-3 text-sm backdrop-blur-sm"
        :class="toneConfig.container"
        role="status"
        aria-live="polite"
      >
        <component :is="toneConfig.icon" :size="16" class="mt-0.5 shrink-0" />
        <div class="min-w-0 flex-1 break-words leading-6">
          {{ message }}
        </div>
      </div>
    </div>
  </Teleport>
</template>
