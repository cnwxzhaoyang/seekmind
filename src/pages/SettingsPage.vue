<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, nextTick } from "vue";
import { useI18n } from "vue-i18n";
import { Database, Globe, Languages, MessageSquareText, Moon, Monitor, RefreshCw, Save, Shield, SlidersHorizontal, Sparkles, Sun, Trash2 } from "lucide-vue-next";
import { useTheme } from "../composables/useTheme";
import DocMindBadge from "../components/docmind/DocMindBadge.vue";
import DocMindConfirmDialog from "../components/docmind/DocMindConfirmDialog.vue";
import DocMindQaPanel from "../components/docmind/DocMindQaPanel.vue";
import DocMindSemanticPanel from "../components/docmind/DocMindSemanticPanel.vue";
import { docmindApi, formatDocmindError } from "../services/docmindApi";
import { setLocale as setI18nLocale } from "../i18n";
import type { IndexSettingsView, NetworkProxySettingsView } from "../types/docmind";

const { t, locale } = useI18n();
const { themeMode, setTheme } = useTheme();

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
const networkProxyEnabled = ref(false);
const networkProxyUrl = ref("");
const savedNetworkProxySettings = ref<NetworkProxySettingsView | null>(null);
const networkLoading = ref(false);
const networkSaving = ref(false);
const networkErrorMessage = ref("");
const networkInfoMessage = ref("");
const loading = ref(false);
const saving = ref(false);
const clearing = ref(false);
const showClearConfirm = ref(false);
const errorMessage = ref("");
const infoMessage = ref("");
const activeSection = ref("settings-rules");
const mainScrollEl = ref<HTMLElement | null>(null);
let sectionObserver: IntersectionObserver | null = null;

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

const applyNetworkProxySettings = (settings: NetworkProxySettingsView) => {
  networkProxyEnabled.value = settings.enabled;
  networkProxyUrl.value = settings.proxy_url;
};

const hasNetworkProxyChanges = computed(() => {
  if (!savedNetworkProxySettings.value) {
    return false;
  }

  return (
    Boolean(networkProxyEnabled.value) !== savedNetworkProxySettings.value.enabled ||
    networkProxyUrl.value.trim() !== savedNetworkProxySettings.value.proxy_url.trim()
  );
});

const loadSettings = async () => {
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

const loadNetworkProxySettings = async () => {
  networkLoading.value = true;
  networkErrorMessage.value = "";

  try {
    const settings = await docmindApi.getNetworkProxySettings();
    savedNetworkProxySettings.value = settings;
    applyNetworkProxySettings(settings);
  } catch (error) {
    networkErrorMessage.value = formatDocmindError(error, t("page.settings.network.error.load"));
    console.error("[DocMind] getNetworkProxySettings failed", error);
  } finally {
    networkLoading.value = false;
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

const saveNetworkProxySettings = async () => {
  networkSaving.value = true;
  networkErrorMessage.value = "";
  networkInfoMessage.value = "";

  const payload: NetworkProxySettingsView = {
    enabled: networkProxyEnabled.value,
    proxy_url: networkProxyUrl.value.trim(),
    updated_at: savedNetworkProxySettings.value?.updated_at ?? "",
  };

  try {
    const settings = await docmindApi.saveNetworkProxySettings(payload);
    savedNetworkProxySettings.value = settings;
    applyNetworkProxySettings(settings);
    networkInfoMessage.value = t("page.settings.network.saved");
  } catch (error) {
    networkErrorMessage.value = formatDocmindError(error, t("page.settings.network.error.save"));
    console.error("[DocMind] saveNetworkProxySettings failed", error);
  } finally {
    networkSaving.value = false;
  }
};

const resetToDefaults = () => {
  applySettings(factoryDefaultSettings);
  infoMessage.value = t("page.settings.resetDone");
};

const clearAllIndexes = async () => {
  showClearConfirm.value = true;
};

const handleClearConfirm = async () => {
  showClearConfirm.value = false;
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

const handleClearCancel = () => {
  showClearConfirm.value = false;
};

const scrollToSection = (id: string) => {
  activeSection.value = id;
  document.getElementById(id)?.scrollIntoView({ behavior: "smooth", block: "start" });
};

const settingsNavItems = computed(() => [
  {
    id: "settings-rules",
    label: t("page.settings.section.rules"),
    hint: t("page.settings.rulesDesc"),
    icon: SlidersHorizontal,
  },
  {
    id: "settings-semantic",
    label: t("page.settings.semantic.title"),
    hint: t("page.settings.semantic.thresholdDesc"),
    icon: Sparkles,
  },
  {
    id: "settings-qa",
    label: t("page.settings.qa.title"),
    hint: t("page.settings.qa.desc"),
    icon: MessageSquareText,
  },
  {
    id: "settings-model",
    label: t("semantic.title"),
    hint: t("semantic.desc"),
    icon: Database,
  },
  {
    id: "settings-appearance",
    label: t("page.settings.section.appearance"),
    hint: `${t("page.settings.language")} / ${t("page.settings.theme")}`,
    icon: Languages,
  },
  {
    id: "settings-network",
    label: t("page.settings.network.title"),
    hint: t("page.settings.network.desc"),
    icon: Globe,
  },
  {
    id: "settings-danger",
    label: t("page.settings.section.danger"),
    hint: t("page.settings.btn.clear"),
    icon: Trash2,
    danger: true,
  },
] as const);

const syncActiveSection = () => {
  if (sectionObserver) {
    sectionObserver.disconnect();
    sectionObserver = null;
  }

  const root = mainScrollEl.value;
  if (!root) {
    return;
  }

  const sectionIds = settingsNavItems.value.map((item) => item.id);
  const sectionElements = sectionIds
    .map((id) => document.getElementById(id))
    .filter((element): element is HTMLElement => Boolean(element));

  if (!sectionElements.length) {
    return;
  }

  sectionObserver = new IntersectionObserver(
    (entries) => {
      const visibleEntries = entries.filter((entry) => entry.isIntersecting);
      if (!visibleEntries.length) {
        return;
      }

      const topMost = visibleEntries.reduce((current, entry) => {
        if (!current) {
          return entry;
        }
        return entry.boundingClientRect.top < current.boundingClientRect.top ? entry : current;
      });

      if (topMost.target instanceof HTMLElement) {
        activeSection.value = topMost.target.id;
      }
    },
    {
      root,
      threshold: 0.18,
      rootMargin: "-18% 0px -62% 0px",
    },
  );

  sectionElements.forEach((element) => sectionObserver?.observe(element));
};

onMounted(async () => {
  loading.value = true;
  try {
    await Promise.all([loadSettings(), loadNetworkProxySettings()]);
  } finally {
    loading.value = false;
  }
  await nextTick();
  syncActiveSection();
});

onBeforeUnmount(() => {
  sectionObserver?.disconnect();
  sectionObserver = null;
});
</script>

<template>
  <div class="flex h-full min-h-0 flex-col bg-page text-primary">
    <header class="flex h-12 items-center justify-between gap-4 border-b border-default bg-header px-5">
      <div class="min-w-0">
        <h1 class="text-base font-semibold tracking-tight text-primary">{{ t("page.settings.title") }}</h1>
        <p class="docmind-item-meta mt-0.5">{{ t("page.settings.subtitle") }}</p>
      </div>

      <div class="flex flex-wrap items-center gap-2">
        <button
          class="inline-flex items-center gap-2 rounded-md border border-default bg-surface px-3 py-1.5 text-sm font-medium text-secondary hover:bg-surface-hover disabled:cursor-not-allowed disabled:opacity-70"
          :disabled="loading || saving"
          @click="resetToDefaults"
        >
          <RefreshCw :size="15" />
          {{ t("page.settings.btn.reset") }}
        </button>
        <button
          class="inline-flex items-center gap-2 rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-70"
          :disabled="loading || saving"
          @click="saveSettings"
        >
          <Save :size="15" />
          {{ saving ? t("page.settings.btn.saving") : t("page.settings.btn.save") }}
        </button>
      </div>
    </header>

    <main ref="mainScrollEl" class="min-h-0 flex-1 overflow-y-auto p-4">
      <div v-if="errorMessage" class="mb-3 rounded-md border border-danger-soft bg-danger-soft px-4 py-2.5 text-xs text-danger">
        {{ errorMessage }}
      </div>

      <div v-if="infoMessage" class="mb-3 rounded-md border border-emerald-soft bg-emerald-soft px-4 py-2.5 text-xs text-success">
        {{ infoMessage }}
      </div>

      <div v-if="loading" class="rounded-md border border-dashed border-default bg-surface px-4 py-6 text-xs text-muted">
        {{ t("page.settings.loading") }}
      </div>

      <div v-else class="mx-auto grid w-full max-w-[1720px] gap-4 xl:grid-cols-[300px_minmax(0,1fr)]">
        <aside class="hidden min-h-0 min-w-0 self-start xl:sticky xl:top-4 xl:block">
          <section class="rounded-lg border border-default bg-surface">
            <div class="flex items-center justify-between border-b border-default px-4 py-2.5">
              <div>
                <div class="docmind-section-label">导航</div>
                <div class="docmind-item-meta mt-1">{{ t("page.settings.instructions.effective") }}</div>
              </div>
              <DocMindBadge tone="success">{{ t("status.savedLocally") }}</DocMindBadge>
            </div>
            <div class="space-y-2 p-4">
              <button
                v-for="item in settingsNavItems"
                :key="item.id"
                class="w-full rounded-lg border px-3 py-2 text-left transition"
                :class="activeSection === item.id
                  ? item.danger
                    ? 'border-danger-soft bg-danger-soft text-danger'
                    : 'border-accent bg-accent-soft text-primary'
                  : item.danger
                    ? 'border-danger-soft bg-danger-soft opacity-80 text-secondary hover:opacity-100'
                    : 'border-default bg-surface text-secondary hover:bg-surface-hover'"
                @click="scrollToSection(item.id)"
              >
                <div class="flex items-start gap-3">
                  <span
                    class="mt-0.5 inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-md border"
                    :class="activeSection === item.id
                      ? item.danger
                        ? 'border-danger-soft bg-danger text-white'
                        : 'border-accent bg-accent text-white'
                      : item.danger
                        ? 'border-danger-soft bg-danger-soft text-danger'
                        : 'border-default bg-surface text-muted'"
                  >
                    <component :is="item.icon" :size="15" />
                  </span>
                  <span class="min-w-0">
                    <span class="block text-sm font-medium leading-5">{{ item.label }}</span>
                    <span class="mt-0.5 block text-[11px] leading-4 text-dim">{{ item.hint }}</span>
                  </span>
                </div>
              </button>
            </div>
          </section>

          <section class="rounded-lg border border-default bg-surface">
            <div class="border-b border-default px-4 py-2.5">
              <div class="docmind-section-label">快捷</div>
            </div>
            <div class="grid gap-2 p-4">
              <button
                class="inline-flex items-center justify-center gap-2 rounded-md border border-default bg-surface px-4 py-2.5 text-sm font-medium text-secondary transition hover:bg-surface-hover disabled:cursor-not-allowed disabled:opacity-70"
                :disabled="loading || saving"
                @click="resetToDefaults"
              >
                <RefreshCw :size="15" />
                {{ t("page.settings.btn.reset") }}
              </button>
              <button
                class="inline-flex items-center justify-center gap-2 rounded-md bg-accent px-4 py-2.5 text-sm font-medium text-white transition disabled:cursor-not-allowed disabled:opacity-70"
                :disabled="loading || saving"
                @click="saveSettings"
              >
                <Save :size="15" />
                {{ saving ? t("page.settings.btn.saving") : t("page.settings.btn.save") }}
              </button>
              <button
                class="inline-flex items-center justify-center gap-2 rounded-md border border-danger-soft bg-danger-soft px-4 py-2.5 text-sm font-medium text-danger transition hover:opacity-90"
                @click="scrollToSection('settings-danger')"
              >
                <Trash2 :size="15" />
                {{ t("page.settings.section.danger") }}
              </button>
            </div>
          </section>

          <section class="rounded-lg border border-default bg-surface">
            <div class="border-b border-default px-4 py-2.5">
              <div class="docmind-section-label">状态</div>
            </div>
            <div class="space-y-3 p-4 text-sm">
              <div class="flex items-center justify-between gap-3">
                <span class="text-dim">{{ t("page.settings.status.synced") }}</span>
                <DocMindBadge :tone="hasChanges ? 'warning' : 'success'">{{ hasChanges ? t("page.settings.status.changed") : t("page.settings.status.synced") }}</DocMindBadge>
              </div>
              <div class="flex items-center justify-between gap-3">
                <span class="text-dim">{{ t("page.settings.language") }}</span>
                <span class="font-medium text-primary">{{ currentLocale === "zh-CN" ? "中文" : "English" }}</span>
              </div>
              <div class="flex items-center justify-between gap-3">
                <span class="text-dim">{{ t("page.settings.theme") }}</span>
                <span class="font-medium text-primary">
                  {{ themeMode === "light" ? t("page.settings.themeLight") : themeMode === "dark" ? t("page.settings.themeDark") : t("page.settings.themeSystem") }}
                </span>
              </div>
            </div>
          </section>
        </aside>

        <div class="min-w-0 space-y-4">
          <div class="grid gap-4 xl:grid-cols-2">
            <section id="settings-rules" class="scroll-mt-4 rounded-lg border border-default bg-surface">
              <div class="flex items-center justify-between border-b border-default px-4 py-2.5">
                <div>
                  <div class="docmind-section-label">{{ t("page.settings.section.rules") }}</div>
                  <div class="docmind-item-meta mt-1">{{ t("page.settings.rulesDesc") }}</div>
                </div>
                <DocMindBadge tone="default">{{ t("status.localEffective") }}</DocMindBadge>
              </div>

              <div class="space-y-4 p-4">
                <div class="grid gap-4 xl:grid-cols-[160px_minmax(0,1fr)] xl:items-start">
                  <div>
                  <div class="docmind-section-label">{{ t("page.settings.label.excludeDirs") }}</div>
                    <div class="docmind-item-meta mt-1">{{ t("page.settings.placeholder.dirs") }}</div>
                  </div>
                  <textarea
                    v-model="excludeDirsText"
                    rows="4"
                    class="w-full rounded-lg border border-default bg-input px-4 py-3 text-sm text-primary outline-none transition focus:border-[var(--color-text-dim)] focus:bg-surface"
                    :placeholder="t('page.settings.placeholder.dirs')"
                  />
                </div>

                <div class="grid gap-4 xl:grid-cols-[160px_minmax(0,1fr)] xl:items-start">
                  <div>
                  <div class="docmind-section-label">{{ t("page.settings.label.excludeExts") }}</div>
                    <div class="docmind-item-meta mt-1">{{ t("page.settings.placeholder.exts") }}</div>
                  </div>
                  <textarea
                    v-model="excludeExtsText"
                    rows="3"
                    class="w-full rounded-lg border border-default bg-input px-4 py-3 text-sm text-primary outline-none transition focus:border-[var(--color-text-dim)] focus:bg-surface"
                    :placeholder="t('page.settings.placeholder.exts')"
                  />
                </div>

                <div class="grid gap-4 xl:grid-cols-[160px_minmax(0,1fr)] xl:items-center">
                  <div>
                    <div class="docmind-section-label">{{ t("page.settings.label.maxFileSize") }}</div>
                    <div class="docmind-item-meta mt-1">{{ t("page.settings.label.maxFileSizeHint") ?? t("page.settings.placeholder.maxFileSize") }}</div>
                  </div>
                  <div class="grid gap-3 md:grid-cols-[160px_minmax(0,1fr)] md:items-center">
                    <input
                      v-model.number="maxFileSizeMb"
                      type="number"
                      min="1"
                      step="1"
                      class="w-full rounded-lg border border-default bg-input px-4 py-3 text-sm text-primary outline-none transition focus:border-[var(--color-text-dim)] focus:bg-surface"
                    />
                    <div class="rounded-lg border border-default bg-panel px-4 py-3 text-sm text-dim">
                      <div class="docmind-section-label">{{ t("page.settings.label.currentStatus") }}</div>
                      <div class="mt-1 text-sm font-medium text-primary">
                        {{ hasChanges ? t("page.settings.status.changed") : t("page.settings.status.synced") }}
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            </section>

            <section id="settings-semantic" class="scroll-mt-4 rounded-lg border border-default bg-surface">
              <div class="flex items-center justify-between border-b border-default px-4 py-2.5">
                <div>
                  <div class="docmind-section-label">{{ t("page.settings.semantic.title") }}</div>
                  <div class="docmind-item-meta mt-1">{{ t("page.settings.semantic.desc") }}</div>
                </div>
                <DocMindBadge tone="success">{{ semanticSearchEnabled ? t("page.settings.semantic.enabled") : t("page.settings.semantic.disabled") }}</DocMindBadge>
              </div>

              <div class="space-y-4 p-4">
                <div class="grid gap-4 xl:grid-cols-[160px_minmax(0,1fr)] xl:items-center">
                  <div>
                    <div class="docmind-section-label">{{ t("page.settings.semantic.title") }}</div>
                    <div class="docmind-item-meta mt-1">{{ t("page.settings.semantic.desc") }}</div>
                  </div>
                  <label class="inline-flex items-center justify-start gap-2 text-sm text-secondary">
                    <input v-model="semanticSearchEnabled" type="checkbox" class="h-4 w-4 rounded border-default text-accent accent-accent" />
                    {{ semanticSearchEnabled ? t("page.settings.semantic.enabled") : t("page.settings.semantic.disabled") }}
                  </label>
                </div>

                <div class="grid gap-4 xl:grid-cols-[160px_minmax(0,1fr)] xl:items-center">
                  <div>
                    <div class="docmind-section-label">{{ t("page.settings.semantic.weight") }}</div>
                    <div class="docmind-item-meta mt-1">{{ t("page.settings.semantic.thresholdDesc") }}</div>
                  </div>
                  <div class="rounded-lg border border-default bg-panel px-4 py-3">
                    <div class="mb-2 flex items-center justify-between docmind-item-meta">
                      <span>{{ t("page.settings.semantic.weight") }}</span>
                      <span>{{ Math.round(semanticWeight * 100) }}%</span>
                    </div>
                    <input
                      v-model.number="semanticWeight"
                      type="range"
                      min="0"
                      max="1"
                      step="0.05"
                      class="w-full accent-accent"
                    />
                  </div>
                </div>

                <div class="grid gap-4 xl:grid-cols-[180px_minmax(0,1fr)] xl:items-center">
                  <div>
                    <div class="docmind-section-label">{{ t("page.settings.semantic.threshold") }}</div>
                    <div class="docmind-item-meta mt-1">{{ t("page.settings.semantic.thresholdDesc") }}</div>
                  </div>
                  <div class="rounded-lg border border-default bg-panel px-4 py-3">
                    <div class="mb-2 flex items-center justify-between docmind-item-meta">
                      <span>{{ t("page.settings.semantic.threshold") }}</span>
                      <span>{{ Math.round(semanticThreshold * 100) }}%</span>
                    </div>
                    <input
                      v-model.number="semanticThreshold"
                      type="range"
                      min="0"
                      max="1"
                      step="0.05"
                      class="w-full accent-accent"
                    />
                  </div>
                </div>

                <div class="grid gap-4 xl:grid-cols-[160px_minmax(0,1fr)] xl:items-start">
                  <div>
                    <div class="docmind-section-label">{{ t("page.settings.preference.title") }}</div>
                    <div class="docmind-item-meta mt-1">{{ t("page.settings.preference.title") }}</div>
                  </div>
                  <div class="grid gap-2 rounded-lg border border-default bg-panel px-4 py-3 text-sm text-secondary">
                    <label class="inline-flex items-center justify-between gap-3">
                      <span>{{ t("page.settings.preference.favorite") }}</span>
                      <input v-model="preferFavoritesEnabled" type="checkbox" class="h-4 w-4 rounded border-default text-accent accent-accent" />
                    </label>
                    <label class="inline-flex items-center justify-between gap-3">
                      <span>{{ t("page.settings.preference.recent") }}</span>
                      <input v-model="preferRecentEnabled" type="checkbox" class="h-4 w-4 rounded border-default text-accent accent-accent" />
                    </label>
                    <label class="inline-flex items-center justify-between gap-3">
                      <span>{{ t("page.settings.preference.history") }}</span>
                      <input v-model="preferHistoryEnabled" type="checkbox" class="h-4 w-4 rounded border-default text-accent accent-accent" />
                    </label>
                  </div>
                </div>

                <div class="grid gap-4 xl:grid-cols-[160px_minmax(0,1fr)] xl:items-start">
                  <div>
                    <div class="docmind-section-label">{{ t("page.settings.weight.title") }}</div>
                    <div class="docmind-item-meta mt-1">{{ t("page.settings.weight.title") }}</div>
                  </div>
                  <div class="space-y-4 rounded-lg border border-default bg-panel px-4 py-3">
                    <label class="block">
                      <div class="mb-2 flex items-center justify-between docmind-item-meta">
                        <span>{{ t("page.settings.weight.titleWeight") }}</span>
                        <span>{{ titleWeight.toFixed(2) }}</span>
                      </div>
                      <input v-model.number="titleWeight" type="range" min="0" max="3" step="0.1" class="w-full accent-accent" />
                    </label>
                    <label class="block">
                      <div class="mb-2 flex items-center justify-between docmind-item-meta">
                        <span>{{ t("page.settings.weight.filenameWeight") }}</span>
                        <span>{{ filenameWeight.toFixed(2) }}</span>
                      </div>
                      <input v-model.number="filenameWeight" type="range" min="0" max="3" step="0.1" class="w-full accent-accent" />
                    </label>
                    <label class="block">
                      <div class="mb-2 flex items-center justify-between docmind-item-meta">
                        <span>{{ t("page.settings.weight.preferenceWeight") }}</span>
                        <span>{{ preferenceWeight.toFixed(2) }}</span>
                      </div>
                      <input v-model.number="preferenceWeight" type="range" min="0" max="3" step="0.1" class="w-full accent-accent" />
                    </label>
                  </div>
                </div>
              </div>
            </section>
          </div>

          <div id="settings-qa" class="scroll-mt-4">
            <DocMindQaPanel />
          </div>

          <div id="settings-model" class="scroll-mt-4">
            <DocMindSemanticPanel />
          </div>

          <div class="grid gap-4 xl:grid-cols-2">
            <section id="settings-appearance" class="scroll-mt-4 rounded-lg border border-default bg-surface">
              <div class="flex items-center justify-between border-b border-default px-4 py-2.5">
                <div>
                  <div class="docmind-section-label">{{ t("page.settings.section.appearance") }}</div>
                  <div class="docmind-item-meta mt-1">{{ t("page.settings.language") }} / {{ t("page.settings.theme") }}</div>
                </div>
                <Languages :size="15" class="text-muted" />
              </div>
              <div class="space-y-4 p-4">
                <div>
                  <div class="mb-2 docmind-section-label">{{ t("page.settings.language") }}</div>
                  <div class="grid gap-2">
                    <button
                      class="rounded-md border px-4 py-2.5 text-sm font-medium transition"
                      :class="currentLocale === 'zh-CN'
                        ? 'border-default bg-accent text-white'
                        : 'border-default bg-surface text-secondary hover:bg-surface-hover'"
                      @click="switchLocale('zh-CN')"
                    >
                      中文
                    </button>
                    <button
                      class="rounded-md border px-4 py-2.5 text-sm font-medium transition"
                      :class="currentLocale === 'en'
                        ? 'border-default bg-accent text-white'
                        : 'border-default bg-surface text-secondary hover:bg-surface-hover'"
                      @click="switchLocale('en')"
                    >
                      English
                    </button>
                  </div>
                </div>

                <div>
                  <div class="mb-2 docmind-section-label">{{ t("page.settings.theme") }}</div>
                  <div class="grid gap-2">
                    <button
                      class="inline-flex items-center justify-center gap-1.5 rounded-md border px-4 py-2.5 text-sm font-medium transition"
                      :class="themeMode === 'light'
                        ? 'border-default bg-accent text-white'
                        : 'border-default bg-surface text-secondary hover:bg-surface-hover'"
                      @click="setTheme('light')"
                    >
                      <Sun :size="15" />{{ t("page.settings.themeLight") }}
                    </button>
                    <button
                      class="inline-flex items-center justify-center gap-1.5 rounded-md border px-4 py-2.5 text-sm font-medium transition"
                      :class="themeMode === 'dark'
                        ? 'border-default bg-accent text-white'
                        : 'border-default bg-surface text-secondary hover:bg-surface-hover'"
                      @click="setTheme('dark')"
                    >
                      <Moon :size="15" />{{ t("page.settings.themeDark") }}
                    </button>
                    <button
                      class="inline-flex items-center justify-center gap-1.5 rounded-md border px-4 py-2.5 text-sm font-medium transition"
                      :class="themeMode === 'system'
                        ? 'border-default bg-accent text-white'
                        : 'border-default bg-surface text-secondary hover:bg-surface-hover'"
                      @click="setTheme('system')"
                    >
                      <Monitor :size="15" />{{ t("page.settings.themeSystem") }}
                    </button>
                  </div>
                </div>
              </div>
            </section>

            <section class="rounded-lg border border-default bg-surface">
              <div class="flex items-center justify-between border-b border-default px-4 py-2.5">
                <div>
                  <div class="docmind-section-label">{{ t("page.settings.section.instructions") }}</div>
                  <div class="docmind-item-meta mt-1">{{ t("page.settings.instructions.effective") }}</div>
                </div>
                <DocMindBadge tone="success">{{ t("status.savedLocally") }}</DocMindBadge>
              </div>
              <div class="space-y-2 p-4 text-sm text-secondary">
                <p>• {{ t("page.settings.instructions.dirs") }}</p>
                <p>• {{ t("page.settings.instructions.exts") }}</p>
                <p>• {{ t("page.settings.instructions.maxSize") }}</p>
              </div>
            </section>
          </div>

          <section id="settings-network" class="scroll-mt-4 rounded-lg border border-default bg-surface">
            <div class="flex items-center justify-between border-b border-default px-4 py-2.5">
              <div>
                <div class="docmind-section-label">{{ t("page.settings.network.title") }}</div>
                <div class="docmind-item-meta mt-1">{{ t("page.settings.network.desc") }}</div>
              </div>
              <DocMindBadge :tone="networkProxyEnabled ? 'success' : 'default'">
                {{ networkProxyEnabled ? t("page.settings.network.enabled") : t("page.settings.network.disabled") }}
              </DocMindBadge>
            </div>

            <div class="space-y-4 p-4">
              <div
                v-if="networkErrorMessage"
                class="rounded-md border border-danger-soft bg-danger-soft px-4 py-2.5 text-xs text-danger"
              >
                {{ networkErrorMessage }}
              </div>
              <div
                v-if="networkInfoMessage"
                class="rounded-md border border-emerald-soft bg-emerald-soft px-4 py-2.5 text-xs text-success"
              >
                {{ networkInfoMessage }}
              </div>

              <div class="grid gap-4 xl:grid-cols-[160px_minmax(0,1fr)] xl:items-center">
                <div>
                  <div class="docmind-section-label">{{ t("page.settings.network.enableLabel") }}</div>
                  <div class="docmind-item-meta mt-1">{{ t("page.settings.network.enableHint") }}</div>
                </div>
                <label class="inline-flex items-center justify-start gap-2 text-sm text-secondary">
                  <input
                    v-model="networkProxyEnabled"
                    type="checkbox"
                    class="h-4 w-4 rounded border-default text-accent accent-accent"
                  />
                  {{ networkProxyEnabled ? t("page.settings.network.enabled") : t("page.settings.network.disabled") }}
                </label>
              </div>

              <div class="grid gap-4 xl:grid-cols-[160px_minmax(0,1fr)] xl:items-start">
                <div>
                  <div class="docmind-section-label">{{ t("page.settings.network.proxyUrl") }}</div>
                  <div class="docmind-item-meta mt-1">{{ t("page.settings.network.proxyUrlHint") }}</div>
                </div>
                <input
                  v-model="networkProxyUrl"
                  type="text"
                  class="w-full rounded-lg border border-default bg-input px-4 py-3 text-sm text-primary outline-none transition focus:border-[var(--color-text-dim)] focus:bg-surface"
                  :placeholder="t('page.settings.network.proxyPlaceholder')"
                />
              </div>

              <div class="rounded-lg border border-default bg-panel px-4 py-3 text-xs leading-5 text-secondary">
                <div class="docmind-section-label text-primary">{{ t("page.settings.network.exampleTitle") }}</div>
                <div class="mt-1">{{ t("page.settings.network.exampleDesc") }}</div>
              </div>

              <div class="flex items-center justify-end gap-2">
                <DocMindBadge v-if="hasNetworkProxyChanges" tone="warning">{{ t("page.settings.status.changed") }}</DocMindBadge>
                <button
                  class="inline-flex items-center gap-2 rounded-md bg-accent px-4 py-2.5 text-sm font-medium text-white transition disabled:cursor-not-allowed disabled:opacity-70"
                  :disabled="networkLoading || networkSaving"
                  @click="saveNetworkProxySettings"
                >
                  <Save :size="15" />
                  {{ networkSaving ? t("page.settings.network.saving") : t("page.settings.network.save") }}
                </button>
              </div>
            </div>
          </section>

          <section class="rounded-lg border border-default bg-surface">
            <div class="flex items-center justify-between border-b border-default px-4 py-2.5">
              <div>
                <div class="docmind-section-label">{{ t("page.settings.section.privacy") }}</div>
                <div class="docmind-item-meta mt-1">{{ t("page.settings.privacy.desc") }}</div>
              </div>
              <Shield :size="15" class="text-muted" />
            </div>
            <div class="space-y-2 p-4 text-sm text-secondary">
              <p>{{ t("page.settings.privacy.localFirst") }}</p>
              <p>{{ t("page.settings.privacy.localFirstDesc") }}</p>
              <p>{{ t("page.settings.privacy.localOnlyHint") }}</p>
            </div>
          </section>

          <section id="settings-danger" class="scroll-mt-4 rounded-lg border border-danger-soft bg-danger-soft px-4 py-3">
            <div class="flex items-start justify-between gap-4">
              <div class="min-w-0">
                <div class="flex items-center gap-2 docmind-section-label text-danger">
                  <Trash2 :size="15" />
                  {{ t("page.settings.section.danger") }}
                </div>
                <div class="docmind-item-meta mt-1 text-secondary">{{ t("page.settings.danger.desc") }}</div>
                <div class="docmind-item-meta mt-2 leading-5 text-secondary">
                  {{ t("page.settings.danger.detail") }}
                </div>
              </div>
              <button
                class="inline-flex shrink-0 items-center gap-2 rounded-md bg-danger px-4 py-2 text-sm font-medium text-white hover:opacity-80 disabled:cursor-not-allowed disabled:opacity-50"
                :disabled="clearing"
                @click="clearAllIndexes"
              >
                <RefreshCw :size="15" :class="{ 'animate-spin': clearing }" />
                {{ clearing ? t("page.settings.btn.clearing") : t("page.settings.btn.clear") }}
              </button>
            </div>
          </section>
        </div>
      </div>
    </main>

    <DocMindConfirmDialog
      :visible="showClearConfirm"
      :title="t('page.settings.btn.clear')"
      :message="t('page.settings.confirmClear')"
      :confirm-text="t('page.settings.btn.clear')"
      :cancel-text="t('common.cancel')"
      :danger="true"
      @confirm="handleClearConfirm"
      @cancel="handleClearCancel"
    />
  </div>
</template>
