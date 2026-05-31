<script setup lang="ts">
defineOptions({
  name: "AppQaPage",
});

import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { listen } from "@tauri-apps/api/event";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";
import {
  MessageSquareText,
  RefreshCw,
  Trash2,
  ChevronDown,
  ChevronRight,
} from "lucide-vue-next";
import DocMindBadge from "../components/docmind/DocMindBadge.vue";
import DocMindMarkdownRenderer from "../components/docmind/DocMindMarkdownRenderer.vue";
import SplitPane from "../components/SplitPane.vue";
import { docmindApi, formatDocmindError } from "../services/docmindApi";
import { buildDocumentLocationParts, formatDocumentCitation, resolveDocumentTitlePath } from "../utils/citation";
import type {
  PreviewBlockView,
  QaAnswerProgressView,
  QaAskStartView,
  QaMessageView,
  QaSessionView,
  QaSettingsView,
} from "../types/docmind";

const { t } = useI18n();
const route = useRoute();
const router = useRouter();

const qaQuestion = ref("");
const qaAnswer = ref<QaMessageView | null>(null);
const qaMessages = ref<QaMessageView[]>([]);
const qaSessions = ref<QaSessionView[]>([]);
const qaSessionId = ref("");
const qaSessionTitle = ref("");
const qaActiveSessionId = ref("");
const qaSettings = ref<QaSettingsView | null>(null);
const qaSelectedSourceId = ref("");
const qaLoading = ref(false);
const qaCancelling = ref(false);
const qaErrorMessage = ref("");
const qaInfoMessage = ref("");
const qaActiveJobId = ref("");
const expandedMessages = ref<Record<string, boolean>>({});
const loading = ref(false);
let unlistenQaProgress: null | (() => void) = null;

const emptyMarkdownBlock: PreviewBlockView = {
  block_index: 0,
  block_type: "paragraph",
  text: "",
  heading: "",
  level: null,
  page: null,
  language: "",
  markdown: "",
  html: "",
};

const currentSession = computed(
  () => qaSessions.value.find((item) => item.id === qaSessionId.value) ?? null,
);

const selectedSource = computed(() => {
  const message = qaAnswer.value;
  if (!message) {
    return null;
  }

  return message.sources.find((item) => item.source_id === qaSelectedSourceId.value) ?? message.sources[0] ?? null;
});

const selectedSourceTitlePath = computed(() =>
  resolveDocumentTitlePath({
    fileName: selectedSource.value?.file_name,
    titlePath: selectedSource.value?.title_path,
    heading: selectedSource.value?.heading,
  }),
);

const selectedSourceCitation = computed(() => {
  if (!selectedSource.value) {
    return "";
  }

  return formatDocumentCitation({
    fileName: selectedSource.value.file_name,
    titlePath: selectedSourceTitlePath.value,
    locationParts: buildDocumentLocationParts({
      page: selectedSource.value.page,
      paragraph: selectedSource.value.paragraph,
      pageLabel: t("page.appSearch.detail.pdfPage", { page: selectedSource.value.page ?? 0 }),
      paragraphLabel: t("searchResultCard.paragraph", { para: selectedSource.value.paragraph ?? 0 }),
    }),
  });
});

const panels = computed(() => {
  const items: { key: string; initialSize?: number; minSize: number; flex?: boolean }[] = [
    { key: "sidebar", initialSize: 280, minSize: 240 },
    { key: "center", minSize: 360, flex: true },
  ];
  if (selectedSource.value) {
    items.push({ key: "right", initialSize: 360, minSize: 280 });
  }
  return items;
});

const isQaConfigured = (settings: QaSettingsView | null) =>
  Boolean(settings?.enabled && settings.base_url.trim() && settings.model.trim());

const routeSessionId = computed(() => (typeof route.query.session === "string" ? route.query.session : ""));

const loadQaSettings = async () => {
  try {
    qaSettings.value = await docmindApi.getQaSettings();
  } catch (error) {
    console.error("[DocMind] getQaSettings failed", error);
  }
};

const setCurrentSession = async (sessionId: string) => {
  qaSessionId.value = sessionId;
  qaSessionTitle.value = qaSessions.value.find((item) => item.id === sessionId)?.title ?? "";

  if (!sessionId) {
    qaMessages.value = [];
    qaAnswer.value = null;
    qaSelectedSourceId.value = "";
    qaInfoMessage.value = "";
    qaErrorMessage.value = "";
    return;
  }

  const messages = await docmindApi.listQaMessages(sessionId, 100);
  qaMessages.value = messages;
  qaAnswer.value = messages[messages.length - 1] ?? null;
  qaSelectedSourceId.value = qaAnswer.value?.sources[0]?.source_id ?? "";
  qaSessionTitle.value = qaSessions.value.find((item) => item.id === sessionId)?.title ?? qaSessionTitle.value;
  expandedMessages.value = {};
};

const syncRouteSession = async (sessionId: string) => {
  if (routeSessionId.value === sessionId) {
    return;
  }

  await router.replace({
    path: "/qa",
    query: sessionId ? { session: sessionId } : {},
  });
};

const refreshSessions = async (preferLatest = false) => {
  const sessions = await docmindApi.listQaSessions(50);
  qaSessions.value = sessions;

  if (qaSessionId.value) {
    if (sessions.some((item) => item.id === qaSessionId.value)) {
      qaSessionTitle.value = sessions.find((item) => item.id === qaSessionId.value)?.title ?? "";
      return;
    }
  }

  const target = sessions[0];
  if (target && (preferLatest || qaSessionId.value)) {
    await setCurrentSession(target.id);
    await syncRouteSession(target.id);
  }
};

const deleteSession = async (sessionId: string) => {
  if (qaLoading.value || qaCancelling.value) {
    return;
  }

  await docmindApi.removeQaSession(sessionId);
  if (qaSessionId.value === sessionId) {
    await newSession();
  }
  await refreshSessions(true);
  if (routeSessionId.value === sessionId) {
    await syncRouteSession("");
  }
};

const loadInitialData = async () => {
  loading.value = true;
  try {
    await loadQaSettings();
    await refreshSessions(true);
    if (routeSessionId.value) {
      const target = qaSessions.value.find((item) => item.id === routeSessionId.value);
      if (target) {
        await setCurrentSession(target.id);
      }
    }
  } catch (error) {
    console.error("[DocMind] loadInitialData failed", error);
  } finally {
    loading.value = false;
  }
};

const ensureSession = async (title: string) => {
  if (qaSessionId.value) {
    return qaSessionId.value;
  }

  const session = await docmindApi.createQaSession(title.trim());
  qaSessions.value = [session, ...qaSessions.value.filter((item) => item.id !== session.id)];
  qaSessionId.value = session.id;
  qaSessionTitle.value = session.title;
  await syncRouteSession(session.id);
  return session.id;
};

const installQaProgressListener = async () => {
  if (unlistenQaProgress) {
    return;
  }

  unlistenQaProgress = await listen<QaAnswerProgressView>("docmind:qa:answer-progress", (event) => {
    const payload = event.payload;
    if (payload.job_id !== qaActiveJobId.value) {
      return;
    }

    const sessionId = qaActiveSessionId.value;
    const nextMessage: QaMessageView = {
      id: payload.job_id,
      session_id: sessionId,
      question: payload.question,
      answer: payload.answer,
      state: payload.state,
      sources: payload.sources,
      retrieval: payload.retrieval,
      model: payload.model,
      created_at: qaAnswer.value?.created_at ?? new Date().toISOString(),
      updated_at: payload.updated_at,
      error: payload.error ?? null,
    };

    const messageIndex = qaMessages.value.findIndex((item) => item.id === payload.job_id);
    if (messageIndex >= 0) {
      qaMessages.value.splice(messageIndex, 1, nextMessage);
    } else {
      qaMessages.value.push(nextMessage);
    }
    qaAnswer.value = nextMessage;
    qaSelectedSourceId.value = qaSelectedSourceId.value || payload.sources[0]?.source_id || "";

    if (payload.state === "searching") {
      qaLoading.value = true;
      qaInfoMessage.value = t("page.appQa.searching");
      qaErrorMessage.value = "";
      return;
    }

    if (payload.state === "generating" || payload.state === "streaming") {
      qaLoading.value = true;
      qaInfoMessage.value = payload.state === "generating" ? t("page.appQa.generating") : t("page.appQa.streaming");
      qaErrorMessage.value = "";
      return;
    }

    qaLoading.value = false;
    qaInfoMessage.value =
      payload.state === "answered"
        ? t("page.appQa.answered")
        : payload.state === "insufficient_evidence"
          ? t("page.appQa.insufficient")
          : payload.state === "cancelled"
            ? t("page.appQa.stopped")
            : "";
    qaCancelling.value = false;
    qaActiveJobId.value = "";
    void refreshSessions();
  });
};

const selectSession = async (session: QaSessionView) => {
  if (qaLoading.value || qaCancelling.value) {
    return;
  }

  await setCurrentSession(session.id);
  await syncRouteSession(session.id);
};

const newSession = async () => {
  if (qaLoading.value || qaCancelling.value) {
    return;
  }

  qaSessionId.value = "";
  qaSessionTitle.value = "";
  qaMessages.value = [];
  qaAnswer.value = null;
  qaSelectedSourceId.value = "";
  qaActiveSessionId.value = "";
  qaActiveJobId.value = "";
  qaQuestion.value = "";
  qaInfoMessage.value = "";
  qaErrorMessage.value = "";
  expandedMessages.value = {};
  await syncRouteSession("");
};

const renameSession = async (session: QaSessionView) => {
  if (qaLoading.value || qaCancelling.value) {
    return;
  }

  const nextTitle = window.prompt(t("page.appQa.renamePrompt"), session.title)?.trim();
  if (!nextTitle || nextTitle === session.title) {
    return;
  }

  await docmindApi.updateQaSessionTitle(session.id, nextTitle);
  await refreshSessions(true);
  if (qaSessionId.value === session.id) {
    qaSessionTitle.value = nextTitle;
  }
};

const runQa = async () => {
  const question = qaQuestion.value.trim();
  if (!question) {
    return;
  }

  if (qaLoading.value || qaCancelling.value) {
    return;
  }

  qaLoading.value = true;
  qaErrorMessage.value = "";
  qaInfoMessage.value = "";

  try {
    if (!isQaConfigured(qaSettings.value)) {
      qaInfoMessage.value = t("page.appQa.notConfigured");
      qaLoading.value = false;
      return;
    }

    const sessionId = await ensureSession(question);
    qaActiveSessionId.value = sessionId;
    const started: QaAskStartView = await docmindApi.askQuestion(question, [], 6, sessionId);
    qaActiveJobId.value = started.job_id;
    const startedMessage: QaMessageView = {
      ...started.status,
      session_id: sessionId,
      updated_at: started.status.created_at,
    };
    qaAnswer.value = startedMessage;
    qaMessages.value = [...qaMessages.value.filter((item) => item.id !== startedMessage.id), startedMessage];
    qaSelectedSourceId.value = startedMessage.sources[0]?.source_id ?? "";
    qaQuestion.value = "";

    if (started.status.state === "model_not_configured") {
      qaInfoMessage.value = t("page.appQa.notConfigured");
      qaLoading.value = false;
      qaErrorMessage.value = started.status.error || "";
      qaActiveJobId.value = "";
      return;
    }

    if (started.status.state === "insufficient_evidence") {
      qaInfoMessage.value = t("page.appQa.insufficient");
      qaLoading.value = false;
      qaActiveJobId.value = "";
      void refreshSessions();
      return;
    }

    qaInfoMessage.value = t("page.appQa.searching");
  } catch (error) {
    qaErrorMessage.value = formatDocmindError(error, t("page.appQa.askFailed"));
    qaLoading.value = false;
    qaActiveJobId.value = "";
  } finally {
    qaCancelling.value = false;
  }
};

const stopQa = async () => {
  if (!qaActiveJobId.value || qaCancelling.value) {
    return;
  }

  const jobId = qaActiveJobId.value;
  qaCancelling.value = true;
  qaLoading.value = false;
  qaInfoMessage.value = t("page.appQa.stopping");
  qaErrorMessage.value = "";

  try {
    await docmindApi.cancelQaQuestion(jobId);
    if (qaAnswer.value && qaAnswer.value.id === jobId) {
      qaAnswer.value = {
        ...qaAnswer.value,
        state: "cancelled",
        error: null,
      };
    }
    qaInfoMessage.value = t("page.appQa.stopped");
  } catch (error) {
    qaErrorMessage.value = error instanceof Error ? error.message : t("page.appQa.askFailed");
  } finally {
    qaActiveJobId.value = "";
    qaCancelling.value = false;
    void refreshSessions();
  }
};

const toggleMessageSources = (messageId: string) => {
  expandedMessages.value = {
    ...expandedMessages.value,
    [messageId]: !expandedMessages.value[messageId],
  };
};

const selectMessage = (message: QaMessageView) => {
  qaAnswer.value = message;
  qaSelectedSourceId.value = message.sources[0]?.source_id ?? "";
};

const selectSource = (sourceId: string) => {
  qaSelectedSourceId.value = sourceId;
};

const openSelectedQaFile = async () => {
  if (!selectedSource.value) return;
  await docmindApi.openFile(selectedSource.value.path);
};

const quickLookSelectedQaFile = async () => {
  if (!selectedSource.value) return;
  await docmindApi.quickLookFile(selectedSource.value.path);
};

const copySelectedQaPath = async () => {
  if (!selectedSource.value) return;
  await navigator.clipboard.writeText(selectedSource.value.path);
};

const copySelectedQaCitation = async () => {
  if (!selectedSource.value) return;
  await navigator.clipboard.writeText(selectedSourceCitation.value);
};

const viewQaChunks = async () => {
  if (!selectedSource.value) return;
  await router.push({ path: "/chunks", query: { path: selectedSource.value.path } });
};

onMounted(async () => {
  await installQaProgressListener();
  await loadInitialData();
});

onBeforeUnmount(() => {
  unlistenQaProgress?.();
  unlistenQaProgress = null;
});

watch(qaMessages, () => {
  if (!qaAnswer.value && qaMessages.value.length > 0) {
    qaAnswer.value = qaMessages.value[qaMessages.value.length - 1] ?? null;
  }
});

watch(routeSessionId, async (next, previous) => {
  if (next === previous) {
    return;
  }

  if (!next) {
    if (qaSessionId.value) {
      newSession();
    }
    return;
  }

  const target = qaSessions.value.find((item) => item.id === next);
  if (target && target.id !== qaSessionId.value) {
    await setCurrentSession(target.id);
  }
});
</script>

<template>
  <section class="flex h-full min-h-0 flex-col overflow-hidden bg-panel/70">
    <div class="border-b border-default bg-surface px-4 py-3">
      <div class="flex items-start justify-between gap-3">
        <div>
          <div class="docmind-section-label">{{ t("page.appQa.title") }}</div>
          <div class="docmind-item-meta mt-1">{{ t("page.appQa.subtitle") }}</div>
        </div>
        <div class="flex items-center gap-2">
          <DocMindBadge :tone="isQaConfigured(qaSettings) ? 'success' : 'default'">
            {{ isQaConfigured(qaSettings) ? t("page.appQa.ready") : t("page.appQa.notConfigured") }}
          </DocMindBadge>
          <RouterLink
            to="/settings"
            class="inline-flex items-center gap-2 rounded-md border border-default bg-panel px-3 py-1.5 text-xs font-medium text-secondary hover:bg-surface-hover"
          >
            <MessageSquareText :size="14" />
            {{ t("page.appQa.settings") }}
          </RouterLink>
        </div>
      </div>

      <div class="mt-3 grid gap-2 lg:grid-cols-[minmax(0,1fr)_auto]">
        <textarea
          v-model="qaQuestion"
          rows="3"
          class="w-full rounded-md border border-default bg-input px-3 py-2.5 text-sm text-primary outline-none transition focus:border-accent focus:bg-surface"
          :placeholder="t('page.appQa.placeholder')"
        />
        <div class="flex flex-wrap items-start gap-2 lg:flex-col lg:items-stretch">
          <button
            v-if="qaLoading || qaCancelling"
            class="inline-flex items-center justify-center gap-2 rounded-md border border-default bg-surface px-3 py-2 text-sm font-medium text-secondary hover:bg-surface-hover"
            type="button"
            @click="stopQa"
          >
            {{ qaCancelling ? t("page.appQa.stopping") : t("page.appQa.stop") }}
          </button>
          <button
            class="inline-flex items-center justify-center gap-2 rounded-md bg-accent px-3 py-2 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-70"
            :disabled="qaLoading || qaCancelling"
            type="button"
            @click="runQa"
          >
            <MessageSquareText :size="15" />
            {{ qaCancelling ? t("page.appQa.stopping") : qaLoading ? t("page.appQa.asking") : t("page.appQa.ask") }}
          </button>
          <button
            class="inline-flex items-center justify-center gap-2 rounded-md border border-default bg-panel px-3 py-2 text-sm font-medium text-secondary hover:bg-surface-hover"
            type="button"
            :disabled="qaLoading || qaCancelling"
            @click="newSession"
          >
            {{ t("page.appQa.newSession") }}
          </button>
        </div>
      </div>
    </div>

    <SplitPane :panels="panels">
      <template #sidebar>
        <aside class="flex h-full min-h-0 flex-col overflow-hidden border-r border-default bg-sidebar">
          <div class="border-b border-default px-4 py-3">
            <div class="flex items-center justify-between gap-2">
              <div>
                <div class="docmind-section-label">{{ t("page.appQa.sessions") }}</div>
                <div class="docmind-item-meta mt-1">{{ t("page.appQa.sessionDesc") }}</div>
              </div>
              <button
                class="inline-flex h-8 w-8 items-center justify-center rounded-md border border-default bg-surface text-secondary hover:bg-surface-hover"
                :title="t('common.refresh')"
                @click="refreshSessions()"
              >
                <RefreshCw :size="14" />
              </button>
            </div>
          </div>

          <div class="min-h-0 flex-1 overflow-y-auto p-3">
            <div v-if="loading" class="rounded-md border border-dashed border-default bg-surface px-4 py-6 text-xs text-muted">
              {{ t("common.loading") }}
            </div>
            <div v-else-if="qaSessions.length === 0" class="rounded-md border border-dashed border-default bg-surface px-4 py-6 text-xs text-muted">
              {{ t("page.appQa.emptySessions") }}
            </div>
            <div v-else class="space-y-2">
              <div
                v-for="session in qaSessions"
                :key="session.id"
                class="w-full rounded-lg border px-3 py-2 text-left transition"
                :class="qaSessionId === session.id ? 'border-accent bg-accent-soft' : 'border-default bg-surface hover:border-accent'"
              >
                <div class="flex items-start justify-between gap-2">
                  <button class="min-w-0 flex-1 text-left" @click="selectSession(session)">
                    <div class="truncate text-sm font-medium text-primary">{{ session.title }}</div>
                    <div class="mt-1 flex items-center gap-2 text-[11px] text-muted">
                      <DocMindBadge tone="default">{{ t("page.appQa.messageCount", { count: session.message_count }) }}</DocMindBadge>
                      <span class="truncate">{{ session.updated_at }}</span>
                    </div>
                  </button>
                  <div class="flex shrink-0 items-center gap-1">
                    <button
                      class="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted hover:bg-surface-hover hover:text-primary"
                      :title="t('page.appQa.renameSession')"
                      @click.stop="renameSession(session)"
                    >
                      <MessageSquareText :size="13" />
                    </button>
                    <button
                      class="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted hover:bg-surface-hover hover:text-danger"
                      :title="t('page.appQa.deleteSession')"
                      @click.stop="deleteSession(session.id)"
                    >
                      <Trash2 :size="13" />
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </aside>
      </template>

      <template #center>
        <section class="flex h-full min-h-0 flex-1 flex-col overflow-hidden bg-panel/70">
          <div class="flex items-center justify-between gap-3 border-b border-default bg-surface px-4 py-2">
            <div class="text-xs font-medium text-dim">
              {{ currentSession ? currentSession.title : t("page.appQa.currentSessionEmpty") }}
            </div>
            <div class="flex items-center gap-2">
              <DocMindBadge tone="default">{{ qaMessages.length }}</DocMindBadge>
              <DocMindBadge :tone="isQaConfigured(qaSettings) ? 'success' : 'default'">
                {{ isQaConfigured(qaSettings) ? t("page.appQa.enabled") : t("page.appQa.disabled") }}
              </DocMindBadge>
            </div>
          </div>

          <div class="min-h-0 flex-1 overflow-y-auto">
            <div v-if="qaErrorMessage" class="m-4 rounded-md border border-danger-soft bg-danger-soft px-4 py-3 text-sm text-danger">
              {{ qaErrorMessage }}
            </div>
            <div v-if="qaInfoMessage" class="m-4 rounded-md border border-emerald-soft bg-emerald-soft px-4 py-3 text-sm text-success">
              {{ qaInfoMessage }}
            </div>
            <div v-if="qaLoading && qaMessages.length === 0" class="m-4 rounded-md border border-dashed border-default bg-surface px-4 py-6 text-center text-xs text-muted">
              {{ t("page.appQa.loading") }}
            </div>
            <div v-else-if="qaMessages.length" class="space-y-3 p-4">
              <article
                v-for="message in qaMessages"
                :key="message.id"
                class="rounded-xl border border-default bg-surface p-4 shadow-sm transition"
                :class="qaAnswer?.id === message.id ? 'ring-1 ring-accent-soft' : ''"
                @click="selectMessage(message)"
              >
                <div class="flex items-start justify-between gap-3">
                  <div class="min-w-0">
                    <div class="docmind-section-label">{{ t("page.appQa.question") }}</div>
                    <div class="mt-1 break-words text-sm font-medium text-primary">{{ message.question }}</div>
                  </div>
                  <div class="flex items-center gap-2">
                    <DocMindBadge tone="default">{{ t(`page.appSearch.qa.state.${message.state}`) }}</DocMindBadge>
                    <DocMindBadge v-if="message.state === 'cancelled'" tone="danger">
                      {{ t("page.appSearch.qa.cancelledByUser") }}
                    </DocMindBadge>
                  </div>
                </div>

                <div class="mt-3">
                  <DocMindMarkdownRenderer
                    :block="emptyMarkdownBlock"
                    :markdown="message.answer || t('page.appQa.noAnswer')"
                  />
                </div>

                <div class="mt-4 flex flex-wrap items-center gap-2 text-xs text-dim">
                  <DocMindBadge tone="default">{{ message.model || t("common.none") }}</DocMindBadge>
                  <DocMindBadge tone="default">{{ message.created_at }}</DocMindBadge>
                  <DocMindBadge tone="default">{{ t("page.appQa.sourceCount", { count: message.sources.length }) }}</DocMindBadge>
                  <button
                    class="inline-flex items-center gap-1 rounded-full border border-default bg-badge px-2 py-0.5 text-[12px] text-secondary hover:bg-surface-hover"
                    type="button"
                    @click.stop="toggleMessageSources(message.id)"
                  >
                    <ChevronDown v-if="expandedMessages[message.id]" :size="12" />
                    <ChevronRight v-else :size="12" />
                    {{ expandedMessages[message.id] ? t("page.appQa.hideSources") : t("page.appQa.showSources") }}
                  </button>
                </div>

                <div
                  v-if="expandedMessages[message.id]"
                  class="mt-4 rounded-lg border border-default bg-panel/40 p-3"
                >
                  <div class="docmind-section-label">{{ t("page.appQa.sourceSummary") }}</div>
                  <div class="mt-3 space-y-2">
                    <button
                      v-for="source in message.sources"
                      :key="source.source_id"
                      class="w-full rounded-lg border px-3 py-2 text-left transition"
                      :class="qaSelectedSourceId === source.source_id ? 'border-accent bg-accent-soft' : 'border-default bg-surface hover:border-accent'"
                      @click.stop="selectSource(source.source_id)"
                    >
                      <div class="flex items-start justify-between gap-3">
                        <div class="min-w-0">
                          <div class="flex flex-wrap items-center gap-2">
                            <DocMindBadge tone="default">{{ source.source_id }}</DocMindBadge>
                            <span class="truncate text-sm font-medium text-primary">{{ source.file_name }}</span>
                          </div>
                          <div class="mt-1 truncate text-xs text-muted">{{ source.path }}</div>
                          <div class="mt-1 text-[11px] text-dim">{{ source.title_path || source.heading }}</div>
                        </div>
                        <div class="text-right text-xs text-dim">
                          <div>{{ Math.round(source.score * 100) }}%</div>
                          <div class="mt-1">{{ source.rank_reason }}</div>
                        </div>
                      </div>
                    </button>
                  </div>
                </div>
              </article>
            </div>
            <div v-else class="m-4 rounded-md border border-dashed border-default bg-surface px-4 py-6 text-center text-xs text-muted">
              {{ t("page.appQa.enterQuestion") }}
            </div>
          </div>
        </section>
      </template>

      <template #right>
        <aside v-if="selectedSource" class="flex h-full min-h-0 flex-col overflow-hidden border-l border-default bg-panel">
          <div class="border-b border-default px-4 py-3">
            <div class="flex items-start justify-between gap-3">
              <div class="min-w-0">
                <div class="docmind-section-label">{{ t("page.appQa.sourceDetails") }}</div>
                <div class="mt-1 truncate text-sm font-medium text-primary">{{ selectedSource.file_name }}</div>
                <div class="mt-1 break-all text-xs text-muted">{{ selectedSource.path }}</div>
              </div>
              <DocMindBadge tone="default">{{ selectedSource.source_id }}</DocMindBadge>
            </div>
          </div>

          <div class="min-h-0 flex-1 overflow-y-auto p-4 space-y-4">
            <div class="rounded-lg border border-default bg-surface p-4">
              <div class="flex flex-wrap gap-2">
                <DocMindBadge tone="default">{{ selectedSource.ext.toUpperCase() }}</DocMindBadge>
                <DocMindBadge tone="default">{{ selectedSource.page ? t("searchResultCard.page", { page: selectedSource.page }) : t("searchResultCard.paragraph", { para: selectedSource.paragraph ?? 0 }) }}</DocMindBadge>
                <DocMindBadge tone="success">{{ selectedSourceCitation || t("common.none") }}</DocMindBadge>
              </div>
              <div class="mt-4 whitespace-pre-wrap text-sm leading-7 text-secondary">
                {{ selectedSource.snippet }}
              </div>
              <p class="docmind-item-meta mt-3">{{ selectedSource.rank_reason }}</p>
            </div>

            <div class="grid grid-cols-2 gap-2">
              <button class="rounded-md border border-default bg-surface px-3 py-2 text-xs text-secondary hover:bg-surface-hover" @click="openSelectedQaFile">
                {{ t("common.openFile") }}
              </button>
              <button class="rounded-md border border-default bg-surface px-3 py-2 text-xs text-secondary hover:bg-surface-hover" @click="viewQaChunks">
                {{ t("common.viewChunks") }}
              </button>
              <button class="rounded-md border border-default bg-surface px-3 py-2 text-xs text-secondary hover:bg-surface-hover" @click="quickLookSelectedQaFile">
                {{ t("page.appSearch.detail.quickLook") }}
              </button>
              <button class="rounded-md border border-default bg-surface px-3 py-2 text-xs text-secondary hover:bg-surface-hover" @click="copySelectedQaPath">
                {{ t("page.appSearch.detail.copyPath") }}
              </button>
              <button class="col-span-2 rounded-md border border-default bg-surface px-3 py-2 text-xs text-secondary hover:bg-surface-hover" @click="copySelectedQaCitation">
                {{ t("page.appSearch.detail.copyCitation") }}
              </button>
            </div>
          </div>
        </aside>
        <aside v-else class="flex h-full min-h-0 items-center justify-center border-l border-default bg-panel px-4 text-center text-xs text-muted">
          {{ qaMessages.length ? t("page.appQa.noSourceSelected") : t("page.appQa.noSourceYet") }}
        </aside>
      </template>
    </SplitPane>
  </section>
</template>
