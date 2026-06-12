/**
 * @author MorningSun
 * @CreatedDate 2026/06/09
 * @Description SeekMind 首次启动引导条，提示用户先添加索引目录再开始搜索与问答。
 */
<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import { seekMindApi } from "../../services/seekMindApi";
import SeekMindIcon from "./SeekMindIcon.vue";

const STORAGE_KEY = "seekmind.onboarding.dismissed.v1";

const { t } = useI18n();
const router = useRouter();

const dismissed = ref(loadDismissed());
const hasIndexDirs = ref(true);
const forceFirstLaunch = ref(false);

function loadDismissed() {
  try {
    return localStorage.getItem(STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

function saveDismissed() {
  try {
    localStorage.setItem(STORAGE_KEY, "1");
  } catch {
    // ignore storage errors
  }
}

const showGuide = computed(() => {
  // 修复：用户手动关闭后必须优先生效，不能被“强制首次启动”标记覆盖。
  if (dismissed.value) return false;
  return forceFirstLaunch.value || !hasIndexDirs.value;
});

const steps = computed(() => [
  { icon: "icon-folder", title: t("onboarding.step1Title"), desc: t("onboarding.step1Desc") },
  { icon: "icon-search", title: t("onboarding.step2Title"), desc: t("onboarding.step2Desc") },
  { icon: "icon-qa", title: t("onboarding.step3Title"), desc: t("onboarding.step3Desc") },
]);

const loadFirstLaunchState = async () => {
  try {
    const [dirs, runtime] = await Promise.all([
      seekMindApi.listIndexDirs(),
      seekMindApi.getAppRuntimeInfo(),
    ]);
    hasIndexDirs.value = dirs.length > 0;
    forceFirstLaunch.value = runtime.force_first_launch;
    if (showGuide.value) {
      console.info("[SeekMind] first launch guide shown");
    }
  } catch (error) {
    console.warn("[SeekMind] failed to load onboarding state", error);
    hasIndexDirs.value = true;
    forceFirstLaunch.value = false;
  }
};

const dismissGuide = () => {
  dismissed.value = true;
  saveDismissed();
  console.info("[SeekMind] first launch guide dismissed");
};

const goToStatus = async () => {
  await router.push("/status");
  dismissGuide();
};

onMounted(() => {
  void loadFirstLaunchState();
});
</script>

<template>
  <div
    v-if="showGuide"
    class="mx-3 mb-3 rounded-[18px] border border-accent/20 bg-surface/90 px-4 py-3 shadow-card"
  >
    <div class="flex items-start gap-3">
      <div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-[14px] bg-accent-soft text-accent">
        <SeekMindIcon icon="icon-folder" :size="19" />
      </div>
      <div class="min-w-0 flex-1">
        <div class="flex items-start justify-between gap-3">
          <div class="min-w-0">
            <div class="text-[14px] font-semibold leading-5 text-primary">{{ t("onboarding.title") }}</div>
            <div class="mt-1 text-[12px] leading-5 text-secondary">{{ t("onboarding.desc") }}</div>
          </div>
          <button
            class="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted transition hover:bg-surface-hover hover:text-primary"
            :aria-label="t('onboarding.close')"
            :title="t('onboarding.close')"
            @click="dismissGuide"
          >
            <SeekMindIcon icon="icon-close" :size="15" />
          </button>
        </div>

        <div class="mt-3 grid gap-2 md:grid-cols-3">
          <div
            v-for="(step, index) in steps"
            :key="step.title"
            class="flex min-h-[68px] items-start gap-2 rounded-[14px] border border-default bg-panel/70 px-3 py-2"
          >
            <div class="flex h-8 w-8 shrink-0 items-center justify-center rounded-[12px] bg-surface text-accent">
              <SeekMindIcon :icon="step.icon" :size="15" />
            </div>
            <div class="min-w-0">
              <div class="text-[11px] font-medium leading-4 text-muted">{{ t("onboarding.stepLabel", { index: index + 1 }) }}</div>
              <div class="truncate text-[12px] font-medium leading-5 text-primary">{{ step.title }}</div>
              <div class="mt-0.5 text-[11px] leading-4 text-muted">{{ step.desc }}</div>
            </div>
          </div>
        </div>

        <div class="mt-3 flex flex-wrap items-center gap-2">
          <button
            class="inline-flex h-8 items-center justify-center rounded-[12px] bg-accent px-3 text-[12px] font-medium text-white transition hover:opacity-90"
            @click="goToStatus"
          >
            {{ t("onboarding.goStatus") }}
          </button>
          <button
            class="inline-flex h-8 items-center justify-center rounded-[12px] border border-default bg-surface px-3 text-[12px] font-medium text-secondary transition hover:bg-surface-hover hover:text-primary"
            @click="dismissGuide"
          >
            {{ t("onboarding.dismiss") }}
          </button>
          <div class="text-[11px] text-muted">{{ t("onboarding.tip") }}</div>
        </div>
      </div>
    </div>
  </div>
</template>
