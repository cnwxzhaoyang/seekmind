<script setup lang="ts">
/**
 * @author MorningSun
 * @CreatedDate 2026/06/10
 * @Description 设置页自动更新面板，支持更新源配置、手动检查和启动时自动检查。
 */
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { AlertTriangle, Download, RefreshCw, ShieldCheck } from "lucide-vue-next";
import { openUrl } from "@tauri-apps/plugin-opener";
import SeekMindBadge from "./SeekMindBadge.vue";
import SeekMindToast from "./SeekMindToast.vue";
import { formatSeekMindError, seekMindApi } from "../../services/seekMindApi";
import type { AppRuntimeInfoView, UpdateCheckView } from "../../types/SeekMind";

const { t } = useI18n();
const STORAGE_KEY = "seekmind.update.manifest-url.v1";
const AUTO_CHECK_KEY = "seekmind.update.auto-check.v1";

const runtimeInfo = ref<AppRuntimeInfoView | null>(null);
const manifestUrl = ref(localStorage.getItem(STORAGE_KEY) ?? "");
const autoCheckEnabled = ref(localStorage.getItem(AUTO_CHECK_KEY) === "1");
const checking = ref(false);
const errorMessage = ref("");
const infoMessage = ref("");
const updateResult = ref<UpdateCheckView | null>(null);

const effectiveManifestUrl = computed(() => manifestUrl.value.trim() || runtimeInfo.value?.update_manifest_url.trim() || "");
const currentVersion = computed(() => runtimeInfo.value?.app_version ?? "-");
const runtimeSummary = computed(() =>
  runtimeInfo.value
    ? `${runtimeInfo.value.build_mode} · ${runtimeInfo.value.target_os}/${runtimeInfo.value.target_arch}`
    : "-",
);

watch(
  [manifestUrl, autoCheckEnabled],
  () => {
    localStorage.setItem(STORAGE_KEY, manifestUrl.value);
    localStorage.setItem(AUTO_CHECK_KEY, autoCheckEnabled.value ? "1" : "0");
  },
  { deep: true },
);

const loadRuntimeInfo = async () => {
  try {
    runtimeInfo.value = await seekMindApi.getAppRuntimeInfo();
    if (!manifestUrl.value.trim() && runtimeInfo.value.update_manifest_url.trim()) {
      manifestUrl.value = runtimeInfo.value.update_manifest_url.trim();
    }
  } catch (error) {
    console.error("[SeekMind] getAppRuntimeInfo failed", error);
    errorMessage.value = formatSeekMindError(error, t("page.settings.update.error.loadRuntime"));
  }
};

const checkForUpdates = async () => {
  const url = effectiveManifestUrl.value;
  if (!url) {
    updateResult.value = {
      current_version: currentVersion.value,
      latest_version: null,
      release_name: null,
      release_notes: null,
      download_url: null,
      manifest_url: "",
      is_update_available: false,
      status: "disabled",
      message: t("page.settings.update.noSource"),
      target_os: runtimeInfo.value?.target_os ?? "-",
      target_arch: runtimeInfo.value?.target_arch ?? "-",
    };
    errorMessage.value = "";
    infoMessage.value = "";
    return;
  }

  checking.value = true;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    const payload = await seekMindApi.checkAppUpdate(url);
    updateResult.value = payload;
    infoMessage.value = payload.message;
    console.info("[SeekMind] update check finished", payload);
  } catch (error) {
    console.error("[SeekMind] checkAppUpdate failed", error);
    errorMessage.value = formatSeekMindError(error, t("page.settings.update.error.check"));
    updateResult.value = null;
  } finally {
    checking.value = false;
  }
};

const openDownload = async () => {
  const downloadUrl = updateResult.value?.download_url?.trim();
  if (!downloadUrl) {
    return;
  }

  try {
    await openUrl(downloadUrl);
  } catch (error) {
    console.error("[SeekMind] open update download url failed", error);
    errorMessage.value = formatSeekMindError(error, t("page.settings.update.error.openDownload"));
  }
};

onMounted(async () => {
  await loadRuntimeInfo();
  if (autoCheckEnabled.value) {
    await checkForUpdates();
  }
});
</script>

<template>
  <section id="settings-update" class="scroll-mt-4 rounded-lg border border-default bg-surface">
    <div class="settings-section-head">
      <div class="settings-section-head-left">
        <span class="settings-section-icon settings-section-icon--plain">
          <RefreshCw :size="18" />
        </span>
        <div class="min-w-0">
          <div class="settings-section-title">{{ t("page.settings.update.title") }}</div>
        </div>
      </div>
      <SeekMindBadge tone="default">{{ currentVersion }}</SeekMindBadge>
    </div>

    <div class="space-y-5 p-4">
      <SeekMindToast v-if="errorMessage" :message="errorMessage" tone="error" />
      <SeekMindToast v-if="infoMessage" :message="infoMessage" tone="success" />

      <div class="grid gap-4 xl:grid-cols-[160px_minmax(0,1fr)] xl:items-start">
        <div>
          <div class="seekmind-section-label">{{ t("page.settings.update.sourceLabel") }}</div>
          <div class="seekmind-item-meta mt-1">{{ t("page.settings.update.sourceHint") }}</div>
        </div>
        <div class="space-y-2">
          <input
            v-model="manifestUrl"
            type="text"
            class="w-full rounded-lg border border-default bg-input px-4 py-3 text-sm text-primary outline-none transition focus:border-[var(--color-text-dim)] focus:bg-surface"
            :placeholder="t('page.settings.update.sourcePlaceholder')"
          />
          <div class="text-xs text-dim">
            {{ runtimeInfo?.update_manifest_url?.trim() ? t("page.settings.update.runtimeSourceHint") : t("page.settings.update.manifestHint") }}
          </div>
        </div>
      </div>

      <div class="grid gap-4 xl:grid-cols-[160px_minmax(0,1fr)] xl:items-center">
        <div>
          <div class="seekmind-section-label">{{ t("page.settings.update.autoCheckLabel") }}</div>
          <div class="seekmind-item-meta mt-1">{{ t("page.settings.update.autoCheckHint") }}</div>
        </div>
        <label class="inline-flex items-center justify-start gap-2 text-sm text-secondary">
          <input
            v-model="autoCheckEnabled"
            type="checkbox"
            class="h-4 w-4 rounded border-default text-accent accent-accent"
          />
          {{ t("page.settings.update.autoCheckEnabled") }}
        </label>
      </div>

      <div class="flex items-center justify-end gap-2">
        <SeekMindBadge v-if="updateResult?.status === 'available'" tone="warning">
          {{ t("page.settings.update.available") }}
        </SeekMindBadge>
        <SeekMindBadge v-else-if="updateResult?.status === 'up_to_date'" tone="success">
          {{ t("page.settings.update.upToDate") }}
        </SeekMindBadge>
        <SeekMindBadge v-else-if="updateResult?.status === 'disabled'" tone="default">
          {{ t("page.settings.update.disabled") }}
        </SeekMindBadge>
        <button
          class="inline-flex items-center gap-2 rounded-md border border-default bg-surface px-4 py-2.5 text-sm font-medium text-secondary transition hover:bg-surface-hover disabled:cursor-not-allowed disabled:opacity-60"
          :disabled="checking"
          @click="checkForUpdates"
        >
          <RefreshCw :size="15" :class="{ 'animate-spin': checking }" />
          {{ checking ? t("page.settings.update.checking") : t("page.settings.update.check") }}
        </button>
      </div>

      <div
        class="grid gap-3 rounded-lg border border-default bg-panel px-4 py-4 text-sm text-secondary xl:grid-cols-2"
      >
        <div>
          <div class="seekmind-item-meta">{{ t("page.settings.update.currentVersion") }}</div>
          <div class="mt-1 text-base font-semibold text-primary">{{ currentVersion }}</div>
        </div>
        <div>
          <div class="seekmind-item-meta">{{ t("page.settings.update.runtimeInfo") }}</div>
          <div class="mt-1 text-base font-semibold text-primary">{{ runtimeSummary }}</div>
        </div>
        <div>
          <div class="seekmind-item-meta">{{ t("page.settings.update.latestVersion") }}</div>
          <div class="mt-1 text-base font-semibold text-primary">
            {{ updateResult?.latest_version ?? "—" }}
          </div>
        </div>
        <div>
          <div class="seekmind-item-meta">{{ t("page.settings.update.sourceStatus") }}</div>
          <div class="mt-1 truncate text-base font-semibold text-primary">
            {{ updateResult?.manifest_url || effectiveManifestUrl || "—" }}
          </div>
        </div>
      </div>

      <div
        v-if="updateResult?.release_notes"
        class="rounded-lg border border-default bg-panel px-4 py-3 text-sm text-secondary"
      >
        <div class="flex items-center gap-2 text-xs font-medium uppercase tracking-[0.12em] text-dim">
          <ShieldCheck :size="14" />
          {{ t("page.settings.update.notes") }}
        </div>
        <div class="mt-2 whitespace-pre-wrap leading-6 text-primary">
          {{ updateResult.release_notes }}
        </div>
      </div>

      <div v-if="updateResult?.download_url" class="flex items-center justify-end gap-2">
        <button
          class="inline-flex items-center gap-2 rounded-md bg-accent px-4 py-2.5 text-sm font-medium text-white transition hover:opacity-90"
          @click="openDownload"
        >
          <Download :size="15" />
          {{ t("page.settings.update.openDownload") }}
        </button>
      </div>

      <div
        v-if="updateResult && updateResult.status === 'disabled'"
        class="flex items-start gap-2 rounded-lg border border-default bg-panel px-4 py-3 text-xs text-dim"
      >
        <AlertTriangle :size="14" class="mt-0.5 shrink-0 text-warning" />
        <span>{{ t("page.settings.update.noSourceDetail") }}</span>
      </div>
    </div>
  </section>
</template>
