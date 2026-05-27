<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { Loader2 } from "lucide-vue-next";
import DocMindBadge from "./DocMindBadge.vue";
import type { CurrentTaskView } from "../../types/docmind";

const { t } = useI18n();

const props = withDefaults(
  defineProps<{
    task: CurrentTaskView | null;
    title?: string;
    description?: string;
    badgeLabel?: string;
    badgeTone?: "default" | "success" | "warning" | "danger";
    badgeSpinning?: boolean;
  }>(),
  {
    title: "",
    description: "",
    badgeLabel: "",
    badgeTone: "warning",
    badgeSpinning: false,
  },
);

const displayTitle = computed(() => props.title || t("taskCard.defaultTitle"));
const displayDesc = computed(() => props.description || props.task?.details || t("taskCard.defaultDesc"));
const displayBadge = computed(() => props.badgeLabel || t("status.running"));

const remainingCount = computed(() => {
  if (!props.task) {
    return 0;
  }
  return Math.max(props.task.total - props.task.scanned, 0);
});
</script>

<template>
  <div v-if="props.task" class="rounded-lg border border-default bg-surface p-3">
    <div class="mb-2.5 flex items-center justify-between">
      <div>
        <div class="text-xs font-semibold uppercase tracking-wide text-dim">{{ displayTitle }}</div>
        <div class="mt-1 text-xs text-dim">{{ displayDesc }}</div>
      </div>
      <DocMindBadge :tone="badgeTone">
        <Loader2 v-if="badgeSpinning" class="mr-1 animate-spin" :size="13" />
        {{ displayBadge }}
      </DocMindBadge>
    </div>

    <div class="h-1 rounded-full bg-badge">
      <div
        class="h-1 rounded-full bg-accent transition-[width] duration-500"
        :style="{ width: `${Math.max(props.task.progress, 8)}%` }"
      />
    </div>

    <div v-if="props.task.warning" class="mb-2 rounded-md border border-amber-soft bg-amber-soft px-3 py-2 text-xs text-warning">
      {{ props.task.warning }}
    </div>

    <div class="mt-2.5 grid gap-2 md:grid-cols-2 xl:grid-cols-6">
      <div class="rounded-md bg-panel px-2.5 py-2">
        <div class="text-[11px] uppercase tracking-wide text-dim">{{ t("taskCard.currentDir") }}</div>
        <div class="mt-1 truncate text-sm font-medium text-primary">
          {{ props.task.current_dir || t("taskCard.noDir") }}
        </div>
      </div>
      <div class="rounded-md bg-panel px-2.5 py-2">
        <div class="text-[11px] uppercase tracking-wide text-dim">{{ t("taskCard.currentFile") }}</div>
        <div class="mt-1 truncate text-sm font-medium text-primary">
          {{ props.task.current_file || t("taskCard.noFile") }}
        </div>
      </div>
      <div class="rounded-md bg-panel px-2.5 py-2">
        <div class="text-[11px] uppercase tracking-wide text-dim">{{ t("taskCard.cumulativeSuccess") }}</div>
        <div class="mt-1 text-sm font-semibold text-success">{{ props.task.succeeded }}</div>
      </div>
      <div class="rounded-md bg-panel px-2.5 py-2">
        <div class="text-[11px] uppercase tracking-wide text-dim">{{ t("taskCard.cumulativeFailed") }}</div>
        <div class="mt-1 text-sm font-semibold text-danger">{{ props.task.failed }}</div>
      </div>
      <div class="rounded-md bg-panel px-2.5 py-2">
        <div class="text-[11px] uppercase tracking-wide text-dim">{{ t("taskCard.thisUpdate") }}</div>
        <div class="mt-1 text-sm font-semibold text-primary">{{ props.task.updated }}</div>
      </div>
      <div class="rounded-md bg-panel px-2.5 py-2">
        <div class="text-[11px] uppercase tracking-wide text-dim">{{ t("taskCard.thisSkipped") }}</div>
        <div class="mt-1 text-sm font-semibold text-primary">{{ props.task.skipped }}</div>
      </div>
    </div>

    <div class="mt-2 grid gap-2 md:grid-cols-2 xl:grid-cols-3">
      <div class="rounded-md bg-panel px-2.5 py-2">
        <div class="text-[11px] uppercase tracking-wide text-dim">{{ t("taskCard.thisDeleted") }}</div>
        <div class="mt-1 text-sm font-semibold text-primary">{{ props.task.deleted }}</div>
      </div>
      <div class="rounded-md bg-panel px-2.5 py-2">
        <div class="text-[11px] uppercase tracking-wide text-dim">{{ t("taskCard.processed") }}</div>
        <div class="mt-1 text-sm font-semibold text-primary">
          {{ props.task.scanned }} / {{ props.task.total }}
        </div>
      </div>
      <div class="rounded-md bg-panel px-2.5 py-2">
        <div class="text-[11px] uppercase tracking-wide text-dim">{{ t("taskCard.queueRemaining") }}</div>
        <div class="mt-1 text-sm font-semibold text-primary">
          {{ remainingCount }}
        </div>
      </div>
    </div>

    <div class="mt-2 flex flex-wrap items-center gap-2 text-[11px] text-dim">
      <span>{{ props.task.scanned }} / {{ props.task.total }} {{ t("taskCard.files", { count: "" }) }}</span>
      <span>·</span>
      <span>{{ props.task.progress }}%</span>
      <span>·</span>
      <span>{{ props.task.details }}</span>
    </div>
  </div>
</template>
