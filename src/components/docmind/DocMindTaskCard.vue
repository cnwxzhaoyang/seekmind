<script setup lang="ts">
import { computed } from "vue";
import { Loader2 } from "lucide-vue-next";
import DocMindBadge from "./DocMindBadge.vue";
import type { CurrentTaskView } from "../../types/docmind";

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
    title: "当前任务",
    description: "",
    badgeLabel: "运行中",
    badgeTone: "warning",
    badgeSpinning: false,
  },
);

const remainingCount = computed(() => {
  if (!props.task) {
    return 0;
  }
  return Math.max(props.task.total - props.task.scanned, 0);
});
</script>

<template>
  <div v-if="task" class="rounded-3xl border border-slate-200 bg-white p-6 shadow-sm">
    <div class="mb-4 flex items-center justify-between">
      <div>
        <div class="text-sm font-semibold text-slate-900">{{ title }}</div>
        <div class="mt-1 text-xs text-slate-500">{{ description || task.details || "正在同步任务状态" }}</div>
      </div>
      <DocMindBadge :tone="badgeTone">
        <Loader2 v-if="badgeSpinning" class="mr-1 animate-spin" :size="13" />
        {{ badgeLabel }}
      </DocMindBadge>
    </div>

    <div class="h-2 rounded-full bg-slate-100">
      <div
        class="h-2 rounded-full bg-slate-900 transition-[width] duration-500"
        :style="{ width: `${Math.max(task.progress, 8)}%` }"
      />
    </div>

    <div class="mt-4 grid gap-3 md:grid-cols-2 xl:grid-cols-6">
      <div class="rounded-2xl bg-slate-50 px-4 py-3">
        <div class="text-[11px] uppercase tracking-wide text-slate-500">当前目录</div>
        <div class="mt-1 truncate text-sm font-medium text-slate-900">
          {{ task.current_dir || "暂无目录" }}
        </div>
      </div>
      <div class="rounded-2xl bg-slate-50 px-4 py-3">
        <div class="text-[11px] uppercase tracking-wide text-slate-500">当前文件</div>
        <div class="mt-1 truncate text-sm font-medium text-slate-900">
          {{ task.current_file || "暂无文件" }}
        </div>
      </div>
      <div class="rounded-2xl bg-slate-50 px-4 py-3">
        <div class="text-[11px] uppercase tracking-wide text-slate-500">累计成功</div>
        <div class="mt-1 text-sm font-semibold text-emerald-700">{{ task.succeeded }}</div>
      </div>
      <div class="rounded-2xl bg-slate-50 px-4 py-3">
        <div class="text-[11px] uppercase tracking-wide text-slate-500">累计失败</div>
        <div class="mt-1 text-sm font-semibold text-rose-700">{{ task.failed }}</div>
      </div>
      <div class="rounded-2xl bg-slate-50 px-4 py-3">
        <div class="text-[11px] uppercase tracking-wide text-slate-500">本次更新</div>
        <div class="mt-1 text-sm font-semibold text-slate-900">{{ task.updated }}</div>
      </div>
      <div class="rounded-2xl bg-slate-50 px-4 py-3">
        <div class="text-[11px] uppercase tracking-wide text-slate-500">本次跳过</div>
        <div class="mt-1 text-sm font-semibold text-slate-900">{{ task.skipped }}</div>
      </div>
    </div>

    <div class="mt-4 grid gap-3 md:grid-cols-2 xl:grid-cols-3">
      <div class="rounded-2xl bg-slate-50 px-4 py-3">
        <div class="text-[11px] uppercase tracking-wide text-slate-500">本次删除</div>
        <div class="mt-1 text-sm font-semibold text-slate-900">{{ task.deleted }}</div>
      </div>
      <div class="rounded-2xl bg-slate-50 px-4 py-3">
        <div class="text-[11px] uppercase tracking-wide text-slate-500">已处理</div>
        <div class="mt-1 text-sm font-semibold text-slate-900">
          {{ task.scanned }} / {{ task.total }}
        </div>
      </div>
      <div class="rounded-2xl bg-slate-50 px-4 py-3">
        <div class="text-[11px] uppercase tracking-wide text-slate-500">队列剩余</div>
        <div class="mt-1 text-sm font-semibold text-slate-900">
          {{ remainingCount }}
        </div>
      </div>
    </div>

    <div class="mt-4 flex flex-wrap items-center gap-3 text-xs text-slate-500">
      <span>{{ task.scanned }} / {{ task.total }} 个文件</span>
      <span>·</span>
      <span>{{ task.progress }}%</span>
      <span>·</span>
      <span>{{ task.details }}</span>
    </div>
  </div>
</template>
