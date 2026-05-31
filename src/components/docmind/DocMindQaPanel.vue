<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { MessageSquareText, RefreshCw, Save, Shield } from "lucide-vue-next";
import DocMindBadge from "./DocMindBadge.vue";
import { docmindApi, formatDocmindError } from "../../services/docmindApi";
import type { QaConnectionTestView, QaSettingsView } from "../../types/docmind";

const { t } = useI18n();

const savedSettings = ref<QaSettingsView | null>(null);
const enabled = ref(false);
const provider = ref("openai_compatible");
const baseUrl = ref("");
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
const errorMessage = ref("");
const infoMessage = ref("");
const connectionResult = ref<QaConnectionTestView | null>(null);

const hasChanges = computed(() => {
  if (!savedSettings.value) {
    return false;
  }

  return (
    enabled.value !== savedSettings.value.enabled ||
    provider.value.trim() !== savedSettings.value.provider ||
    baseUrl.value.trim() !== savedSettings.value.base_url ||
    apiKey.value !== savedSettings.value.api_key ||
    model.value.trim() !== savedSettings.value.model ||
    Number(temperature.value) !== savedSettings.value.temperature ||
    Math.floor(Number(maxOutputTokens.value) || 0) !== savedSettings.value.max_output_tokens ||
    Math.floor(Number(contextChunkLimit.value) || 0) !== savedSettings.value.context_chunk_limit ||
    Math.floor(Number(contextTokenBudget.value) || 0) !== savedSettings.value.context_token_budget ||
    Math.floor(Number(minEvidenceCount.value) || 0) !== savedSettings.value.min_evidence_count ||
    Number(minRetrievalScore.value) !== savedSettings.value.min_retrieval_score
  );
});

const applySettings = (settings: QaSettingsView) => {
  enabled.value = settings.enabled;
  provider.value = settings.provider;
  baseUrl.value = settings.base_url;
  apiKey.value = settings.api_key;
  model.value = settings.model;
  temperature.value = settings.temperature;
  maxOutputTokens.value = settings.max_output_tokens;
  contextChunkLimit.value = settings.context_chunk_limit;
  contextTokenBudget.value = settings.context_token_budget;
  minEvidenceCount.value = settings.min_evidence_count;
  minRetrievalScore.value = settings.min_retrieval_score;
};

const loadSettings = async () => {
  loading.value = true;
  errorMessage.value = "";

  try {
    const settings = await docmindApi.getQaSettings();
    savedSettings.value = settings;
    applySettings(settings);
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.settings.qa.error.load"));
    console.error("[DocMind] getQaSettings failed", error);
  } finally {
    loading.value = false;
  }
};

const buildSettingsPayload = (): QaSettingsView => ({
  enabled: enabled.value,
  provider: provider.value.trim() || "openai_compatible",
  base_url: baseUrl.value.trim(),
  api_key: apiKey.value,
  model: model.value.trim(),
  temperature: Math.max(0, Math.min(2, Number(temperature.value) || 0.2)),
  max_output_tokens: Math.max(1, Math.floor(Number(maxOutputTokens.value) || 600)),
  context_chunk_limit: Math.max(1, Math.floor(Number(contextChunkLimit.value) || 8)),
  context_token_budget: Math.max(1, Math.floor(Number(contextTokenBudget.value) || 6000)),
  min_evidence_count: Math.max(1, Math.floor(Number(minEvidenceCount.value) || 2)),
  min_retrieval_score: Math.max(-1, Math.min(1, Number(minRetrievalScore.value) || 0)),
  updated_at: savedSettings.value?.updated_at ?? "",
});

const saveSettings = async () => {
  saving.value = true;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    const settings = await docmindApi.saveQaSettings(buildSettingsPayload());
    savedSettings.value = settings;
    applySettings(settings);
    infoMessage.value = t("page.settings.qa.saved");
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.settings.qa.error.save"));
    console.error("[DocMind] saveQaSettings failed", error);
  } finally {
    saving.value = false;
  }
};

const testConnection = async () => {
  testing.value = true;
  errorMessage.value = "";
  infoMessage.value = "";
  connectionResult.value = null;

  try {
    const payload = buildSettingsPayload();
    const result = await docmindApi.testQaConnection(payload);
    const settings = await docmindApi.saveQaSettings(payload);
    savedSettings.value = settings;
    applySettings(settings);
    connectionResult.value = result;
    infoMessage.value = t("page.settings.qa.connectionSaved", { message: result.message });
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.settings.qa.error.connection"));
    console.error("[DocMind] testQaConnection failed", error);
  } finally {
    testing.value = false;
  }
};

const refreshAll = async () => {
  await loadSettings();
};

onMounted(async () => {
  await refreshAll();
});
</script>

<template>
  <section class="rounded-lg border border-default bg-surface">
        <div class="flex items-center justify-between border-b border-default px-4 py-2.5">
      <div>
        <div class="docmind-section-label">{{ t("page.settings.qa.title") }}</div>
        <div class="docmind-item-meta mt-1">{{ t("page.settings.qa.desc") }}</div>
      </div>
      <DocMindBadge :tone="enabled ? 'success' : 'default'">
        {{ enabled ? t("page.settings.qa.enabled") : t("page.settings.qa.disabled") }}
      </DocMindBadge>
    </div>

    <div class="space-y-4 p-4">
      <div v-if="errorMessage" class="rounded-md border border-danger-soft bg-danger-soft px-4 py-2.5 text-xs text-danger">
        {{ errorMessage }}
      </div>

      <div v-if="infoMessage" class="rounded-md border border-emerald-soft bg-emerald-soft px-4 py-2.5 text-xs text-success">
        {{ infoMessage }}
      </div>

      <div v-if="loading" class="rounded-md border border-dashed border-default bg-surface px-4 py-6 text-xs text-muted">
        {{ t("common.loading") }}
      </div>

      <div v-else class="space-y-4">
        <div class="grid gap-4 xl:grid-cols-[160px_minmax(0,1fr)] xl:items-start">
          <div>
            <div class="docmind-section-label">{{ t("page.settings.qa.enableLabel") }}</div>
            <div class="docmind-item-meta mt-1">{{ t("page.settings.qa.enableHint") }}</div>
          </div>
          <label class="inline-flex items-center gap-2 text-sm text-secondary">
            <input v-model="enabled" type="checkbox" class="h-4 w-4 rounded border-default text-accent accent-accent" />
            {{ enabled ? t("page.settings.qa.enabled") : t("page.settings.qa.disabled") }}
          </label>
        </div>

        <div class="grid gap-4 xl:grid-cols-2">
          <label class="block">
            <div class="mb-2 docmind-section-label">{{ t("page.settings.qa.provider") }}</div>
            <input
              v-model="provider"
              class="w-full rounded-lg border border-default bg-input px-4 py-3 text-sm text-primary outline-none transition focus:border-[var(--color-text-dim)] focus:bg-surface"
              :placeholder="t('page.settings.qa.providerPlaceholder')"
            />
          </label>

          <label class="block">
            <div class="mb-2 docmind-section-label">{{ t("page.settings.qa.model") }}</div>
            <input
              v-model="model"
              class="w-full rounded-lg border border-default bg-input px-4 py-3 text-sm text-primary outline-none transition focus:border-[var(--color-text-dim)] focus:bg-surface"
              :placeholder="t('page.settings.qa.modelPlaceholder')"
            />
          </label>
        </div>

        <div class="grid gap-4 xl:grid-cols-2">
          <label class="block">
            <div class="mb-2 docmind-section-label">{{ t("page.settings.qa.baseUrl") }}</div>
            <input
              v-model="baseUrl"
              class="w-full rounded-lg border border-default bg-input px-4 py-3 text-sm text-primary outline-none transition focus:border-[var(--color-text-dim)] focus:bg-surface"
              :placeholder="t('page.settings.qa.baseUrlPlaceholder')"
            />
          </label>

          <label class="block">
            <div class="mb-2 docmind-section-label">{{ t("page.settings.qa.apiKey") }}</div>
            <input
              v-model="apiKey"
              type="password"
              class="w-full rounded-lg border border-default bg-input px-4 py-3 text-sm text-primary outline-none transition focus:border-[var(--color-text-dim)] focus:bg-surface"
              :placeholder="t('page.settings.qa.apiKeyPlaceholder')"
            />
          </label>
        </div>

        <div class="grid gap-4 xl:grid-cols-2">
          <label class="block">
            <div class="mb-2 flex items-center justify-between docmind-section-label">
              <span>{{ t("page.settings.qa.temperature") }}</span>
              <span>{{ temperature.toFixed(2) }}</span>
            </div>
            <input v-model.number="temperature" type="range" min="0" max="2" step="0.05" class="w-full accent-accent" />
          </label>

          <label class="block">
            <div class="mb-2 docmind-section-label">{{ t("page.settings.qa.maxTokens") }}</div>
            <input
              v-model.number="maxOutputTokens"
              type="number"
              min="1"
              step="1"
              class="w-full rounded-lg border border-default bg-input px-4 py-3 text-sm text-primary outline-none transition focus:border-[var(--color-text-dim)] focus:bg-surface"
            />
          </label>
        </div>

        <div class="grid gap-4 xl:grid-cols-3">
          <label class="block">
            <div class="mb-2 docmind-section-label">{{ t("page.settings.qa.contextLimit") }}</div>
            <input
              v-model.number="contextChunkLimit"
              type="number"
              min="1"
              step="1"
              class="w-full rounded-lg border border-default bg-input px-4 py-3 text-sm text-primary outline-none transition focus:border-[var(--color-text-dim)] focus:bg-surface"
            />
          </label>
          <label class="block">
            <div class="mb-2 docmind-section-label">{{ t("page.settings.qa.tokenBudget") }}</div>
            <input
              v-model.number="contextTokenBudget"
              type="number"
              min="1"
              step="1"
              class="w-full rounded-lg border border-default bg-input px-4 py-3 text-sm text-primary outline-none transition focus:border-[var(--color-text-dim)] focus:bg-surface"
            />
          </label>
          <label class="block">
            <div class="mb-2 docmind-section-label">{{ t("page.settings.qa.minEvidence") }}</div>
            <input
              v-model.number="minEvidenceCount"
              type="number"
              min="1"
              step="1"
              class="w-full rounded-lg border border-default bg-input px-4 py-3 text-sm text-primary outline-none transition focus:border-[var(--color-text-dim)] focus:bg-surface"
            />
          </label>
        </div>

        <label class="block">
          <div class="mb-2 flex items-center justify-between docmind-section-label">
            <span>{{ t("page.settings.qa.minRetrievalScore") }}</span>
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
          <DocMindBadge tone="default">{{ t("page.settings.qa.localNotice") }}</DocMindBadge>
        </div>

        <div class="rounded-lg border border-default bg-panel px-4 py-3 text-sm text-secondary">
          {{ t("page.settings.qa.privacyHint") }}
        </div>

        <div v-if="connectionResult" class="rounded-lg border border-default bg-panel px-4 py-3 text-sm text-secondary">
          <div class="docmind-section-label">{{ t("page.settings.qa.connectionResult") }}</div>
          <div class="mt-1">{{ connectionResult.message }}</div>
        </div>

        <div class="rounded-lg border border-default bg-panel px-4 py-3">
          <div class="flex items-center justify-between gap-3">
            <div class="min-w-0">
              <div class="docmind-section-label">{{ t("page.settings.qa.sessionEntryTitle") }}</div>
              <div class="docmind-item-meta mt-1">{{ t("page.settings.qa.sessionEntryDesc") }}</div>
            </div>
            <RouterLink
              to="/qa"
              class="inline-flex shrink-0 items-center gap-2 rounded-md border border-default bg-surface px-3 py-2 text-sm font-medium text-secondary hover:bg-surface-hover"
            >
              <MessageSquareText :size="15" />
              {{ t("page.settings.qa.openQa") }}
            </RouterLink>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>
