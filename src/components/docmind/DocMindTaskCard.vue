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
  <div v-if="props.task" class="rounded-3xl border border-slate-200 bg-white p-6 shadow-sm">
    <div class="mb-4 flex items-center justify-between">
      <div>
        <div class="text-sm font-semibold text-slate-900">{{ displayTitle }}</div>
        <div class="mt-1 text-xs text-slate-500">{{ displayDesc }}</div>
      </div>
      <DocMindBadge :tone="badgeTone">
        <Loader2 v-if="badgeSpinning" class="mr-1 animate-spin" :size="13" />
        {{ displayBadge }}
      </DocMindBadge>
    </div>

    <div class="h-2 rounded-full bg-slate-100">
      <div
        class="h-2 rounded-full bg-slate-900 transition-[width] duration-500"
        :style="{ width: `${Math.max(props.task.progress, 8)}%` }"
      />
    </div>

    <div class="mt-4 grid gap-3 md:grid-cols-2 xl:grid-cols-6">
      <div class="rounded-2xl bg-slate-50 px-4 py-3">
        <div class="text-[11px] uppercase tracking-wide text-slate-500">{{ t("taskCard.currentDir") }}</div>
        <div class="mt-1 truncate text-sm font-medium text-slate-900">
          {{ props.task.current_dir || t("taskCard.noDir") }}
        </div>
      </div>
      <div class="rounded-2xl bg-slate-50 px-4 py-3">
        <div class="text-[11px] uppercase tracking-wide text-slate-500">{{ t("taskCard.currentFile") }}</div>
        <div class="mt-1 truncate text-sm font-medium text-slate-900">
          {{ props.task.current_file || t("taskCard.noFile") }}
        </div>
      </div>
      <div class="rounded-2xl bg-slate-50 px-4 py-3">
        <div class="text-[11px] uppercase tracking-wide text-slate-500">{{ t("taskCard.cumulativeSuccess") }}</div>
        <div class="mt-1 text-sm font-semibold text-emerald-700">{{ props.task.succeeded }}</div>
      </div>
      <div class="rounded-2xl bg-slate-50 px-4 py-3">
        <div class="text-[11px] uppercase tracking-wide text-slate-500">{{ t("taskCard.cumulativeFailed") }}</div>
        <div class="mt-1 text-sm font-semibold text-rose-700">{{ props.task.failed }}</div>
      </div>
      <div class="rounded-2xl bg-slate-50 px-4 py-3">
        <div class="text-[11px] uppercase tracking-wide text-slate-500">{{ t("taskCard.thisUpdate") }}</div>
        <div class="mt-1 text-sm font-semibold text-slate-900">{{ props.task.updated }}</div>
      </div>
      <div class="rounded-2xl bg-slate-50 px-4 py-3">
        <div class="text-[11px] uppercase tracking-wide text-slate-500">{{ t("taskCard.thisSkipped") }}</div>
        <div class="mt-1 text-sm font-semibold text-slate-900">{{ props.task.skipped }}</div>
      </div>
    </div>

    <div class="mt-4 grid gap-3 md:grid-cols-2 xl:grid-cols-3">
      <div class="rounded-2xl bg-slate-50 px-4 py-3">
        <div class="text-[11px] uppercase tracking-wide text-slate-500">{{ t("taskCard.thisDeleted") }}</div>
        <div class="mt-1 text-sm font-semibold text-slate-900">{{ props.task.deleted }}</div>
      </div>
      <div class="rounded-2xl bg-slate-50 px-4 py-3">
        <div class="text-[11px] uppercase tracking-wide text-slate-500">{{ t("taskCard.processed") }}</div>
        <div class="mt-1 text-sm font-semibold text-slate-900">
          {{ props.task.scanned }} / {{ props.task.total }}
        </div>
      </div>
      <div class="rounded-2xl bg-slate-50 px-4 py-3">
        <div class="text-[11px] uppercase tracking-wide text-slate-500">{{ t("taskCard.queueRemaining") }}</div>
        <div class="mt-1 text-sm font-semibold text-slate-900">
          {{ remainingCount }}
        </div>
      </div>
    </div>

    <div class="mt-4 flex flex-wrap items-center gap-3 text-xs text-slate-500">
      <span>{{ props.task.scanned }} / {{ props.task.total }} {{ t("taskCard.files", { count: "" }) }}</span>
      <span>·</span>
      <span>{{ props.task.progress }}%</span>
      <span>·</span>
      <span>{{ props.task.details }}</span>
    </div>
  </div>
</template>
