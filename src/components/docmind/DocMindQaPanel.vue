<script setup lang="ts">
/**
 * @author MorningSun
 * @CreatedDate 2026/06/02
 * @Description 设置页中的 AI 回答配置面板，负责模型、参数与连通性测试。
 */
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { Check, MessageSquareText, RefreshCw, Save, Shield, Trash2 } from "lucide-vue-next";
import DocMindBadge from "./DocMindBadge.vue";
import { docmindApi, formatDocmindError } from "../../services/docmindApi";
import { useInfoMessage } from "../../composables/useInfoMessage";
import type { QaConnectionTestView, QaModelProfileUpsertView, QaModelProfileView, QaSettingsView } from "../../types/docmind";

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
const profilesLoading = ref(false);
const profileSaving = ref(false);
const profileDeleting = ref(false);
const errorMessage = ref("");
const { infoMessage } = useInfoMessage();
const profileMessage = ref("");
const profileErrorMessage = ref("");
const connectionResult = ref<QaConnectionTestView | null>(null);
const profiles = ref<QaModelProfileView[]>([]);
const editingProfileId = ref("");
const profileName = ref("");

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

const applyProfile = (profile: QaModelProfileView) => {
  editingProfileId.value = profile.id;
  profileName.value = profile.name;
  enabled.value = profile.enabled;
  provider.value = profile.provider;
  baseUrl.value = profile.base_url;
  apiKey.value = profile.api_key;
  model.value = profile.model;
};

const loadProfiles = async () => {
  profilesLoading.value = true;
  profileErrorMessage.value = "";

  try {
    profiles.value = await docmindApi.listQaModelProfiles();
  } catch (error) {
    profileErrorMessage.value = formatDocmindError(error, t("page.settings.qa.profileErrorLoad"));
    console.error("[DocMind] listQaModelProfiles failed", error);
  } finally {
    profilesLoading.value = false;
  }
};

const loadSettings = async () => {
  loading.value = true;
  errorMessage.value = "";

  try {
    const settings = await docmindApi.getQaSettings();
    savedSettings.value = settings;
    applySettings(settings);
    await loadProfiles();
  } catch (error) {
    errorMessage.value = formatDocmindError(error, t("page.settings.qa.error.load"));
    console.error("[DocMind] getQaSettings failed", error);
  } finally {
    loading.value = false;
  }
};

const saveProfile = async () => {
  profileSaving.value = true;
  profileErrorMessage.value = "";
  profileMessage.value = "";

  try {
    const payload: QaModelProfileUpsertView = {
      id: editingProfileId.value || null,
      name: profileName.value.trim() || model.value.trim() || t("page.settings.qa.profileUnnamed"),
      provider: provider.value.trim() || "openai_compatible",
      base_url: baseUrl.value.trim(),
      api_key: apiKey.value,
      model: model.value.trim(),
      enabled: enabled.value,
      is_default: false,
    };
    const saved = await docmindApi.saveQaModelProfile(payload);
    profiles.value = [saved, ...profiles.value.filter((item) => item.id !== saved.id)];
    editingProfileId.value = saved.id;
    profileName.value = saved.name;
    profileMessage.value = t("page.settings.qa.profileSaved", { name: saved.name });
  } catch (error) {
    profileErrorMessage.value = formatDocmindError(error, t("page.settings.qa.profileErrorSave"));
    console.error("[DocMind] saveQaModelProfile failed", error);
  } finally {
    profileSaving.value = false;
  }
};

const loadProfileToForm = (profile: QaModelProfileView) => {
  applyProfile(profile);
  profileMessage.value = t("page.settings.qa.profileLoaded", { name: profile.name });
};

const deleteProfile = async (profile: QaModelProfileView) => {
  profileDeleting.value = true;
  profileErrorMessage.value = "";
  profileMessage.value = "";

  try {
    await docmindApi.removeQaModelProfile(profile.id);
    profiles.value = profiles.value.filter((item) => item.id !== profile.id);
    if (editingProfileId.value === profile.id) {
      editingProfileId.value = "";
      profileName.value = "";
    }
    profileMessage.value = t("page.settings.qa.profileDeleted", { name: profile.name });
  } catch (error) {
    profileErrorMessage.value = formatDocmindError(error, t("page.settings.qa.profileErrorDelete"));
    console.error("[DocMind] removeQaModelProfile failed", error);
  } finally {
    profileDeleting.value = false;
  }
};

const setDefaultProfile = async (profile: QaModelProfileView) => {
  profileErrorMessage.value = "";
  profileMessage.value = "";

  try {
    const saved = await docmindApi.setDefaultQaModelProfile(profile.id);
    profiles.value = profiles.value.map((item) => ({ ...item, is_default: item.id === saved.id }));
    profileMessage.value = t("page.settings.qa.profileDefaulted", { name: saved.name });
  } catch (error) {
    profileErrorMessage.value = formatDocmindError(error, t("page.settings.qa.profileErrorDefault"));
    console.error("[DocMind] setDefaultQaModelProfile failed", error);
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
  <section class="settings-card-shell">
    <div class="settings-card-head">
      <div class="settings-card-head-left">
        <span class="settings-card-icon docmind-primary-icon">
          <MessageSquareText :size="18" />
        </span>
        <div class="min-w-0">
          <div class="settings-card-title">{{ t("page.settings.qa.title") }}</div>
          <div class="settings-card-desc">{{ t("page.settings.qa.desc") }}</div>
        </div>
      </div>
      <DocMindBadge :tone="enabled ? 'success' : 'default'">
        {{ enabled ? t("page.settings.qa.enabled") : t("page.settings.qa.disabled") }}
      </DocMindBadge>
    </div>

    <div class="settings-card-body space-y-4">
      <div v-if="errorMessage" class="rounded-md border border-danger-soft bg-danger-soft px-4 py-2.5 text-xs text-danger">
        {{ errorMessage }}
      </div>

      <div v-if="infoMessage" class="rounded-md border border-emerald-soft bg-emerald-soft px-4 py-2.5 text-xs text-success">
        {{ infoMessage }}
      </div>

      <div v-if="loading" class="settings-empty-state">
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

        <div class="mx-auto w-full max-w-4xl rounded-lg border border-default bg-panel px-4 py-4">
          <div class="flex items-center justify-between gap-3">
            <div>
              <div class="docmind-section-label">{{ t("page.settings.qa.profileTitle") }}</div>
              <div class="docmind-item-meta mt-1">{{ t("page.settings.qa.profileDesc") }}</div>
            </div>
            <DocMindBadge tone="default">{{ profiles.length }}</DocMindBadge>
          </div>

          <div v-if="profileErrorMessage" class="mt-3 rounded-md border border-danger-soft bg-danger-soft px-4 py-2.5 text-xs text-danger">
            {{ profileErrorMessage }}
          </div>
          <div v-if="profileMessage" class="mt-3 rounded-md border border-emerald-soft bg-emerald-soft px-4 py-2.5 text-xs text-success">
            {{ profileMessage }}
          </div>

          <div class="mt-4 grid gap-3 md:grid-cols-[minmax(0,1fr)_auto] md:items-end">
            <label class="block">
              <div class="mb-2 docmind-section-label">{{ t("page.settings.qa.profileName") }}</div>
              <input
                v-model="profileName"
                class="w-full rounded-lg border border-default bg-input px-3 py-2.5 text-sm text-primary outline-none transition focus:border-[var(--color-text-dim)] focus:bg-surface"
                :placeholder="t('page.settings.qa.profileNamePlaceholder')"
              />
            </label>
            <div class="flex items-center gap-2 md:pb-0.5">
              <button
                class="inline-flex items-center gap-2 rounded-md bg-accent px-3 py-2 text-xs font-medium text-white disabled:cursor-not-allowed disabled:opacity-70"
                :disabled="profilesLoading || profileSaving || profileDeleting || !baseUrl.trim() || !model.trim()"
                @click="saveProfile"
              >
                <Save :size="15" />
                {{ profileSaving ? t("page.settings.qa.profileSaving") : t("page.settings.qa.profileSave") }}
              </button>
            </div>
          </div>

          <div class="mt-4">
            <div v-if="profilesLoading" class="rounded-md border border-dashed border-default bg-surface px-4 py-5 text-xs text-muted">
              {{ t("common.loading") }}
            </div>
            <div v-else-if="profiles.length === 0" class="rounded-md border border-dashed border-default bg-surface px-4 py-5 text-xs text-muted">
              {{ t("page.settings.qa.profileEmpty") }}
            </div>
            <div v-else class="space-y-2">
              <div
                v-for="profile in profiles"
                :key="profile.id"
                class="w-full rounded-lg border px-3 py-2.5 text-left transition"
                :class="profile.is_default ? 'border-accent bg-accent-soft' : 'border-default bg-surface hover:border-accent'"
              >
                <div class="flex items-start justify-between gap-3">
                  <div class="min-w-0 flex-1 cursor-pointer" @click="loadProfileToForm(profile)">
                    <div class="flex flex-wrap items-center gap-2">
                      <div class="truncate text-sm font-medium text-primary">{{ profile.name }}</div>
                      <DocMindBadge v-if="profile.is_default" tone="success">{{ t("page.settings.qa.profileDefault") }}</DocMindBadge>
                      <DocMindBadge v-if="editingProfileId === profile.id" tone="default">{{ t("page.settings.qa.profileEditing") }}</DocMindBadge>
                    </div>
                    <div class="mt-1 truncate text-xs text-secondary">
                      {{ profile.provider }} · {{ profile.model }} · {{ profile.base_url }}
                    </div>
                  </div>
                  <div class="flex shrink-0 flex-wrap items-center justify-end gap-1.5">
                    <button
                      class="inline-flex items-center gap-1 rounded-md border border-default bg-surface px-2 py-1 text-[11px] text-secondary hover:bg-surface-hover"
                      type="button"
                      @click.stop="loadProfileToForm(profile)"
                    >
                      {{ t("page.settings.qa.profileLoad") }}
                    </button>
                    <button
                      v-if="!profile.is_default"
                      class="inline-flex items-center gap-1 rounded-md border border-default bg-surface px-2 py-1 text-[11px] text-secondary hover:bg-surface-hover"
                      type="button"
                      @click.stop="setDefaultProfile(profile)"
                    >
                      <Check :size="13" />
                      {{ t("page.settings.qa.profileSetDefault") }}
                    </button>
                    <button
                      class="inline-flex items-center gap-1 rounded-md border border-danger-soft bg-danger-soft px-2 py-1 text-[11px] text-danger hover:opacity-90"
                      type="button"
                      :disabled="profileDeleting"
                      @click.stop="deleteProfile(profile)"
                    >
                      <Trash2 :size="13" />
                      {{ t("common.delete") }}
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </div>
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
  padding: 18px 18px 16px;
  border-bottom: 1px solid var(--color-border);
}

.settings-card-head-left {
  display: flex;
  align-items: center;
  gap: 14px;
  min-width: 0;
}

.settings-card-icon {
  width: 44px;
  height: 44px;
  border-radius: 14px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  color: white;
}

.settings-card-title {
  font-size: 17px;
  font-weight: 850;
  letter-spacing: -0.02em;
  color: var(--color-text-primary);
}

.settings-card-desc {
  margin-top: 4px;
  font-size: 13px;
  color: var(--color-text-secondary);
}

.settings-card-body {
  padding: 16px 18px 18px;
}

.settings-empty-state {
  border: 1px dashed var(--color-border);
  border-radius: 12px;
  background: var(--color-surface);
  padding: 24px 16px;
  font-size: 12px;
  color: var(--color-text-muted);
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

@media (max-width: 768px) {
  .settings-card-head {
    padding: 16px;
  }

  .settings-card-body {
    padding: 14px 16px 16px;
  }
}
</style>
