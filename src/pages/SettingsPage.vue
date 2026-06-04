<script setup lang="ts">
/**
 * @author MorningSun
 * @CreatedDate 2026/06/02
 * @Description 设置页，承载索引规则、语义检索、AI 回答和外观网络配置。
 */
import { computed, onBeforeUnmount, onMounted, ref, nextTick } from "vue";
import { useI18n } from "vue-i18n";
import { Database, Globe, Languages, MessageSquareText, Moon, Monitor, RefreshCw, Save, Settings, Shield, SlidersHorizontal, Sparkles, Sun, Trash2 } from "lucide-vue-next";
import { useTheme } from "../composables/useTheme";
import DocMindBadge from "../components/docmind/DocMindBadge.vue";
import DocMindConfirmDialog from "../components/docmind/DocMindConfirmDialog.vue";
import DocMindQaPanel from "../components/docmind/DocMindQaPanel.vue";
import DocMindSemanticPanel from "../components/docmind/DocMindSemanticPanel.vue";
import { docmindApi, formatDocmindError } from "../services/docmindApi";
import { useInfoMessage } from "../composables/useInfoMessage";
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
const { infoMessage } = useInfoMessage();
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
    // 规则分区保留滑杆图标，和配置含义保持一致。
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
  <div class="settings-prototype-shell flex h-full min-h-0 flex-col bg-page text-primary">
    <header class="settings-prototype-topbar">
      <div class="settings-prototype-header-left">
        <div class="settings-prototype-header-title">
          <span class="settings-prototype-title-icon docmind-page-header-icon" aria-hidden="true">
            <Settings :size="17" />
          </span>
          <h1 class="settings-prototype-title">{{ t("page.settings.title") }}</h1>
        </div>
        <p class="settings-prototype-subtitle">{{ t("page.settings.subtitle") }}</p>
      </div>

      <div class="settings-prototype-header-right">
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

    <main ref="mainScrollEl" class="settings-prototype-main min-h-0 flex-1 overflow-y-auto p-4">
      <div v-if="errorMessage" class="mb-3 rounded-md border border-danger-soft bg-danger-soft px-4 py-2.5 text-xs text-danger">
        {{ errorMessage }}
      </div>

      <div v-if="infoMessage" class="mb-3 rounded-md border border-emerald-soft bg-emerald-soft px-4 py-2.5 text-xs text-success">
        {{ infoMessage }}
      </div>

      <div v-if="loading" class="rounded-md border border-dashed border-default bg-surface px-4 py-6 text-xs text-muted">
        {{ t("page.settings.loading") }}
      </div>

      <div v-else class="settings-workbench grid w-full gap-5 xl:grid-cols-[248px_minmax(0,1fr)]">
        <aside class="settings-prototype-sidebar settings-sidebar-rail hidden min-h-0 min-w-0 self-start xl:sticky xl:top-4 xl:block">
          <section class="settings-sidebar-shell rounded-lg border border-default bg-surface">
            <div class="settings-sidebar-head flex items-center justify-between border-b border-default px-4 py-2.5">
              <div>
                <div class="docmind-section-label">导航</div>
                <div class="docmind-item-meta mt-1">{{ t("page.settings.instructions.effective") }}</div>
              </div>
              <DocMindBadge tone="success">{{ t("status.savedLocally") }}</DocMindBadge>
            </div>
            <div class="settings-sidebar-nav space-y-2 p-3">
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
                    <span class="block text-[12px] font-medium leading-5">{{ item.label }}</span>
                    <span class="mt-0.5 block text-[10px] leading-4 text-dim">{{ item.hint }}</span>
                  </span>
                </div>
              </button>
            </div>
          </section>

          <section class="settings-sidebar-shell rounded-lg border border-default bg-surface">
            <div class="border-b border-default px-4 py-2.5">
              <div class="docmind-section-label">快捷</div>
            </div>
            <div class="grid gap-2 p-3">
              <button
                class="inline-flex items-center justify-center gap-2 rounded-md border border-default bg-surface px-3 py-2.5 text-sm font-medium text-secondary transition hover:bg-surface-hover disabled:cursor-not-allowed disabled:opacity-70"
                :disabled="loading || saving"
                @click="resetToDefaults"
              >
                <RefreshCw :size="15" />
                {{ t("page.settings.btn.reset") }}
              </button>
              <button
                class="inline-flex items-center justify-center gap-2 rounded-md bg-accent px-3 py-2.5 text-sm font-medium text-white transition disabled:cursor-not-allowed disabled:opacity-70"
                :disabled="loading || saving"
                @click="saveSettings"
              >
                <Save :size="15" />
                {{ saving ? t("page.settings.btn.saving") : t("page.settings.btn.save") }}
              </button>
              <button
                class="inline-flex items-center justify-center gap-2 rounded-md border border-danger-soft bg-danger-soft px-3 py-2.5 text-sm font-medium text-danger transition hover:opacity-90"
                @click="scrollToSection('settings-danger')"
              >
                <Trash2 :size="15" />
                {{ t("page.settings.section.danger") }}
              </button>
            </div>
          </section>

          <section class="settings-sidebar-shell rounded-lg border border-default bg-surface">
            <div class="border-b border-default px-4 py-2.5">
              <div class="docmind-section-label">状态</div>
            </div>
            <div class="space-y-3 p-3 text-sm">
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
              <div class="settings-section-head">
                <div class="settings-section-head-left">
                  <span class="settings-section-icon docmind-primary-icon">
                    <SlidersHorizontal :size="18" />
                  </span>
                  <div class="min-w-0">
                    <div class="settings-section-title">{{ t("page.settings.section.rules") }}</div>
                    <div class="settings-section-desc">{{ t("page.settings.rulesDesc") }}</div>
                  </div>
                </div>
                <DocMindBadge tone="default">{{ t("status.localEffective") }}</DocMindBadge>
              </div>

              <div class="space-y-5 p-4">
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
              <div class="settings-section-head">
                <div class="settings-section-head-left">
                  <span class="settings-section-icon docmind-primary-icon">
                    <Sparkles :size="18" />
                  </span>
                  <div class="min-w-0">
                    <div class="settings-section-title">{{ t("page.settings.semantic.title") }}</div>
                    <div class="settings-section-desc">{{ t("page.settings.semantic.desc") }}</div>
                  </div>
                </div>
                <DocMindBadge tone="success">{{ semanticSearchEnabled ? t("page.settings.semantic.enabled") : t("page.settings.semantic.disabled") }}</DocMindBadge>
              </div>

              <div class="space-y-5 p-4">
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
              <div class="settings-section-head">
                <div class="settings-section-head-left">
                  <span class="settings-section-icon docmind-primary-icon">
                    <Languages :size="18" />
                  </span>
                  <div class="min-w-0">
                    <div class="settings-section-title">{{ t("page.settings.section.appearance") }}</div>
                    <div class="settings-section-desc">{{ t("page.settings.language") }} / {{ t("page.settings.theme") }}</div>
                  </div>
                </div>
                <DocMindBadge tone="default">{{ themeMode === "light" ? t("page.settings.themeLight") : themeMode === "dark" ? t("page.settings.themeDark") : t("page.settings.themeSystem") }}</DocMindBadge>
              </div>
              <div class="space-y-5 p-4">
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
              <div class="settings-section-head">
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
            <div class="settings-section-head">
              <div class="settings-section-head-left">
                <span class="settings-section-icon docmind-primary-icon">
                  <Globe :size="18" />
                </span>
                <div class="min-w-0">
                  <div class="settings-section-title">{{ t("page.settings.network.title") }}</div>
                  <div class="settings-section-desc">{{ t("page.settings.network.desc") }}</div>
                </div>
              </div>
              <DocMindBadge :tone="networkProxyEnabled ? 'success' : 'default'">
                {{ networkProxyEnabled ? t("page.settings.network.enabled") : t("page.settings.network.disabled") }}
              </DocMindBadge>
            </div>

            <div class="space-y-5 p-4">
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
            <div class="settings-section-head">
              <div class="settings-section-head-left">
                <span class="settings-section-icon docmind-primary-icon">
                  <Shield :size="18" />
                </span>
                <div class="min-w-0">
                  <div class="settings-section-title">{{ t("page.settings.section.privacy") }}</div>
                  <div class="settings-section-desc">{{ t("page.settings.privacy.desc") }}</div>
                </div>
              </div>
              <DocMindBadge tone="success">{{ t("status.savedLocally") }}</DocMindBadge>
            </div>
            <div class="space-y-2 p-4 text-sm text-secondary">
              <p>{{ t("page.settings.privacy.localFirst") }}</p>
              <p>{{ t("page.settings.privacy.localFirstDesc") }}</p>
              <p>{{ t("page.settings.privacy.remoteModel") }}</p>
              <p>{{ t("page.settings.privacy.localOnlyHint") }}</p>
            </div>
          </section>

          <section id="settings-danger" class="scroll-mt-4 rounded-lg border border-danger-soft bg-danger-soft px-4 py-3">
            <div class="settings-section-head settings-section-head-danger">
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
              <DocMindBadge tone="warning">{{ t("page.settings.btn.clear") }}</DocMindBadge>
            </div>
            <div class="mt-4 flex justify-end">
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

<style scoped>
.settings-prototype-shell {
  color: #eef5ff;
  background:
    radial-gradient(circle at 78% 12%, rgba(47, 129, 255, 0.08), transparent 34%),
    radial-gradient(circle at 28% 22%, rgba(47, 129, 255, 0.05), transparent 32%),
    linear-gradient(135deg, #060b14 0%, #0a111b 42%, #0d1521 100%);
}

.settings-prototype-main {
  padding: 26px 30px 82px;
}

.settings-workbench {
  align-items: start;
}

.settings-prototype-topbar {
  position: sticky;
  top: 0;
  z-index: 20;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  min-height: 48px;
  padding: 0 20px;
  flex-shrink: 0;
  border-bottom: 1px solid rgba(138, 161, 190, 0.18);
  background-color: rgba(10, 15, 24, 0.92);
  backdrop-filter: blur(10px);
}

.settings-prototype-header-left {
  flex: 1;
  min-width: 0;
}

.settings-prototype-header-title {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 0;
}

.settings-prototype-title-icon {
  display: inline-flex;
  align-items: center;
  color: var(--color-accent);
}

.settings-prototype-title {
  margin: 0;
  font-size: 16px;
  font-weight: 600;
  line-height: 1.25;
  letter-spacing: -0.01em;
  color: #eef5ff;
}

.settings-prototype-subtitle {
  margin: 0;
  font-size: 13px;
  line-height: 1.35;
  color: #9aa9bd;
}

.settings-prototype-header-right {
  display: flex;
  align-items: center;
  gap: 16px;
  flex-shrink: 0;
}

.settings-prototype-header-right > button {
  height: 42px;
  padding: 0 16px;
  border-radius: 8px;
  border: 1px solid rgba(105, 134, 171, 0.25);
  background: rgba(17, 24, 39, 0.7);
  color: #eef5ff;
  font-weight: 800;
  box-shadow: none;
}

.settings-prototype-header-right > button:first-child {
  color: #ffc76c;
  border-color: rgba(247, 184, 75, 0.34);
}

.settings-prototype-header-right > button:last-child {
  background: linear-gradient(135deg, #2f81ff, #1267e8);
  border-color: rgba(81, 151, 255, 0.8);
  box-shadow: 0 14px 34px rgba(47, 129, 255, 0.22);
}

/* 修复：收敛深色主题的蓝色面积，保留层次但避免整页发蓝。 */
.settings-prototype-main :is(section, .rounded-lg, .rounded-md) {
  border-color: rgba(138, 161, 190, 0.18) !important;
}

.settings-prototype-main section,
.settings-prototype-sidebar > section {
  border-radius: 16px !important;
  background: linear-gradient(180deg, rgba(15, 23, 42, 0.84), rgba(10, 16, 28, 0.8)) !important;
  box-shadow:
    var(--shadow-card, 0 1px 2px rgba(0, 0, 0, 0.05)),
    inset 0 1px 0 rgba(255, 255, 255, 0.035);
}

.settings-prototype-sidebar > section {
  background: rgba(14, 20, 32, 0.84) !important;
  backdrop-filter: blur(18px);
}

.settings-sidebar-rail {
  padding-right: 18px;
  border-right: 1px solid rgba(138, 161, 190, 0.12);
}

.settings-sidebar-rail > section {
  border-radius: 0 !important;
  border-top: 0;
  border-left: 0;
  border-right: 0;
  background: transparent !important;
  box-shadow: none;
  backdrop-filter: none;
}

.settings-sidebar-rail > section:first-child {
  border-top: 1px solid rgba(138, 161, 190, 0.12);
  border-radius: 14px 14px 0 0 !important;
}

.settings-sidebar-rail > section:last-child {
  border-radius: 0 0 14px 14px !important;
}

.settings-sidebar-shell {
  overflow: hidden;
}

.settings-sidebar-head {
  padding-top: 10px;
  padding-bottom: 10px;
}

.settings-sidebar-nav {
  align-content: start;
}

.settings-section-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 16px 18px 14px;
  border-bottom: 1px solid var(--color-border);
}

.settings-section-head-left {
  display: flex;
  align-items: center;
  gap: 14px;
  min-width: 0;
}

.settings-section-icon {
  width: 44px;
  height: 44px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  border-radius: 14px;
  color: white;
}

.settings-section-title {
  font-size: 17px;
  font-weight: 850;
  letter-spacing: -0.02em;
  color: var(--color-text-primary);
}

.settings-section-desc {
  margin-top: 4px;
  font-size: 13px;
  color: var(--color-text-secondary);
}

.settings-section-head-danger {
  align-items: flex-start;
  background: rgba(255, 101, 101, 0.04);
  border-bottom-color: transparent;
  border-radius: 16px 16px 0 0;
}

.settings-prototype-sidebar .space-y-2.p-4,
.settings-prototype-sidebar .space-y-2.p-3,
.settings-sidebar-nav {
  display: grid;
  gap: 7px;
}

.settings-prototype-sidebar button {
  min-height: 50px;
  border-radius: 10px;
  border: 1px solid transparent;
  color: #bfd0e5;
  background: rgba(255, 255, 255, 0.018);
}

.settings-prototype-sidebar button:hover {
  background: rgba(255, 255, 255, 0.03);
  border-color: rgba(138, 161, 190, 0.18);
}

.settings-prototype-sidebar button[class*="border-accent"] {
  color: #ffffff;
  background: linear-gradient(135deg, rgba(47, 129, 255, 0.32), rgba(13, 95, 215, 0.2));
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.08);
}

.settings-prototype-sidebar .docmind-section-label,
.settings-prototype-main .docmind-section-label {
  color: #cbd8ea;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  font-size: 12px;
  font-weight: 800;
}

.settings-prototype-sidebar .docmind-item-meta,
.settings-prototype-main .docmind-item-meta {
  color: #9aa9bd;
}

.settings-prototype-main input:not([type="checkbox"]):not([type="range"]),
.settings-prototype-main textarea,
.settings-prototype-main .rounded-lg.border.border-default.bg-input,
.settings-prototype-main .rounded-lg.border.border-default.bg-panel {
  background: rgba(15, 23, 42, 0.44) !important;
  border-color: rgba(138, 161, 190, 0.18) !important;
  color: #eef5ff !important;
}

.settings-prototype-main input::placeholder,
.settings-prototype-main textarea::placeholder {
  color: #697b92;
}

.settings-prototype-main input[type="checkbox"] {
  accent-color: #2f81ff;
}

.settings-prototype-main input[type="range"] {
  accent-color: #2f81ff;
}

.settings-prototype-main .rounded-lg.border.border-default.bg-panel,
.settings-prototype-main .rounded-md.border.border-default.bg-panel {
  background: rgba(15, 23, 42, 0.54) !important;
}

.settings-prototype-main .rounded-md.border.border-danger-soft.bg-danger-soft,
.settings-prototype-main .rounded-lg.border.border-danger-soft.bg-danger-soft {
  background: rgba(68, 18, 24, 0.28) !important;
  border-color: rgba(255, 101, 101, 0.25) !important;
}

.settings-prototype-main .bg-accent {
  background: linear-gradient(135deg, #2f81ff, #1267e8) !important;
}

.settings-prototype-main .text-primary {
  color: #eef5ff !important;
}

.settings-prototype-main .text-secondary,
.settings-prototype-main .text-dim,
.settings-prototype-main .text-muted {
  color: #9aa9bd !important;
}

html:not(.dark) .settings-prototype-shell {
  color: #0f172a;
  background:
    radial-gradient(circle at 78% 12%, rgba(47, 129, 255, 0.11), transparent 34%),
    radial-gradient(circle at 28% 22%, rgba(47, 129, 255, 0.07), transparent 32%),
    linear-gradient(135deg, #f8fbff 0%, #eef4fb 42%, #eaf2fb 100%);
}

html:not(.dark) .settings-prototype-topbar {
  border-bottom-color: rgba(148, 163, 184, 0.2);
  background-color: rgba(248, 251, 255, 0.96);
}

html:not(.dark) .settings-prototype-title {
  color: #0f172a;
}

html:not(.dark) .settings-prototype-subtitle {
  color: #64748b;
}

html:not(.dark) .settings-prototype-header-right > button {
  background: rgba(255, 255, 255, 0.86);
  border-color: rgba(148, 163, 184, 0.32);
  color: #334155;
}

html:not(.dark) .settings-prototype-header-right > button:first-child {
  color: #92400e;
  border-color: rgba(245, 158, 11, 0.28);
}

html:not(.dark) .settings-prototype-header-right > button:last-child {
  background: linear-gradient(135deg, #2f81ff, #1267e8);
  border-color: rgba(47, 129, 255, 0.44);
  color: white;
}

html:not(.dark) .settings-prototype-main section,
html:not(.dark) .settings-prototype-sidebar > section {
  background: rgba(255, 255, 255, 0.92) !important;
  border-color: rgba(148, 163, 184, 0.24) !important;
  box-shadow: 0 12px 28px rgba(15, 23, 42, 0.045), inset 0 1px 0 rgba(255, 255, 255, 0.82);
}

html:not(.dark) .settings-prototype-sidebar > section {
  background: rgba(255, 255, 255, 0.88) !important;
  backdrop-filter: blur(14px);
}

html:not(.dark) .settings-sidebar-rail {
  border-right-color: rgba(148, 163, 184, 0.16);
}

html:not(.dark) .settings-sidebar-rail > section {
  background: transparent !important;
  box-shadow: none;
  backdrop-filter: none;
}

html:not(.dark) .settings-sidebar-rail > section:first-child {
  border-top-color: rgba(148, 163, 184, 0.16);
}

html:not(.dark) .settings-prototype-sidebar button {
  color: #334155;
  background: rgba(248, 250, 252, 0.84);
  box-shadow: none;
}

html:not(.dark) .settings-prototype-sidebar button:hover {
  background: rgba(241, 245, 249, 0.96);
  border-color: rgba(148, 163, 184, 0.3);
}

html:not(.dark) .settings-prototype-sidebar button[class*="border-accent"] {
  color: white;
  background: linear-gradient(135deg, rgba(47, 129, 255, 0.95), rgba(13, 95, 215, 0.78));
}

/* 修复：浅色主题选中导航项的说明文字需要更高对比度，避免被亮蓝底吃掉。 */
html:not(.dark) .settings-prototype-sidebar button[class*="border-accent"] .text-dim {
  color: rgba(232, 241, 255, 0.88) !important;
}

html:not(.dark) .settings-prototype-sidebar .docmind-section-label,
html:not(.dark) .settings-prototype-main .docmind-section-label {
  color: #475569;
}

html:not(.dark) .settings-prototype-sidebar .docmind-item-meta,
html:not(.dark) .settings-prototype-main .docmind-item-meta {
  color: #64748b;
}

html:not(.dark) .settings-prototype-main input:not([type="checkbox"]):not([type="range"]),
html:not(.dark) .settings-prototype-main textarea,
html:not(.dark) .settings-prototype-main .rounded-lg.border.border-default.bg-input,
html:not(.dark) .settings-prototype-main .rounded-lg.border.border-default.bg-panel {
  background: rgba(255, 255, 255, 0.92) !important;
  border-color: rgba(148, 163, 184, 0.24) !important;
  color: #0f172a !important;
}

html:not(.dark) .settings-prototype-main input::placeholder,
html:not(.dark) .settings-prototype-main textarea::placeholder {
  color: #94a3b8;
}

html:not(.dark) .settings-prototype-main .rounded-lg.border.border-default.bg-panel,
html:not(.dark) .settings-prototype-main .rounded-md.border.border-default.bg-panel {
  background: rgba(248, 250, 252, 0.94) !important;
}

html:not(.dark) .settings-prototype-main .rounded-md.border.border-danger-soft.bg-danger-soft,
html:not(.dark) .settings-prototype-main .rounded-lg.border.border-danger-soft.bg-danger-soft {
  background: rgba(255, 241, 242, 0.9) !important;
  border-color: rgba(248, 113, 113, 0.22) !important;
}

html:not(.dark) .settings-prototype-main .text-primary {
  color: #0f172a !important;
}

html:not(.dark) .settings-prototype-main .text-secondary,
html:not(.dark) .settings-prototype-main .text-dim,
html:not(.dark) .settings-prototype-main .text-muted {
  color: #64748b !important;
}

html:not(.dark) .settings-section-head {
  border-bottom-color: rgba(148, 163, 184, 0.2);
}

html:not(.dark) .settings-section-title {
  color: #0f172a;
}

html:not(.dark) .settings-section-desc {
  color: #64748b;
}

html:not(.dark) .settings-section-head-danger {
  background: rgba(255, 241, 242, 0.9);
}

html:not(.dark) .settings-prototype-main .bg-accent {
  background: linear-gradient(135deg, #2f81ff, #1267e8) !important;
}

html:not(.dark) .settings-prototype-main .border-default {
  border-color: rgba(148, 163, 184, 0.24) !important;
}

@media (max-width: 768px) {
  .settings-prototype-main {
    padding: 16px;
  }

  .settings-prototype-topbar {
    padding: 0 16px;
  }

  .settings-prototype-title {
    font-size: 16px;
  }

  .settings-prototype-subtitle {
    font-size: 13px;
  }
}
</style>
