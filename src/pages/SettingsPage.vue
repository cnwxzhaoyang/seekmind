<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { Languages, RefreshCw, Save, Trash2 } from "lucide-vue-next";
import DocMindBadge from "../components/docmind/DocMindBadge.vue";
import DocMindSemanticPanel from "../components/docmind/DocMindSemanticPanel.vue";
import { docmindApi, formatDocmindError } from "../services/docmindApi";
import { setLocale as setI18nLocale } from "../i18n";
import type { IndexSettingsView } from "../types/docmind";

const { t, locale } = useI18n();

const currentLocale = ref(locale.value);
const switchLocale = (lang: "zh-CN" | "en") => {
  currentLocale.value = lang;
  setI18nLocale(lang);
};

const factoryDefaultSettings: IndexSettingsView = {
  exclude_dirs: ["node_modules", ".git", "target", "Library", "Caches", "Application Support"],
  exclude_exts: [],
  max_file_size_mb: 50,
  semantic_search_enabled: true,
  semantic_weight: 0.25,
  semantic_threshold: 0.2,
  title_weight: 1.0,
  filename_weight: 1.0,
  preference_weight: 1.0,
  prefer_favorites_enabled: true,
  prefer_recent_enabled: true,
  prefer_history_enabled: true,
};

const savedSettings = ref<IndexSettingsView | null>(null);
const excludeDirsText = ref("");
const excludeExtsText = ref("");
const maxFileSizeMb = ref(50);
const semanticSearchEnabled = ref(true);
const semanticWeight = ref(0.25);
const semanticThreshold = ref(0.2);
const titleWeight = ref(1.0);
const filenameWeight = ref(1.0);
const preferenceWeight = ref(1.0);
const preferFavoritesEnabled = ref(true);
const preferRecentEnabled = ref(true);
const preferHistoryEnabled = ref(true);
const loading = ref(false);
const saving = ref(false);
const clearing = ref(false);
const errorMessage = ref("");
const infoMessage = ref("");

const hasChanges = computed(() => {
  if (!savedSettings.value) {
    return false;
  }

  const normalize = (input: string) =>
    input
      .split(/[\n,;]+/)
      .map((item) => item.trim())
      .filter(Boolean)
      .join(",");

  return (
    normalize(excludeDirsText.value) !== savedSettings.value.exclude_dirs.join(",") ||
    normalize(excludeExtsText.value) !== savedSettings.value.exclude_exts.join(",") ||
    Number(maxFileSizeMb.value) !== savedSettings.value.max_file_size_mb ||
    Boolean(semanticSearchEnabled.value) !== savedSettings.value.semantic_search_enabled ||
    Number(semanticWeight.value) !== savedSettings.value.semantic_weight ||
    Number(semanticThreshold.value) !== savedSettings.value.semantic_threshold ||
    Number(titleWeight.value) !== savedSettings.value.title_weight ||
    Number(filenameWeight.value) !== savedSettings.value.filename_weight ||
    Number(preferenceWeight.value) !== savedSettings.value.preference_weight ||
    Boolean(preferFavoritesEnabled.value) !== savedSettings.value.prefer_favorites_enabled ||
    Boolean(preferRecentEnabled.value) !== savedSettings.value.prefer_recent_enabled ||
    Boolean(preferHistoryEnabled.value) !== savedSettings.value.prefer_history_enabled
  );
});

const parseList = (value: string) =>
  Array.from(
    new Set(
      value
        .split(/[\n,;]+/)
        .map((item) => item.trim())
        .filter(Boolean)
        .map((item) => item.replace(/^\./, "")),
    ),
  );

const applySettings = (settings: IndexSettingsView) => {
  excludeDirsText.value = settings.exclude_dirs.join(", ");
  excludeExtsText.value = settings.exclude_exts.join(", ");
  maxFileSizeMb.value = settings.max_file_size_mb;
  semanticSearchEnabled.value = settings.semantic_search_enabled;
  semanticWeight.value = settings.semantic_weight;
  semanticThreshold.value = settings.semantic_threshold;
  titleWeight.value = settings.title_weight;
  filenameWeight.value = settings.filename_weight;
  preferenceWeight.value = settings.preference_weight;
  preferFavoritesEnabled.value = settings.prefer_favorites_enabled;
  preferRecentEnabled.value = settings.prefer_recent_enabled;
  preferHistoryEnabled.value = settings.prefer_history_enabled;
};

const loadSettings = async () => {
  loading.value = true;
  errorMessage.value = "";

  try {
    const settings = await docmindApi.getIndexSettings();
    savedSettings.value = settings;
    applySettings(settings);
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.settings.error.load"));
    console.error("[DocMind] getIndexSettings failed", error);
  } finally {
    loading.value = false;
  }
};

const saveSettings = async () => {
  saving.value = true;
  errorMessage.value = "";
  infoMessage.value = "";

  const payload: IndexSettingsView = {
    exclude_dirs: parseList(excludeDirsText.value),
    exclude_exts: parseList(excludeExtsText.value),
    max_file_size_mb: Math.max(1, Math.floor(Number(maxFileSizeMb.value) || 50)),
    semantic_search_enabled: semanticSearchEnabled.value,
    semantic_weight: Math.max(0, Math.min(1, Number(semanticWeight.value) || 0.25)),
    semantic_threshold: Math.max(0, Math.min(1, Number(semanticThreshold.value) || 0.2)),
    title_weight: Math.max(0, Math.min(3, Number(titleWeight.value) || 1)),
    filename_weight: Math.max(0, Math.min(3, Number(filenameWeight.value) || 1)),
    preference_weight: Math.max(0, Math.min(3, Number(preferenceWeight.value) || 1)),
    prefer_favorites_enabled: preferFavoritesEnabled.value,
    prefer_recent_enabled: preferRecentEnabled.value,
    prefer_history_enabled: preferHistoryEnabled.value,
  };

  try {
    await docmindApi.saveIndexSettings(payload);
    infoMessage.value = t("page.settings.saved");
    savedSettings.value = payload;
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.settings.error.save"));
    console.error("[DocMind] saveIndexSettings failed", error);
  } finally {
    saving.value = false;
  }
};

const resetToDefaults = () => {
  applySettings(factoryDefaultSettings);
  infoMessage.value = t("page.settings.resetDone");
};

const clearAllIndexes = async () => {
  if (!window.confirm(t("page.settings.confirmClear"))) {
    return;
  }

  clearing.value = true;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    await docmindApi.clearAllIndexes();
    infoMessage.value = t("page.settings.cleared");
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.settings.error.clear"));
    console.error("[DocMind] clearAllIndexes failed", error);
  } finally {
    clearing.value = false;
  }
};

onMounted(loadSettings);
</script>

<template>
  <div class="flex h-full min-h-0 flex-col bg-slate-50 text-slate-900">
    <header class="flex h-12 items-center justify-between gap-4 border-b border-slate-200 bg-white px-5">
      <div class="min-w-0">
        <h1 class="text-base font-semibold tracking-tight text-slate-950">{{ t("page.settings.title") }}</h1>
        <p class="mt-0.5 text-xs text-slate-500">{{ t("page.settings.subtitle") }}</p>
      </div>

      <div class="flex flex-wrap items-center gap-2">
        <button
          class="inline-flex items-center gap-2 rounded-md border border-slate-200 bg-white px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
          :disabled="loading || saving"
          @click="resetToDefaults"
        >
          <RefreshCw :size="15" />
          {{ t("page.settings.btn.reset") }}
        </button>
        <button
          class="inline-flex items-center gap-2 rounded-md bg-slate-900 px-3 py-1.5 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-70"
          :disabled="loading || saving"
          @click="saveSettings"
        >
          <Save :size="15" />
          {{ saving ? t("page.settings.btn.saving") : t("page.settings.btn.save") }}
        </button>
      </div>
    </header>

    <main class="min-h-0 flex-1 overflow-y-auto p-4">
      <div v-if="errorMessage" class="mb-3 rounded-md border border-red-100 bg-red-50 px-4 py-2.5 text-xs text-red-700">
        {{ errorMessage }}
      </div>

      <div v-if="infoMessage" class="mb-3 rounded-md border border-emerald-100 bg-emerald-50 px-4 py-2.5 text-xs text-emerald-700">
        {{ infoMessage }}
      </div>

      <div v-if="loading" class="rounded-md border border-dashed border-slate-200 bg-white px-4 py-6 text-xs text-slate-400">
        {{ t("page.settings.loading") }}
      </div>

      <div v-else class="space-y-4">
        <section class="rounded-lg border border-slate-200 bg-white">
          <div class="flex items-center justify-between border-b border-slate-200 px-4 py-2.5">
            <div>
              <div class="text-[10px] font-semibold uppercase tracking-[0.16em] text-slate-500">{{ t("page.settings.section.rules") }}</div>
              <div class="mt-1 text-xs text-slate-500">{{ t("page.settings.rulesDesc") }}</div>
            </div>
            <DocMindBadge tone="default">{{ t("status.localEffective") }}</DocMindBadge>
          </div>

          <div class="space-y-4 p-4">
            <div class="grid gap-4 xl:grid-cols-[220px_minmax(0,1fr)] xl:items-start">
              <div>
                <div class="text-sm font-medium text-slate-900">{{ t("page.settings.label.excludeDirs") }}</div>
                <div class="mt-1 text-xs text-slate-500">{{ t("page.settings.placeholder.dirs") }}</div>
              </div>
              <textarea
                v-model="excludeDirsText"
                rows="4"
                class="w-full rounded-lg border border-slate-200 bg-slate-50 px-4 py-3 text-sm text-slate-900 outline-none transition focus:border-slate-400 focus:bg-white"
                :placeholder="t('page.settings.placeholder.dirs')"
              />
            </div>

            <div class="grid gap-4 xl:grid-cols-[220px_minmax(0,1fr)] xl:items-start">
              <div>
                <div class="text-sm font-medium text-slate-900">{{ t("page.settings.label.excludeExts") }}</div>
                <div class="mt-1 text-xs text-slate-500">{{ t("page.settings.placeholder.exts") }}</div>
              </div>
              <textarea
                v-model="excludeExtsText"
                rows="3"
                class="w-full rounded-lg border border-slate-200 bg-slate-50 px-4 py-3 text-sm text-slate-900 outline-none transition focus:border-slate-400 focus:bg-white"
                :placeholder="t('page.settings.placeholder.exts')"
              />
            </div>

            <div class="grid gap-4 xl:grid-cols-[220px_minmax(0,1fr)] xl:items-center">
              <div>
                <div class="text-sm font-medium text-slate-900">{{ t("page.settings.label.maxFileSize") }}</div>
                <div class="mt-1 text-xs text-slate-500">{{ t("page.settings.label.maxFileSizeHint") ?? t("page.settings.placeholder.maxFileSize") }}</div>
              </div>
              <div class="grid gap-3 md:grid-cols-[180px_minmax(0,1fr)] md:items-center">
                <input
                  v-model.number="maxFileSizeMb"
                  type="number"
                  min="1"
                  step="1"
                  class="w-full rounded-lg border border-slate-200 bg-slate-50 px-4 py-3 text-sm text-slate-900 outline-none transition focus:border-slate-400 focus:bg-white"
                />
                <div class="rounded-lg border border-slate-200 bg-slate-50 px-4 py-3 text-xs text-slate-500">
                  <div class="text-[10px] font-semibold uppercase tracking-[0.16em] text-slate-500">{{ t("page.settings.label.currentStatus") }}</div>
                  <div class="mt-1 text-sm font-medium text-slate-900">
                    {{ hasChanges ? t("page.settings.status.changed") : t("page.settings.status.synced") }}
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>

        <section class="rounded-lg border border-slate-200 bg-white">
          <div class="flex items-center justify-between border-b border-slate-200 px-4 py-2.5">
            <div>
              <div class="text-[10px] font-semibold uppercase tracking-[0.16em] text-slate-500">{{ t("page.settings.semantic.title") }}</div>
              <div class="mt-1 text-xs text-slate-500">{{ t("page.settings.semantic.desc") }}</div>
            </div>
            <DocMindBadge tone="success">{{ semanticSearchEnabled ? t("page.settings.semantic.enabled") : t("page.settings.semantic.disabled") }}</DocMindBadge>
          </div>

          <div class="space-y-4 p-4">
            <div class="grid gap-4 xl:grid-cols-[220px_minmax(0,1fr)] xl:items-center">
              <div>
                <div class="text-sm font-medium text-slate-900">{{ t("page.settings.semantic.title") }}</div>
                <div class="mt-1 text-xs text-slate-500">{{ t("page.settings.semantic.desc") }}</div>
              </div>
              <label class="inline-flex items-center justify-start gap-2 text-sm text-slate-700">
                <input v-model="semanticSearchEnabled" type="checkbox" class="h-4 w-4 rounded border-slate-300 text-slate-900" />
                {{ semanticSearchEnabled ? t("page.settings.semantic.enabled") : t("page.settings.semantic.disabled") }}
              </label>
            </div>

            <div class="grid gap-4 xl:grid-cols-[220px_minmax(0,1fr)] xl:items-center">
              <div>
                <div class="text-sm font-medium text-slate-900">{{ t("page.settings.semantic.weight") }}</div>
                <div class="mt-1 text-xs text-slate-500">{{ t("page.settings.semantic.thresholdDesc") }}</div>
              </div>
              <div class="rounded-lg border border-slate-200 bg-slate-50 px-4 py-3">
                <div class="mb-2 flex items-center justify-between text-xs text-slate-500">
                  <span>{{ t("page.settings.semantic.weight") }}</span>
                  <span>{{ Math.round(semanticWeight * 100) }}%</span>
                </div>
                <input
                  v-model.number="semanticWeight"
                  type="range"
                  min="0"
                  max="1"
                  step="0.05"
                  class="w-full accent-slate-900"
                />
              </div>
            </div>

            <div class="grid gap-4 xl:grid-cols-[220px_minmax(0,1fr)] xl:items-center">
              <div>
                <div class="text-sm font-medium text-slate-900">{{ t("page.settings.semantic.threshold") }}</div>
                <div class="mt-1 text-xs text-slate-500">{{ t("page.settings.semantic.thresholdDesc") }}</div>
              </div>
              <div class="rounded-lg border border-slate-200 bg-slate-50 px-4 py-3">
                <div class="mb-2 flex items-center justify-between text-xs text-slate-500">
                  <span>{{ t("page.settings.semantic.threshold") }}</span>
                  <span>{{ Math.round(semanticThreshold * 100) }}%</span>
                </div>
                <input
                  v-model.number="semanticThreshold"
                  type="range"
                  min="0"
                  max="1"
                  step="0.05"
                  class="w-full accent-slate-900"
                />
              </div>
            </div>

            <div class="grid gap-4 xl:grid-cols-[220px_minmax(0,1fr)] xl:items-start">
              <div>
                <div class="text-sm font-medium text-slate-900">{{ t("page.settings.preference.title") }}</div>
                <div class="mt-1 text-xs text-slate-500">{{ t("page.settings.preference.title") }}</div>
              </div>
              <div class="grid gap-2 rounded-lg border border-slate-200 bg-slate-50 px-4 py-3 text-sm text-slate-700">
                <label class="inline-flex items-center justify-between gap-3">
                  <span>{{ t("page.settings.preference.favorite") }}</span>
                  <input v-model="preferFavoritesEnabled" type="checkbox" class="h-4 w-4 rounded border-slate-300 text-slate-900" />
                </label>
                <label class="inline-flex items-center justify-between gap-3">
                  <span>{{ t("page.settings.preference.recent") }}</span>
                  <input v-model="preferRecentEnabled" type="checkbox" class="h-4 w-4 rounded border-slate-300 text-slate-900" />
                </label>
                <label class="inline-flex items-center justify-between gap-3">
                  <span>{{ t("page.settings.preference.history") }}</span>
                  <input v-model="preferHistoryEnabled" type="checkbox" class="h-4 w-4 rounded border-slate-300 text-slate-900" />
                </label>
              </div>
            </div>

            <div class="grid gap-4 xl:grid-cols-[220px_minmax(0,1fr)] xl:items-start">
              <div>
                <div class="text-sm font-medium text-slate-900">{{ t("page.settings.weight.title") }}</div>
                <div class="mt-1 text-xs text-slate-500">{{ t("page.settings.weight.title") }}</div>
              </div>
              <div class="space-y-4 rounded-lg border border-slate-200 bg-slate-50 px-4 py-3">
                <label class="block">
                  <div class="mb-2 flex items-center justify-between text-xs text-slate-500">
                    <span>{{ t("page.settings.weight.titleWeight") }}</span>
                    <span>{{ titleWeight.toFixed(2) }}</span>
                  </div>
                  <input v-model.number="titleWeight" type="range" min="0" max="3" step="0.1" class="w-full accent-slate-900" />
                </label>
                <label class="block">
                  <div class="mb-2 flex items-center justify-between text-xs text-slate-500">
                    <span>{{ t("page.settings.weight.filenameWeight") }}</span>
                    <span>{{ filenameWeight.toFixed(2) }}</span>
                  </div>
                  <input v-model.number="filenameWeight" type="range" min="0" max="3" step="0.1" class="w-full accent-slate-900" />
                </label>
                <label class="block">
                  <div class="mb-2 flex items-center justify-between text-xs text-slate-500">
                    <span>{{ t("page.settings.weight.preferenceWeight") }}</span>
                    <span>{{ preferenceWeight.toFixed(2) }}</span>
                  </div>
                  <input v-model.number="preferenceWeight" type="range" min="0" max="3" step="0.1" class="w-full accent-slate-900" />
                </label>
              </div>
            </div>
          </div>
        </section>

        <DocMindSemanticPanel />

        <section class="rounded-lg border border-slate-200 bg-white">
          <div class="flex items-center justify-between border-b border-slate-200 px-4 py-2.5">
            <div>
              <div class="text-[10px] font-semibold uppercase tracking-[0.16em] text-slate-500">{{ t("page.settings.section.instructions") }}</div>
              <div class="mt-1 text-xs text-slate-500">{{ t("page.settings.instructions.effective") }}</div>
            </div>
            <DocMindBadge tone="success">{{ t("status.savedLocally") }}</DocMindBadge>
          </div>
          <div class="space-y-2 p-4 text-sm text-slate-600">
            <p>• {{ t("page.settings.instructions.dirs") }}</p>
            <p>• {{ t("page.settings.instructions.exts") }}</p>
            <p>• {{ t("page.settings.instructions.maxSize") }}</p>
          </div>
        </section>

        <section class="rounded-lg border border-slate-200 bg-white">
          <div class="flex items-center justify-between border-b border-slate-200 px-4 py-2.5">
            <div>
              <div class="text-[10px] font-semibold uppercase tracking-[0.16em] text-slate-500">{{ t("page.settings.language") }}</div>
            </div>
            <Languages :size="15" class="text-slate-400" />
          </div>
          <div class="flex gap-2 p-4">
            <button
              class="flex-1 rounded-md border px-4 py-2.5 text-sm font-medium transition"
              :class="currentLocale === 'zh-CN'
                ? 'border-slate-900 bg-slate-900 text-white'
                : 'border-slate-200 bg-white text-slate-700 hover:bg-slate-50'"
              @click="switchLocale('zh-CN')"
            >
              中文
            </button>
            <button
              class="flex-1 rounded-md border px-4 py-2.5 text-sm font-medium transition"
              :class="currentLocale === 'en'
                ? 'border-slate-900 bg-slate-900 text-white'
                : 'border-slate-200 bg-white text-slate-700 hover:bg-slate-50'"
              @click="switchLocale('en')"
            >
              English
            </button>
          </div>
        </section>

        <section class="rounded-lg border border-red-100 bg-red-50 px-4 py-3">
          <div class="flex items-start justify-between gap-4">
            <div class="min-w-0">
              <div class="flex items-center gap-2 text-[10px] font-semibold uppercase tracking-[0.16em] text-red-600">
                <Trash2 :size="15" />
                {{ t("page.settings.section.danger") }}
              </div>
              <div class="mt-1 text-xs text-red-500">{{ t("page.settings.danger.desc") }}</div>
              <div class="mt-2 max-w-3xl text-xs leading-5 text-red-700/80">
                {{ t("page.settings.danger.detail") }}
              </div>
            </div>
            <button
              class="inline-flex shrink-0 items-center gap-2 rounded-md bg-red-500 px-4 py-2 text-sm font-medium text-white hover:bg-red-600 disabled:cursor-not-allowed disabled:opacity-70"
              :disabled="clearing"
              @click="clearAllIndexes"
            >
              <RefreshCw :size="15" :class="{ 'animate-spin': clearing }" />
              {{ clearing ? t("page.settings.btn.clearing") : t("page.settings.btn.clear") }}
            </button>
          </div>
        </section>
      </div>
    </main>
  </div>
</template>
