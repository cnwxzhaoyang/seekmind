<script setup lang="ts">
import { ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import type { CollectionView } from "../../types/SeekMind";
import SeekMindIcon from "./SeekMindIcon.vue";

interface Props {
  visible: boolean;
  collections: CollectionView[];
  loading?: boolean;
  title?: string;
  subtitle?: string;
}

const props = withDefaults(defineProps<Props>(), {
  loading: false,
  title: "",
  subtitle: "",
});

const emit = defineEmits<{
  close: [];
  select: [collectionId: string];
  create: [name: string];
}>();

const { t } = useI18n();
const newCollectionName = ref("");

watch(
  () => props.visible,
  (visible) => {
    if (visible) {
      newCollectionName.value = "";
    }
  },
);

const handleCreate = () => {
  const name = newCollectionName.value.trim();
  if (!name) {
    return;
  }
  emit("create", name);
  newCollectionName.value = "";
};
</script>

<template>
  <teleport to="body">
    <div
      v-if="visible"
      class="fixed inset-0 z-[12000] bg-black/24 backdrop-blur-[1px]"
      @click="emit('close')"
    />
    <div
      v-if="visible"
      class="fixed left-1/2 top-1/2 z-[12001] flex w-[min(92vw,680px)] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-xl border border-default bg-surface shadow-2xl"
    >
      <div class="flex items-start justify-between gap-3 border-b border-default px-4 py-3">
        <div class="min-w-0">
          <div class="text-sm font-semibold text-primary">{{ title || t("page.collections.pickerTitle") }}</div>
          <div class="mt-1 text-xs text-muted">{{ subtitle || t("page.collections.pickerSubtitle") }}</div>
        </div>
        <button
          class="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-muted transition hover:bg-surface-hover hover:text-primary"
          type="button"
          :aria-label="t('common.close')"
          @click="emit('close')"
        >
          <SeekMindIcon icon="icon-close" :size="16" />
        </button>
      </div>

      <div class="max-h-[52vh] overflow-y-auto p-4">
        <div v-if="loading" class="rounded-lg border border-dashed border-default bg-panel/40 px-4 py-6 text-center text-xs text-muted">
          {{ t("common.loading") }}
        </div>
        <div v-else-if="collections.length === 0" class="rounded-lg border border-dashed border-default bg-panel/40 px-4 py-6 text-center text-xs text-muted">
          {{ t("page.collections.emptyCollections") }}
        </div>
        <div v-else class="space-y-2">
          <button
            v-for="collection in collections"
            :key="collection.id"
            class="flex w-full items-start justify-between gap-3 rounded-lg border border-default bg-panel/40 px-3 py-3 text-left transition hover:border-accent hover:bg-accent-soft/30"
            type="button"
            @click="emit('select', collection.id)"
          >
            <div class="min-w-0">
              <div class="truncate text-sm font-medium text-primary">{{ collection.name }}</div>
              <div class="mt-1 max-h-10 overflow-hidden text-xs leading-5 text-muted">
                {{ collection.description || t("page.collections.noDescription") }}
              </div>
            </div>
            <div class="shrink-0 text-xs text-dim">
              {{ t("page.collections.itemCount", { count: collection.item_count }) }}
            </div>
          </button>
        </div>
      </div>

      <div class="border-t border-default bg-panel/30 p-4">
        <div class="flex items-end gap-2">
          <div class="min-w-0 flex-1">
            <label class="mb-1 block text-[11px] font-medium uppercase tracking-[0.16em] text-dim">
              {{ t("page.collections.createQuick") }}
            </label>
            <input
              v-model="newCollectionName"
              class="w-full rounded-lg border border-default bg-surface px-3 py-2 text-sm text-primary placeholder:text-muted focus:border-accent focus:outline-none"
              type="text"
              :placeholder="t('page.collections.quickPlaceholder')"
              @keydown.enter.prevent="handleCreate"
            >
          </div>
          <button
            class="inline-flex h-10 items-center gap-2 rounded-lg bg-accent px-4 text-sm font-medium text-white transition hover:bg-accent-strong disabled:cursor-not-allowed disabled:opacity-60"
            type="button"
            :disabled="!newCollectionName.trim()"
            @click="handleCreate"
          >
            <SeekMindIcon icon="icon-plus" :size="16" />
            {{ t("page.collections.create") }}
          </button>
        </div>
      </div>
    </div>
  </teleport>
</template>
