<script setup lang="ts">
import { useI18n } from "vue-i18n";
import SeekMindIcon from "./SeekMindIcon.vue";

interface Props {
  visible: boolean;
  title?: string;
  message?: string;
  confirmText?: string;
  cancelText?: string;
  danger?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  title: "",
  message: "",
  confirmText: "",
  cancelText: "",
  danger: false,
});

const emit = defineEmits<{
  confirm: [];
  cancel: [];
}>();

const { t } = useI18n();
</script>

<template>
  <teleport to="body">
    <div
      v-if="visible"
      class="fixed inset-0 z-[12000] bg-black/24 backdrop-blur-[1px]"
      @click="emit('cancel')"
    />
    <div
      v-if="visible"
      class="fixed left-1/2 top-1/2 z-[12001] w-[min(92vw,420px)] -translate-x-1/2 -translate-y-1/2 overflow-hidden rounded-xl border border-default bg-surface shadow-2xl"
    >
      <div class="flex items-start justify-between gap-3 px-5 pt-5">
        <div class="flex items-center gap-3">
          <div
            class="flex h-10 w-10 shrink-0 items-center justify-center rounded-full"
            :class="danger ? 'bg-danger-soft text-danger' : 'bg-accent-soft text-accent'"
          >
            <SeekMindIcon :icon="danger ? 'icon-warning' : 'icon-info'" :size="20" />
          </div>
          <div class="text-sm font-semibold text-primary">{{ title || t("common.confirm") }}</div>
        </div>
        <button
          class="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-muted transition hover:bg-surface-hover hover:text-primary"
          type="button"
          :aria-label="t('common.close')"
          @click="emit('cancel')"
        >
          <SeekMindIcon icon="icon-close" :size="16" />
        </button>
      </div>

      <div class="px-5 py-4 text-sm leading-6 text-secondary">
        {{ message }}
      </div>

      <div class="flex items-center justify-end gap-2 border-t border-default bg-panel/30 px-5 py-3">
        <button
          class="inline-flex h-9 items-center rounded-md border border-default bg-surface px-4 text-sm font-medium text-secondary transition hover:bg-surface-hover hover:text-primary"
          type="button"
          @click="emit('cancel')"
        >
          {{ cancelText || t("common.cancel") }}
        </button>
        <button
          class="inline-flex h-9 items-center rounded-md px-4 text-sm font-medium text-white transition"
          :class="danger ? 'bg-danger hover:opacity-80' : 'bg-accent hover:bg-accent-strong'"
          type="button"
          @click="emit('confirm')"
        >
          {{ confirmText || t("common.confirm") }}
        </button>
      </div>
    </div>
  </teleport>
</template>
