import { ref, watch, onUnmounted } from "vue";

const DISMISS_MS = 3000;

export const useInfoMessage = () => {
  const infoMessage = ref("");
  let dismissTimer: ReturnType<typeof setTimeout> | null = null;

  watch(infoMessage, (newVal) => {
    if (dismissTimer) {
      clearTimeout(dismissTimer);
      dismissTimer = null;
    }
    if (newVal) {
      dismissTimer = setTimeout(() => {
        infoMessage.value = "";
        dismissTimer = null;
      }, DISMISS_MS);
    }
  });

  onUnmounted(() => {
    if (dismissTimer) {
      clearTimeout(dismissTimer);
      dismissTimer = null;
    }
  });

  return { infoMessage };
};
