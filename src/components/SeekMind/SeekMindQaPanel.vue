<script setup lang="ts">
/**
 * @author MorningSun
 * @CreatedDate 2026/06/02
 * @Description 设置页中的 LLM 连接配置面板，负责模型、参数与连通性测试。
 */
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { Check, ChevronDown, CircleHelp, MessageSquareText, Plus, RefreshCw, Save, Shield, SlidersHorizontal, Trash2 } from "lucide-vue-next";
import SeekMindConfirmDialog from "./SeekMindConfirmDialog.vue";
import SeekMindBadge from "./SeekMindBadge.vue";
import SeekMindToast from "./SeekMindToast.vue";
import { seekMindApi, formatSeekMindError } from "../../services/seekMindApi";
import { useInfoMessage } from "../../composables/useInfoMessage";
import { emitQaConfigUpdated } from "../../utils/qaConfigEvents";
import type { QaConnectionTestView, QaModelProfileUpsertView, QaModelProfileView, QaSettingsView } from "../../types/SeekMind";

const { t } = useI18n();

const savedSettings = ref<QaSettingsView | null>(null);
// 修复：模型启用状态改为自动维持，不再暴露给用户手动切换，避免默认连接与当前配置状态分裂。
const enabled = ref(true);
type ProviderPreset = "openai_compatible" | "ollama" | "google_ai" | "deepseek" | "custom";

const providerPresets: ProviderPreset[] = ["openai_compatible", "ollama", "google_ai", "deepseek", "custom"];
// 修复：LLM 连接默认切到 Ollama，本地开发环境优先使用本机服务端点。
const providerMode = ref<ProviderPreset>("ollama");
const customProvider = ref("");
const baseUrl = ref("http://127.0.0.1:11434/v1");
const apiKey = ref("");
const model = ref("");
const temperature = ref(0.2);
const maxOutputTokens = ref(600);
const contextChunkLimit = ref(8);
const contextTokenBudget = ref(6000);
const minEvidenceCount = ref(2);
const minRetrievalScore = ref(0);
const loading = ref(false);
const saving = ref(false);
const testing = ref(false);
const profilesLoading = ref(false);
const profileDeleting = ref(false);
const errorMessage = ref("");
// 修复：保存成功提示必须自动消失，避免右上角浮层常驻遮挡设置页内容。
const { infoMessage: saveMessage } = useInfoMessage();
// 修复：连接测试结果仅保留为按钮旁轻量状态，不再占用底部独立卡片空间。
const { infoMessage: profileToastMessage } = useInfoMessage();
const profileErrorMessage = ref("");
const connectionResult = ref<QaConnectionTestView | null>(null);
const profiles = ref<QaModelProfileView[]>([]);
const selectedProfileId = ref("");
const editingProfileId = ref("");
const profilesReady = ref(false);

interface IntentSynonymRuleForm {
  id: string;
  name: string;
  markersText: string;
  recallTermsText: string;
  noiseTermsText: string;
}

const cloneIntentRule = (rule: IntentSynonymRuleForm): IntentSynonymRuleForm => ({
  id: rule.id,
  name: rule.name,
  markersText: rule.markersText,
  recallTermsText: rule.recallTermsText,
  noiseTermsText: rule.noiseTermsText,
});

const normalizeProviderValue = (value: string) => value.trim().toLowerCase().replace(/[\s-]+/g, "_");

const resolveProviderMode = (value: string): ProviderPreset => {
  const normalized = normalizeProviderValue(value);
  return providerPresets.includes(normalized as ProviderPreset) ? (normalized as ProviderPreset) : "custom";
};

const providerValue = computed(() => (providerMode.value === "custom" ? customProvider.value.trim() : providerMode.value));
const connectionStatus = computed(() => {
  if (!connectionResult.value) {
    return null;
  }

  return {
    tone: connectionResult.value.ok ? ("success" as const) : ("error" as const),
    label: connectionResult.value.ok
      ? t("page.settings.qa.connectionTestPassed")
      : t("page.settings.qa.connectionTestFailed"),
  };
});
const intentRules = ref<IntentSynonymRuleForm[]>([]);
const savedIntentRules = ref<IntentSynonymRuleForm[]>([]);
const intentRulesSaving = ref(false);
const intentRuleDeleteTarget = ref<IntentSynonymRuleForm | null>(null);
const intentRulesScrollRef = ref<HTMLElement | null>(null);
const splitRuleTerms = (value: string) =>
  value
    .split(/[\n,，、;；]/)
    .map((item) => item.trim())
    .filter(Boolean);

const joinRuleTerms = (values: unknown) => {
  if (!Array.isArray(values)) {
    return "";
  }
  return values.map((item) => String(item).trim()).filter(Boolean).join("\n");
};

const createEmptyIntentRule = (): IntentSynonymRuleForm => ({
  id: `${Date.now()}-${Math.random().toString(16).slice(2)}`,
  name: "",
  markersText: "",
  recallTermsText: "",
  noiseTermsText: "",
});

const parseIntentRules = (raw: string): IntentSynonymRuleForm[] => {
  const trimmed = raw.trim();
  if (!trimmed) {
    return [];
  }

  try {
    const parsed = JSON.parse(trimmed) as unknown;
    if (!Array.isArray(parsed)) {
      return [];
    }
    return parsed
      .filter((item): item is Record<string, unknown> => Boolean(item && typeof item === "object" && !Array.isArray(item)))
      .map((item, index) => ({
        id: String(item.id ?? `${index}-${Date.now()}-${Math.random().toString(16).slice(2)}`),
        name: String(item.name ?? "").trim(),
        markersText: joinRuleTerms(item.markers),
        recallTermsText: joinRuleTerms(item.recall_terms),
        noiseTermsText: joinRuleTerms(item.noise_terms),
      }));
  } catch {
    return [];
  }
};

const serializeIntentRules = () => {
  const rules = intentRules.value
    .map((rule) => ({
      name: rule.name.trim(),
      markers: splitRuleTerms(rule.markersText),
      recall_terms: splitRuleTerms(rule.recallTermsText),
      noise_terms: splitRuleTerms(rule.noiseTermsText),
    }))
    .filter((rule) => rule.name || rule.markers.length > 0 || rule.recall_terms.length > 0 || rule.noise_terms.length > 0);

  if (rules.length === 0) {
    return "";
  }
  return JSON.stringify(rules, null, 2);
};

const savedIntentRulesJson = computed(() => savedSettings.value?.intent_synonym_rules_json?.trim() ?? "");
const currentIntentRulesJson = computed(() => serializeIntentRules());
const isIntentRuleBlank = (rule: IntentSynonymRuleForm) =>
  !rule.name.trim() && !rule.markersText.trim() && !rule.recallTermsText.trim() && !rule.noiseTermsText.trim();
const isIntentRuleDirty = (rule: IntentSynonymRuleForm) => {
  const savedRule = savedIntentRules.value.find((item) => item.id === rule.id);
  if (!savedRule) {
    return !isIntentRuleBlank(rule);
  }

  return (
    rule.name.trim() !== savedRule.name.trim() ||
    rule.markersText.trim() !== savedRule.markersText.trim() ||
    rule.recallTermsText.trim() !== savedRule.recallTermsText.trim() ||
    rule.noiseTermsText.trim() !== savedRule.noiseTermsText.trim()
  );
};
const addIntentRule = async () => {
  intentRules.value = [...intentRules.value, createEmptyIntentRule()];
  // 修复：规则条目超过一屏时，新增后自动滚动到底部，让新建行立即可见并可继续编辑。
  await nextTick();
  intentRulesScrollRef.value?.scrollTo({
    top: intentRulesScrollRef.value.scrollHeight,
    behavior: "auto",
  });
};
const requestRemoveIntentRule = (rule: IntentSynonymRuleForm) => {
  intentRuleDeleteTarget.value = cloneIntentRule(rule);
};
const cancelRemoveIntentRule = () => {
  intentRuleDeleteTarget.value = null;
};
// 兼容历史连接里出现的自定义 provider，避免旧配置在切换成下拉后丢失。
const providerLabel = (value: string) => {
  const normalized = normalizeProviderValue(value);
  if (normalized === "openai_compatible") {
    return t("page.settings.qa.providerOptions.openaiCompatible");
  }
  if (normalized === "ollama") {
    return t("page.settings.qa.providerOptions.ollama");
  }
  if (normalized === "google_ai") {
    return t("page.settings.qa.providerOptions.googleAi");
  }
  if (normalized === "deepseek") {
    return t("page.settings.qa.providerOptions.deepSeek");
  }

  return value.trim() || t("page.settings.qa.providerOptions.custom");
};

const applyProviderValue = (value: string) => {
  const mode = resolveProviderMode(value);
  providerMode.value = mode;
  customProvider.value = mode === "custom" ? value.trim() : "";
};

const findMatchingProfileId = (settings: QaSettingsView | null, list: QaModelProfileView[]) => {
  if (!settings) {
    return "";
  }

  const normalizedProvider = normalizeProviderValue(settings.provider);
  const matched = list.find(
    (item) =>
      normalizeProviderValue(item.provider) === normalizedProvider &&
      item.base_url.trim() === settings.base_url.trim() &&
      item.api_key === settings.api_key &&
      item.model.trim() === settings.model.trim(),
  );

  return matched?.id ?? "";
};

// 从预设切回自定义时，默认把上一个 provider 带进自定义输入框，减少手工重输。
watch(providerMode, (next, prev) => {
  if (next === "custom" && prev !== "custom" && !customProvider.value.trim()) {
    customProvider.value = prev;
    return;
  }

  if (next !== "custom" && prev === "custom") {
    customProvider.value = "";
  }
});

const hasChanges = computed(() => {
  if (!savedSettings.value) {
    return false;
  }

  return (
    providerValue.value !== savedSettings.value.provider ||
    baseUrl.value.trim() !== savedSettings.value.base_url ||
    apiKey.value !== savedSettings.value.api_key ||
    model.value.trim() !== savedSettings.value.model ||
    Number(temperature.value) !== savedSettings.value.temperature ||
    Math.floor(Number(maxOutputTokens.value) || 0) !== savedSettings.value.max_output_tokens ||
    Math.floor(Number(contextChunkLimit.value) || 0) !== savedSettings.value.context_chunk_limit ||
    Math.floor(Number(contextTokenBudget.value) || 0) !== savedSettings.value.context_token_budget ||
    Math.floor(Number(minEvidenceCount.value) || 0) !== savedSettings.value.min_evidence_count ||
    Number(minRetrievalScore.value) !== savedSettings.value.min_retrieval_score ||
    currentIntentRulesJson.value !== savedIntentRulesJson.value
  );
});

const applySettings = (settings: QaSettingsView) => {
  // 修复：启用状态由默认连接和保存逻辑自动推导，读取时统一视为已启用，避免旧配置残留 false 阻断问答。
  enabled.value = true;
  applyProviderValue(settings.provider);
  baseUrl.value = settings.base_url;
  apiKey.value = settings.api_key;
  model.value = settings.model;
  temperature.value = settings.temperature;
  maxOutputTokens.value = settings.max_output_tokens;
  contextChunkLimit.value = settings.context_chunk_limit;
  contextTokenBudget.value = settings.context_token_budget;
  minEvidenceCount.value = settings.min_evidence_count;
  minRetrievalScore.value = settings.min_retrieval_score;
  intentRules.value = parseIntentRules(settings.intent_synonym_rules_json ?? "");
  savedIntentRules.value = intentRules.value.map(cloneIntentRule);
};

const applyProfile = (profile: QaModelProfileView) => {
  editingProfileId.value = profile.id;
  enabled.value = true;
  applyProviderValue(profile.provider);
  baseUrl.value = profile.base_url;
  apiKey.value = profile.api_key;
  model.value = profile.model;
};

const loadProfiles = async () => {
  profilesLoading.value = true;
  profileErrorMessage.value = "";
  profilesReady.value = false;

  try {
    profiles.value = await seekMindApi.listQaModelProfiles();
    if (profiles.value.length > 0) {
      const matchedProfileId =
        findMatchingProfileId(savedSettings.value, profiles.value) ||
        profiles.value.find((item) => item.is_default)?.id ||
        profiles.value[0]?.id ||
        "";
      if (!selectedProfileId.value) {
        selectedProfileId.value = matchedProfileId;
      }
      if (!editingProfileId.value && matchedProfileId) {
        editingProfileId.value = matchedProfileId;
      }
    }
  } catch (error) {
    profileErrorMessage.value = formatSeekMindError(error, t("page.settings.qa.profileErrorLoad"));
    console.error("[SeekMind] listQaModelProfiles failed", error);
  } finally {
    profilesReady.value = true;
    profilesLoading.value = false;
  }
};

const loadSettings = async () => {
  loading.value = true;
  errorMessage.value = "";

  try {
    const settings = await seekMindApi.getQaSettings();
    savedSettings.value = settings;
    applySettings(settings);
    await loadProfiles();
  } catch (error) {
    errorMessage.value = formatSeekMindError(error, t("page.settings.qa.error.load"));
    console.error("[SeekMind] getQaSettings failed", error);
  } finally {
    loading.value = false;
  }
};

const buildConnectionProfileName = (settings: QaSettingsView) => {
  const providerName = providerLabel(settings.provider).trim();
  const modelName = settings.model.trim();
  if (providerName && modelName) {
    return `${providerName} · ${modelName}`;
  }
  return modelName || providerName || t("page.settings.qa.profileUnnamed");
};

const syncCurrentProfileToList = async (settings: QaSettingsView) => {
  profileErrorMessage.value = "";
  profileToastMessage.value = "";

  try {
    const existing = profiles.value.find((item) => item.id === editingProfileId.value) ?? null;
    const payload: QaModelProfileUpsertView = {
      id: existing?.id ?? null,
      name: existing?.name?.trim() || buildConnectionProfileName(settings),
      provider: settings.provider,
      base_url: settings.base_url,
      api_key: settings.api_key,
      model: settings.model,
      enabled: true,
      is_default: existing?.is_default ?? false,
    };
    const saved = await seekMindApi.saveQaModelProfile(payload);
    profiles.value = [saved, ...profiles.value.filter((item) => item.id !== saved.id)];
    selectedProfileId.value = saved.id;
    editingProfileId.value = saved.id;
    profileToastMessage.value = t("page.settings.qa.profileSaved", { name: saved.name });
  } catch (error) {
    profileErrorMessage.value = formatSeekMindError(error, t("page.settings.qa.profileErrorSave"));
    console.error("[SeekMind] saveQaModelProfile failed", error);
  }
};

const loadProfileToForm = (profile: QaModelProfileView) => {
  selectedProfileId.value = profile.id;
  applyProfile(profile);
  profileToastMessage.value = t("page.settings.qa.profileLoaded", { name: profile.name });
};

const deleteProfile = async (profile: QaModelProfileView) => {
  profileDeleting.value = true;
  profileErrorMessage.value = "";
  profileToastMessage.value = "";

  try {
    await seekMindApi.removeQaModelProfile(profile.id);
    profiles.value = profiles.value.filter((item) => item.id !== profile.id);
    if (selectedProfileId.value === profile.id) {
      selectedProfileId.value = profiles.value.find((item) => item.is_default)?.id ?? profiles.value[0]?.id ?? "";
    }
    if (editingProfileId.value === profile.id) {
      editingProfileId.value = "";
    }
    emitQaConfigUpdated("delete-profile");
    profileToastMessage.value = t("page.settings.qa.profileDeleted", { name: profile.name });
  } catch (error) {
    profileErrorMessage.value = formatSeekMindError(error, t("page.settings.qa.profileErrorDelete"));
    console.error("[SeekMind] removeQaModelProfile failed", error);
  } finally {
    profileDeleting.value = false;
  }
};

const setDefaultProfile = async (profile: QaModelProfileView) => {
  profileErrorMessage.value = "";
  profileToastMessage.value = "";

  try {
    const saved = await seekMindApi.setDefaultQaModelProfile(profile.id);
    profiles.value = profiles.value.map((item) => ({ ...item, is_default: item.id === saved.id }));
    selectedProfileId.value = saved.id;
    // 修复：默认连接即为当前启用连接，避免默认项与可用项状态不一致。
    enabled.value = true;
    emitQaConfigUpdated("set-default-profile");
    profileToastMessage.value = t("page.settings.qa.profileDefaulted", { name: saved.name });
  } catch (error) {
    profileErrorMessage.value = formatSeekMindError(error, t("page.settings.qa.profileErrorDefault"));
    console.error("[SeekMind] setDefaultQaModelProfile failed", error);
  }
};

const buildSettingsPayload = (): QaSettingsView => ({
  // 启用状态由默认连接自动维持，这里始终按已启用保存，避免旧数据把问答入口锁死。
  enabled: true,
  provider: providerValue.value || "ollama",
  base_url: baseUrl.value.trim() || "http://127.0.0.1:11434/v1",
  api_key: apiKey.value,
  model: model.value.trim(),
  temperature: Math.max(0, Math.min(2, Number(temperature.value) || 0.2)),
  max_output_tokens: Math.max(1, Math.floor(Number(maxOutputTokens.value) || 600)),
  context_chunk_limit: Math.max(1, Math.floor(Number(contextChunkLimit.value) || 8)),
  context_token_budget: Math.max(1, Math.floor(Number(contextTokenBudget.value) || 6000)),
  min_evidence_count: Math.max(1, Math.floor(Number(minEvidenceCount.value) || 2)),
  min_retrieval_score: Math.max(-1, Math.min(1, Number(minRetrievalScore.value) || 0)),
  intent_synonym_rules_json: currentIntentRulesJson.value,
  updated_at: savedSettings.value?.updated_at ?? "",
});

const buildIntentRulesPayload = (): QaSettingsView | null => {
  if (!savedSettings.value) {
    return null;
  }

  return {
    ...savedSettings.value,
    enabled: true,
    intent_synonym_rules_json: currentIntentRulesJson.value,
  };
};

const validateIntentSynonymRules = () => {
  for (const rule of intentRules.value) {
    const isBlank = !rule.name.trim() && !rule.markersText.trim() && !rule.recallTermsText.trim() && !rule.noiseTermsText.trim();
    if (isBlank) {
      continue;
    }
    if (splitRuleTerms(rule.markersText).length === 0 || splitRuleTerms(rule.recallTermsText).length === 0) {
      errorMessage.value = t("page.settings.qa.intentRulesInvalid");
      return false;
    }
  }

  return true;
};

const validateIntentSynonymRule = (rule: IntentSynonymRuleForm) => {
  if (isIntentRuleBlank(rule)) {
    return true;
  }

  if (splitRuleTerms(rule.markersText).length === 0 || splitRuleTerms(rule.recallTermsText).length === 0) {
    errorMessage.value = t("page.settings.qa.intentRulesInvalid");
    return false;
  }

  return true;
};

const persistIntentRules = async (reason: string, messageKey: string) => {
  if (intentRulesSaving.value) {
    return;
  }

  if (!validateIntentSynonymRules()) {
    return;
  }

  const payload = buildIntentRulesPayload();
  if (!payload) {
    errorMessage.value = t("page.settings.qa.error.save");
    return;
  }

  intentRulesSaving.value = true;
  errorMessage.value = "";
  saveMessage.value = "";

  try {
    const settings = await seekMindApi.saveQaSettings(payload);
    savedSettings.value = settings;
    savedIntentRules.value = intentRules.value.map(cloneIntentRule);
    // 修复：同义词规则改为独立持久化，不再依赖 LLM 连接卡片的总保存按钮。
    emitQaConfigUpdated(reason);
    saveMessage.value = t(messageKey);
  } catch (error) {
    errorMessage.value = formatSeekMindError(error, t("page.settings.qa.error.save"));
    console.error("[SeekMind] saveIntentRules failed", { reason, error });
  } finally {
    intentRulesSaving.value = false;
  }
};

const saveIntentRule = async (ruleId: string) => {
  const rule = intentRules.value.find((item) => item.id === ruleId);
  if (!rule || isIntentRuleBlank(rule)) {
    return;
  }

  if (!validateIntentSynonymRule(rule)) {
    return;
  }

  await persistIntentRules("save-intent-rule", "page.settings.qa.intentRuleSaved");
};

const confirmRemoveIntentRule = async () => {
  const target = intentRuleDeleteTarget.value;
  if (!target) {
    return;
  }

  intentRuleDeleteTarget.value = null;
  intentRules.value = intentRules.value.filter((rule) => rule.id !== target.id);

  if (intentRules.value.length === 0) {
    savedIntentRules.value = [];
    if (savedSettings.value) {
      await persistIntentRules("delete-intent-rule", "page.settings.qa.intentRuleDeleted");
    }
    return;
  }

  await persistIntentRules("delete-intent-rule", "page.settings.qa.intentRuleDeleted");
};

const saveSettings = async () => {
  saving.value = true;
  errorMessage.value = "";
  saveMessage.value = "";

  if (!validateIntentSynonymRules()) {
    saving.value = false;
    return;
  }

  try {
    const settings = await seekMindApi.saveQaSettings(buildSettingsPayload());
    savedSettings.value = settings;
    applySettings(settings);
    savedIntentRules.value = intentRules.value.map(cloneIntentRule);
    await syncCurrentProfileToList(settings);
    // 修复：设置页保存在 KeepAlive 场景下不会触发问答页重建，必须主动广播配置更新。
    emitQaConfigUpdated("save-settings");
    saveMessage.value = t("page.settings.qa.saved");
  } catch (error) {
    errorMessage.value = formatSeekMindError(error, t("page.settings.qa.error.save"));
    console.error("[SeekMind] saveQaSettings failed", error);
  } finally {
    saving.value = false;
  }
};

const testConnection = async () => {
  testing.value = true;
  errorMessage.value = "";
  saveMessage.value = "";
  connectionResult.value = null;

  if (!validateIntentSynonymRules()) {
    testing.value = false;
    return;
  }

  try {
    const payload = buildSettingsPayload();
    const result = await seekMindApi.testQaConnection(payload);
    const settings = await seekMindApi.saveQaSettings(payload);
    savedSettings.value = settings;
    applySettings(settings);
    savedIntentRules.value = intentRules.value.map(cloneIntentRule);
    await syncCurrentProfileToList(settings);
    emitQaConfigUpdated("test-connection");
    connectionResult.value = result;
    saveMessage.value = t("page.settings.qa.connectionSaved", { message: result.message });
  } catch (error) {
    errorMessage.value = formatSeekMindError(error, t("page.settings.qa.error.connection"));
    console.error("[SeekMind] testQaConnection failed", error);
  } finally {
    testing.value = false;
  }
};

const refreshAll = async () => {
  await loadSettings();
};

watch(selectedProfileId, (next) => {
  if (!profilesReady.value) {
    return;
  }

  const profile = profiles.value.find((item) => item.id === next);
  if (!profile) {
    return;
  }

  if (editingProfileId.value !== profile.id) {
    loadProfileToForm(profile);
  }
});

onMounted(async () => {
  await refreshAll();
});
</script>

<template>
  <section class="settings-card-shell">
    <div class="settings-card-head">
      <div class="settings-card-head-left">
        <span class="settings-card-icon settings-card-icon--plain">
          <MessageSquareText :size="18" />
        </span>
        <div class="min-w-0">
          <div class="settings-card-title">{{ t("page.settings.qa.title") }}</div>
        </div>
      </div>
      <SeekMindBadge tone="default">{{ t("page.settings.qa.configured") }}</SeekMindBadge>
    </div>

    <div class="settings-card-body space-y-4">
      <SeekMindToast v-if="errorMessage" :message="errorMessage" tone="error" />
      <SeekMindToast v-if="saveMessage" :message="saveMessage" tone="success" />

      <div v-if="loading" class="settings-empty-state">
        {{ t("common.loading") }}
      </div>

      <div v-else class="space-y-2.5">
        <div class="grid gap-3 xl:grid-cols-[minmax(0,3fr)_minmax(0,2fr)] xl:items-start">
          <div class="space-y-2.5">
            <div class="grid gap-2.5 xl:grid-cols-2">
              <label class="block">
                <div class="mb-1.5 seekmind-section-label">{{ t("page.settings.qa.provider") }}</div>
                <div class="relative">
                  <select
                    v-model="providerMode"
                    class="seekmind-select w-full px-4 py-2.5 pr-10 text-sm outline-none transition"
                  >
                    <option v-for="option in providerPresets" :key="option" :value="option">
                      {{ providerLabel(option) }}
                    </option>
                  </select>
                  <ChevronDown :size="15" class="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-secondary" />
                </div>
                <input
                  v-if="providerMode === 'custom'"
                  v-model="customProvider"
                  class="mt-1.5 w-full rounded-lg border border-default bg-input px-4 py-2.5 text-sm text-primary outline-none transition focus:border-[var(--color-text-dim)] focus:bg-surface"
                  :placeholder="t('page.settings.qa.providerCustomPlaceholder')"
                />
                <div class="mt-1 seekmind-item-meta">{{ t("page.settings.qa.providerHint") }}</div>
              </label>

              <label class="block">
                <div class="mb-1.5 seekmind-section-label">{{ t("page.settings.qa.model") }}</div>
                <input
                  v-model="model"
                  class="w-full rounded-lg border border-default bg-input px-4 py-2.5 text-sm text-primary outline-none transition focus:border-[var(--color-text-dim)] focus:bg-surface"
                  :placeholder="t('page.settings.qa.modelPlaceholder')"
                />
              </label>
            </div>

            <div class="grid gap-2.5 xl:grid-cols-2">
              <label class="block">
                <div class="mb-1.5 seekmind-section-label">{{ t("page.settings.qa.baseUrl") }}</div>
                <input
                  v-model="baseUrl"
                  class="w-full rounded-lg border border-default bg-input px-4 py-2.5 text-sm text-primary outline-none transition focus:border-[var(--color-text-dim)] focus:bg-surface"
                  :placeholder="t('page.settings.qa.baseUrlPlaceholder')"
                />
              </label>

              <label class="block">
                <div class="mb-1.5 seekmind-section-label">{{ t("page.settings.qa.apiKey") }}</div>
                <input
                  v-model="apiKey"
                  type="password"
                  class="w-full rounded-lg border border-default bg-input px-4 py-2.5 text-sm text-primary outline-none transition focus:border-[var(--color-text-dim)] focus:bg-surface"
                  :placeholder="t('page.settings.qa.apiKeyPlaceholder')"
                />
              </label>
            </div>

            <div class="grid gap-2.5 xl:grid-cols-2">
              <label class="block">
                <div class="mb-1.5 flex items-center justify-between seekmind-section-label">
                  <span>{{ t("page.settings.qa.temperature") }}</span>
                  <span>{{ temperature.toFixed(2) }}</span>
                </div>
                <input v-model.number="temperature" type="range" min="0" max="2" step="0.05" class="w-full accent-accent" />
              </label>

              <label class="block">
                <div class="mb-1.5 seekmind-section-label">{{ t("page.settings.qa.maxTokens") }}</div>
                <input
                  v-model.number="maxOutputTokens"
                  type="number"
                  min="1"
                  step="1"
                  class="w-full rounded-lg border border-default bg-input px-4 py-2.5 text-sm text-primary outline-none transition focus:border-[var(--color-text-dim)] focus:bg-surface"
                />
              </label>
            </div>

            <div class="grid gap-2.5 xl:grid-cols-3">
              <label class="block">
                <div class="mb-1.5 seekmind-section-label settings-inline-help">
                  <span>{{ t("page.settings.qa.contextLimit") }}</span>
                  <button
                    type="button"
                    class="settings-help-trigger"
                    :title="t('page.settings.qa.help.contextLimit')"
                    :aria-label="t('page.settings.qa.help.contextLimit')"
                  >
                    <CircleHelp :size="14" />
                  </button>
                </div>
                <input
                  v-model.number="contextChunkLimit"
                  type="number"
                  min="1"
                  step="1"
                  class="w-full rounded-lg border border-default bg-input px-4 py-2.5 text-sm text-primary outline-none transition focus:border-[var(--color-text-dim)] focus:bg-surface"
                />
              </label>
              <label class="block">
                <div class="mb-1.5 seekmind-section-label">{{ t("page.settings.qa.tokenBudget") }}</div>
                <input
                  v-model.number="contextTokenBudget"
                  type="number"
                  min="1"
                  step="1"
                  class="w-full rounded-lg border border-default bg-input px-4 py-2.5 text-sm text-primary outline-none transition focus:border-[var(--color-text-dim)] focus:bg-surface"
                />
              </label>
              <label class="block">
                <div class="mb-1.5 seekmind-section-label">{{ t("page.settings.qa.minEvidence") }}</div>
                <input
                  v-model.number="minEvidenceCount"
                  type="number"
                  min="1"
                  step="1"
                  class="w-full rounded-lg border border-default bg-input px-4 py-2.5 text-sm text-primary outline-none transition focus:border-[var(--color-text-dim)] focus:bg-surface"
                />
              </label>
            </div>

            <label class="block">
              <div class="mb-1.5 flex items-center justify-between seekmind-section-label settings-inline-help">
                <span class="inline-flex items-center gap-1.5">
                  <span>{{ t("page.settings.qa.minRetrievalScore") }}</span>
                  <button
                    type="button"
                    class="settings-help-trigger"
                    :title="t('page.settings.qa.help.minRetrievalScore')"
                    :aria-label="t('page.settings.qa.help.minRetrievalScore')"
                  >
                    <CircleHelp :size="14" />
                  </button>
                </span>
                <span>{{ minRetrievalScore.toFixed(2) }}</span>
              </div>
              <input v-model.number="minRetrievalScore" type="range" min="-1" max="1" step="0.05" class="w-full accent-accent" />
            </label>

            <div class="flex flex-wrap items-center gap-2">
              <button
                class="inline-flex items-center gap-2 rounded-md bg-accent px-3 py-2 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-70"
                :disabled="loading || saving || testing || !hasChanges"
                @click="saveSettings"
              >
                <Save :size="15" />
                {{ saving ? t("page.settings.qa.saving") : t("page.settings.qa.save") }}
              </button>
              <button
                class="inline-flex items-center gap-2 rounded-md border border-default bg-surface px-3 py-2 text-sm font-medium text-secondary hover:bg-surface-hover disabled:cursor-not-allowed disabled:opacity-70"
                :disabled="loading || saving || testing"
                @click="refreshAll"
              >
                <RefreshCw :size="15" />
                {{ t("page.settings.qa.refresh") }}
              </button>
              <button
                class="inline-flex items-center gap-2 rounded-md border border-default bg-surface px-3 py-2 text-sm font-medium text-secondary hover:bg-surface-hover disabled:cursor-not-allowed disabled:opacity-70"
                :disabled="loading || saving || testing"
                @click="testConnection"
              >
                <Shield :size="15" />
                {{ testing ? t("page.settings.qa.testing") : t("page.settings.qa.testConnection") }}
              </button>
              <SeekMindBadge v-if="connectionStatus" :tone="connectionStatus.tone" class="shrink-0">
                {{ connectionStatus.label }}
              </SeekMindBadge>
            </div>
          </div>

          <div class="space-y-3">
            <div class="rounded-lg border border-default bg-panel px-3 py-3">
              <div class="flex items-center justify-between gap-3">
                <div class="min-w-0">
                  <div class="seekmind-section-label">{{ t("page.settings.qa.profileTitle") }}</div>
                  <div class="seekmind-item-meta mt-1">{{ t("page.settings.qa.profileDesc") }}</div>
                </div>
                <SeekMindBadge tone="default">{{ profiles.length }}</SeekMindBadge>
              </div>

              <SeekMindToast v-if="profileErrorMessage" :message="profileErrorMessage" tone="error" />
              <SeekMindToast v-if="profileToastMessage" :message="profileToastMessage" tone="success" />

              <div class="mt-2">
                <div class="qa-connection-list-scroll">
                  <div v-if="profilesLoading" class="qa-connection-list-empty">
                    {{ t("common.loading") }}
                  </div>
                  <div v-else-if="profiles.length === 0" class="qa-connection-list-empty">
                    {{ t("page.settings.qa.profileEmpty") }}
                  </div>
                  <div v-else class="space-y-1.5">
                    <div
                      v-for="profile in profiles"
                      :key="profile.id"
                      class="w-full rounded-md border px-2.5 py-2 text-left transition"
                      :class="profile.id === selectedProfileId ? 'border-accent bg-accent-soft' : profile.is_default ? 'border-default bg-accent-soft' : 'border-default bg-surface hover:border-accent'"
                    >
                      <div class="flex items-center justify-between gap-2">
                        <div class="min-w-0 flex-1 cursor-pointer" @click="selectedProfileId = profile.id">
                          <div class="flex flex-wrap items-center gap-1.5">
                            <div class="truncate text-[13px] font-medium text-primary">{{ profile.name }}</div>
                            <SeekMindBadge v-if="profile.is_default" tone="success">{{ t("page.settings.qa.profileDefault") }}</SeekMindBadge>
                          </div>
                          <div class="mt-0.5 truncate text-[11px] leading-4 text-secondary">
                            {{ providerLabel(profile.provider) }} · {{ profile.model }} · {{ profile.base_url }}
                          </div>
                        </div>
                        <div class="flex shrink-0 items-center justify-end gap-1">
                          <button
                            v-if="!profile.is_default"
                            class="inline-flex h-7 w-7 items-center justify-center rounded-md border border-default bg-surface text-secondary hover:bg-surface-hover"
                            type="button"
                            :title="t('page.settings.qa.profileSetDefault')"
                            :aria-label="t('page.settings.qa.profileSetDefault')"
                            @click.stop="setDefaultProfile(profile)"
                          >
                            <Check :size="13" />
                          </button>
                          <button
                            class="inline-flex h-7 w-7 items-center justify-center rounded-md border border-danger-soft bg-danger-soft text-danger hover:opacity-90"
                            type="button"
                            :disabled="profileDeleting"
                            :title="t('common.delete')"
                            :aria-label="t('common.delete')"
                            @click.stop="deleteProfile(profile)"
                          >
                            <Trash2 :size="13" />
                          </button>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </section>

  <section id="settings-qa-rules" class="settings-card-shell scroll-mt-4 mt-4">
    <div class="settings-card-head">
      <div class="settings-card-head-left">
        <span class="settings-card-icon settings-card-icon--plain">
          <SlidersHorizontal :size="18" />
        </span>
        <div class="min-w-0">
          <div class="settings-card-title">{{ t("page.settings.qa.intentRules") }}</div>
        </div>
      </div>
      <SeekMindBadge tone="default">{{ intentRules.length > 0 ? t("page.settings.qa.intentRulesCustom") : t("page.settings.qa.intentRulesBuiltin") }}</SeekMindBadge>
    </div>

    <div class="settings-card-body space-y-3">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div class="seekmind-item-meta">{{ t("page.settings.qa.intentRulesHint") }}</div>
        <button
          type="button"
          class="inline-flex items-center gap-2 rounded-md border border-default bg-surface px-3 py-2 text-sm font-medium text-secondary hover:bg-surface-hover"
          @click="addIntentRule"
        >
          <Plus :size="15" />
          {{ t("page.settings.qa.intentRuleAdd") }}
        </button>
      </div>

      <div v-if="intentRules.length === 0" class="rounded-lg border border-dashed border-default bg-panel px-3 py-3 text-xs leading-5 text-secondary">
        {{ t("page.settings.qa.intentRulesEmpty") }}
      </div>

      <div
        v-else
        class="qa-intent-rules-shell overflow-hidden rounded-lg border border-default bg-panel"
      >
        <div ref="intentRulesScrollRef" class="qa-intent-rules-scroll">
          <table class="qa-intent-rules-table">
            <thead>
              <tr>
                <th>{{ t("page.settings.qa.intentRuleName") }}</th>
                <th>{{ t("page.settings.qa.intentRuleMarkers") }}</th>
                <th>{{ t("page.settings.qa.intentRuleRecallTerms") }}</th>
                <th>{{ t("page.settings.qa.intentRuleNoiseTerms") }}</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="(rule, index) in intentRules" :key="rule.id">
                <td>
                  <input
                    type="text"
                    v-model="rule.name"
                    class="qa-intent-cell-editor qa-intent-cell-editor--name"
                    :placeholder="t('page.settings.qa.intentRuleNamePlaceholder')"
                    :aria-label="t('page.settings.qa.intentRuleItem', { index: index + 1 })"
                    autocomplete="off"
                    spellcheck="false"
                  />
                </td>
                <td>
                  <input
                    type="text"
                    v-model="rule.markersText"
                    class="qa-intent-cell-editor"
                    :placeholder="t('page.settings.qa.intentRuleMarkersPlaceholder')"
                    :aria-label="t('page.settings.qa.intentRuleMarkers')"
                    autocomplete="off"
                    spellcheck="false"
                  />
                </td>
                <td>
                  <input
                    type="text"
                    v-model="rule.recallTermsText"
                    class="qa-intent-cell-editor"
                    :placeholder="t('page.settings.qa.intentRuleRecallTermsPlaceholder')"
                    :aria-label="t('page.settings.qa.intentRuleRecallTerms')"
                    autocomplete="off"
                    spellcheck="false"
                  />
                </td>
                <td>
                  <input
                    type="text"
                    v-model="rule.noiseTermsText"
                    class="qa-intent-cell-editor"
                    :placeholder="t('page.settings.qa.intentRuleNoiseTermsPlaceholder')"
                    :aria-label="t('page.settings.qa.intentRuleNoiseTerms')"
                    autocomplete="off"
                    spellcheck="false"
                  />
                </td>
                <td class="qa-intent-delete-cell">
                  <div class="qa-intent-row-actions">
                    <button
                      type="button"
                      class="qa-intent-save-button"
                      :disabled="intentRulesSaving || !isIntentRuleDirty(rule)"
                      :title="t('page.settings.qa.intentRuleSave')"
                      :aria-label="t('page.settings.qa.intentRuleSave')"
                      @click="saveIntentRule(rule.id)"
                    >
                      <Save :size="13" />
                    </button>
                    <button
                      type="button"
                      class="qa-intent-delete-button"
                      :title="t('common.delete')"
                      :aria-label="t('common.delete')"
                      @click="requestRemoveIntentRule(rule)"
                    >
                      <Trash2 :size="13" />
                    </button>
                  </div>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </div>
  </section>

  <SeekMindConfirmDialog
    :visible="Boolean(intentRuleDeleteTarget)"
    :title="t('page.settings.qa.intentRuleDeleteTitle')"
    :message="t('page.settings.qa.intentRuleDeleteConfirm', { name: intentRuleDeleteTarget?.name?.trim() || t('page.settings.qa.intentRuleUntitled') })"
    :confirm-text="t('common.delete')"
    :cancel-text="t('common.cancel')"
    danger
    @confirm="confirmRemoveIntentRule"
    @cancel="cancelRemoveIntentRule"
  />
</template>

<style scoped>
.settings-card-shell {
  border: 1px solid var(--color-border);
  border-radius: 16px;
  background: linear-gradient(180deg, rgba(15, 23, 42, 0.82), rgba(10, 16, 28, 0.78));
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.035);
}

.settings-card-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 9px 14px 8px;
  border-bottom: 1px solid var(--color-border);
}

.settings-card-head-left {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}

.settings-card-icon {
  width: 24px;
  height: 24px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  color: var(--color-accent);
}

/* 修复：设置页卡头图标不再复用全局蓝色图标壳，避免浅色主题下把图形吃掉。 */
.settings-card-icon--plain {
  background: transparent;
  border: 0;
  box-shadow: none;
}

.settings-card-title {
  font-size: 14px;
  font-weight: 700;
  line-height: 1;
  letter-spacing: -0.01em;
  color: var(--color-text-primary);
}

.settings-card-body {
  padding: 10px 14px 12px;
}

.settings-inline-help {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.settings-help-trigger {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--color-text-muted);
  cursor: help;
}

.settings-empty-state {
  border: 1px dashed var(--color-border);
  border-radius: 12px;
  background: var(--color-surface);
  padding: 24px 16px;
  font-size: 12px;
  color: var(--color-text-muted);
}

.qa-connection-list-scroll {
  height: 360px;
  overflow-y: auto;
  padding-right: 2px;
}

.qa-connection-list-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 360px;
  border: 1px dashed var(--color-border);
  border-radius: 12px;
  background: var(--color-surface);
  padding: 24px 16px;
  font-size: 12px;
  color: var(--color-text-muted);
}

.qa-intent-rules-shell {
  display: flex;
  flex-direction: column;
  height: clamp(332px, 44vh, 408px);
  min-height: 332px;
}

/* 修复：规则列表改成固定高度的表格式编辑区，超出部分只在表体滚动，避免卡片跟着条目无限增长。 */
.qa-intent-rules-scroll {
  flex: 1;
  overflow-y: auto;
  min-height: 0;
}

.qa-intent-rules-table {
  width: 100%;
  border-collapse: collapse;
  table-layout: fixed;
}

.qa-intent-rules-table th,
.qa-intent-rules-table td {
  border-bottom: 1px solid var(--color-border);
  border-right: 1px solid var(--color-border);
  vertical-align: middle;
}

.qa-intent-rules-table th:last-child,
.qa-intent-rules-table td:last-child {
  border-right: 0;
}

.qa-intent-rules-table th {
  position: sticky;
  top: 0;
  z-index: 1;
  background: var(--color-panel-bg);
  padding: 8px 9px;
  text-align: center;
  font-size: 11px;
  font-weight: 700;
  color: var(--color-text-muted);
}

.qa-intent-rules-table td {
  padding: 0;
  background: var(--color-panel-bg);
}

.qa-intent-cell-editor {
  width: 100%;
  height: 40px;
  border: 0;
  border-radius: 0;
  background: transparent;
  padding: 0 10px;
  text-align: center;
  font-size: 12px;
  line-height: 40px;
  color: var(--color-text-primary);
  outline: none;
  resize: none;
  box-sizing: border-box;
  display: block;
}

.qa-intent-cell-editor--name {
  font-weight: 600;
}

.qa-intent-cell-editor::placeholder {
  color: var(--color-text-muted);
}

.qa-intent-cell-editor:focus {
  background: color-mix(in srgb, var(--color-surface) 65%, var(--color-input-bg) 35%);
}

.qa-intent-delete-cell {
  width: 72px;
  padding: 5px 6px;
  text-align: center;
}

.qa-intent-row-actions {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  width: 100%;
}

.qa-intent-save-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border: 1px solid color-mix(in srgb, var(--color-accent) 44%, var(--color-border) 56%);
  border-radius: 6px;
  background: color-mix(in srgb, var(--color-accent) 12%, var(--color-panel-bg) 88%);
  color: var(--color-accent);
}

.qa-intent-save-button:not(:disabled):hover {
  background: color-mix(in srgb, var(--color-accent) 18%, var(--color-panel-bg) 82%);
}

.qa-intent-save-button:disabled {
  opacity: 0.35;
  cursor: default;
}

.qa-intent-delete-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border: 1px solid var(--color-danger-border);
  border-radius: 6px;
  background: var(--color-danger-soft);
  color: var(--color-danger);
}

.qa-intent-delete-button:hover {
  opacity: 0.9;
}

html:not(.dark) .settings-card-shell {
  background: rgba(255, 255, 255, 0.92);
}

html:not(.dark) .settings-card-title {
  color: #0f172a;
}

html:not(.dark) .settings-card-desc {
  color: #64748b;
}

html:not(.dark) .settings-empty-state {
  background: rgba(248, 250, 252, 0.96);
  color: #64748b;
}

html:not(.dark) .qa-connection-list-empty {
  background: rgba(248, 250, 252, 0.96);
  color: #64748b;
}

@media (max-width: 768px) {
  .settings-card-head {
    padding: 8px 12px 7px;
  }

  .settings-card-body {
    padding: 10px 12px 12px;
  }

  .qa-intent-rules-shell {
    height: clamp(300px, 48vh, 380px);
    min-height: 300px;
  }

  .qa-intent-cell-editor {
    height: 38px;
    line-height: 38px;
    padding: 0 8px;
  }
}
</style>
