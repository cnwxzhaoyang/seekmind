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
};

const savedSettings = ref<IndexSettingsView | null>(null);
const excludeDirsText = ref("");
const excludeExtsText = ref("");
const maxFileSizeMb = ref(50);
const semanticSearchEnabled = ref(true);
const semanticWeight = ref(0.25);
const semanticThreshold = ref(0.2);
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
    Number(semanticThreshold.value) !== savedSettings.value.semantic_threshold
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
  <div class="h-full overflow-y-auto p-8">
    <div class="mb-7 flex items-center justify-between gap-4">
      <div>
        <h1 class="text-2xl font-semibold tracking-tight text-slate-950">{{ t("page.settings.title") }}</h1>
        <p class="mt-1 text-sm text-slate-500">{{ t("page.settings.subtitle") }}</p>
      </div>

      <div class="flex flex-wrap items-center gap-2">
        <button
          class="inline-flex items-center gap-2 rounded-2xl border border-slate-200 bg-white px-4 py-2.5 text-sm font-medium text-slate-700 shadow-sm hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
          :disabled="loading || saving"
          @click="resetToDefaults"
        >
          <RefreshCw :size="16" />
          {{ t("page.settings.btn.reset") }}
        </button>
        <button
          class="inline-flex items-center gap-2 rounded-2xl bg-slate-900 px-4 py-2.5 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-70"
          :disabled="loading || saving"
          @click="saveSettings"
        >
          <Save :size="16" />
          {{ saving ? t("page.settings.btn.saving") : t("page.settings.btn.save") }}
        </button>
      </div>
    </div>

    <div v-if="errorMessage" class="mb-4 rounded-2xl border border-red-100 bg-red-50 px-4 py-3 text-sm text-red-700">
      {{ errorMessage }}
    </div>

    <div v-if="infoMessage" class="mb-4 rounded-2xl border border-emerald-100 bg-emerald-50 px-4 py-3 text-sm text-emerald-700">
      {{ infoMessage }}
    </div>

    <div v-if="loading" class="rounded-3xl border border-dashed border-slate-300 bg-white p-6 text-sm text-slate-500">
      {{ t("page.settings.loading") }}
    </div>

    <div v-else class="grid gap-5 xl:grid-cols-[minmax(0,1.3fr)_minmax(0,0.7fr)]">
      <section class="rounded-3xl border border-slate-200 bg-white p-6 shadow-sm">
        <div class="mb-5 flex items-center justify-between">
          <div>
            <div class="text-sm font-semibold text-slate-900">{{ t("page.settings.section.rules") }}</div>
            <div class="mt-1 text-xs text-slate-500">{{ t("page.settings.rulesDesc") }}</div>
          </div>
          <DocMindBadge tone="default">{{ t("status.localEffective") }}</DocMindBadge>
        </div>

        <div class="space-y-4">
          <label class="block">
            <div class="mb-2 text-sm font-medium text-slate-700">{{ t("page.settings.label.excludeDirs") }}</div>
            <textarea
              v-model="excludeDirsText"
              rows="5"
              class="w-full rounded-2xl border border-slate-200 bg-slate-50 px-4 py-3 text-sm text-slate-900 outline-none transition focus:border-slate-400 focus:bg-white"
              :placeholder="t('page.settings.placeholder.dirs')"
            />
          </label>

          <label class="block">
            <div class="mb-2 text-sm font-medium text-slate-700">{{ t("page.settings.label.excludeExts") }}</div>
            <textarea
              v-model="excludeExtsText"
              rows="4"
              class="w-full rounded-2xl border border-slate-200 bg-slate-50 px-4 py-3 text-sm text-slate-900 outline-none transition focus:border-slate-400 focus:bg-white"
              :placeholder="t('page.settings.placeholder.exts')"
            />
          </label>

          <div class="grid gap-4 md:grid-cols-2">
            <label class="block">
              <div class="mb-2 text-sm font-medium text-slate-700">{{ t("page.settings.label.maxFileSize") }}</div>
              <input
                v-model.number="maxFileSizeMb"
                type="number"
                min="1"
                step="1"
                class="w-full rounded-2xl border border-slate-200 bg-slate-50 px-4 py-3 text-sm text-slate-900 outline-none transition focus:border-slate-400 focus:bg-white"
              />
            </label>

            <div class="rounded-2xl bg-slate-50 px-4 py-3">
              <div class="text-xs text-slate-500">{{ t("page.settings.label.currentStatus") }}</div>
              <div class="mt-2 text-sm font-medium text-slate-900">
                {{ hasChanges ? t("page.settings.status.changed") : t("page.settings.status.synced") }}
              </div>
            </div>
          </div>

          <div class="rounded-3xl border border-slate-200 bg-slate-50 p-4">
            <div class="flex items-start justify-between gap-4">
              <div>
                <div class="text-sm font-medium text-slate-900">{{ t("page.settings.semantic.title") }}</div>
                <div class="mt-1 text-xs text-slate-500">{{ t("page.settings.semantic.desc") }}</div>
              </div>
              <label class="inline-flex items-center gap-2 text-sm text-slate-700">
                <input v-model="semanticSearchEnabled" type="checkbox" class="h-4 w-4 rounded border-slate-300 text-slate-900" />
                {{ semanticSearchEnabled ? t("page.settings.semantic.enabled") : t("page.settings.semantic.disabled") }}
              </label>
            </div>

            <div class="mt-4">
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

            <div class="mt-4">
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
              <div class="mt-2 text-xs text-slate-500">
                {{ t("page.settings.semantic.thresholdDesc") }}
              </div>
            </div>
          </div>
        </div>
      </section>

      <aside class="space-y-5">
        <section class="rounded-3xl border border-slate-200 bg-white p-6 shadow-sm">
          <div class="mb-4 flex items-center justify-between">
            <div>
              <div class="text-sm font-semibold text-slate-900">{{ t("page.settings.section.instructions") }}</div>
              <div class="mt-1 text-xs text-slate-500">{{ t("page.settings.instructions.effective") }}</div>
            </div>
            <DocMindBadge tone="success">{{ t("status.savedLocally") }}</DocMindBadge>
          </div>

          <div class="space-y-3 text-sm text-slate-600">
            <p>• {{ t("page.settings.instructions.dirs") }}</p>
            <p>• {{ t("page.settings.instructions.exts") }}</p>
            <p>• {{ t("page.settings.instructions.maxSize") }}</p>
          </div>
        </section>

        <section class="rounded-3xl border border-amber-200 bg-amber-50 p-6 shadow-sm">
          <div class="mb-3 flex items-center justify-between">
            <div>
              <div class="text-sm font-semibold text-amber-950">{{ t("page.settings.section.danger") }}</div>
              <div class="mt-1 text-xs text-amber-800">{{ t("page.settings.danger.desc") }}</div>
            </div>
            <Trash2 class="text-amber-700" :size="18" />
          </div>
          <button
            class="inline-flex w-full items-center justify-center gap-2 rounded-2xl bg-amber-600 px-4 py-3 text-sm font-medium text-white shadow-sm hover:bg-amber-700 disabled:cursor-not-allowed disabled:opacity-70"
            :disabled="clearing"
            @click="clearAllIndexes"
          >
            <RefreshCw :size="16" :class="{ 'animate-spin': clearing }" />
            {{ clearing ? t("page.settings.btn.clearing") : t("page.settings.btn.clear") }}
          </button>
          <div class="mt-3 text-xs leading-5 text-amber-900/80">
            {{ t("page.settings.danger.detail") }}
          </div>
        </section>

        <section class="rounded-3xl border border-slate-200 bg-white p-6 shadow-sm">
          <div class="mb-4 flex items-center justify-between">
            <div>
              <div class="text-sm font-semibold text-slate-900">{{ t("page.settings.language") }}</div>
            </div>
            <Languages :size="18" class="text-slate-400" />
          </div>
          <div class="flex gap-2">
            <button
              class="flex-1 rounded-2xl border px-4 py-2.5 text-sm font-medium transition"
              :class="currentLocale === 'zh-CN'
                ? 'border-slate-900 bg-slate-900 text-white'
                : 'border-slate-200 bg-white text-slate-700 hover:bg-slate-50'"
              @click="switchLocale('zh-CN')"
            >
              中文
            </button>
            <button
              class="flex-1 rounded-2xl border px-4 py-2.5 text-sm font-medium transition"
              :class="currentLocale === 'en'
                ? 'border-slate-900 bg-slate-900 text-white'
                : 'border-slate-200 bg-white text-slate-700 hover:bg-slate-50'"
              @click="switchLocale('en')"
            >
              English
            </button>
          </div>
        </section>
        <DocMindSemanticPanel />
      </aside>
    </div>
  </div>
</template>
