<script setup lang="ts">
defineOptions({
  name: "AppQaPage",
});

import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";
import {
  Plus,
  X,
  RefreshCw,
  ArrowUp,
  ClipboardCopy,
  FileDown,
  Pencil,
  SlidersHorizontal,
  Trash2,
  ChevronDown,
  ChevronRight,
} from "lucide-vue-next";
import DocMindBadge from "../components/docmind/DocMindBadge.vue";
import DocMindContextMenu from "../components/docmind/DocMindContextMenu.vue";
import type { ContextMenuItem } from "../components/docmind/DocMindContextMenu.vue";
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
const qaSessionFilter = ref("");
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
const editingSessionId = ref("");
const editingSessionTitle = ref("");
const sessionMenuVisible = ref(false);
const sessionMenuPosition = ref({ x: 0, y: 0 });
const sessionMenuTarget = ref<QaSessionView | null>(null);
const loading = ref(false);
const qaQuestionInput = ref<HTMLTextAreaElement | null>(null);
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

const qaUiStateStorageKey = "docmind.qa.uiState";

interface QaUiState {
  sessionId: string;
  answerId: string;
  selectedSourceId: string;
  expandedMessages: Record<string, boolean>;
  sessionFilter: string;
  question: string;
}

const loadSavedQaUiState = (): Partial<QaUiState> => {
  try {
    const raw = sessionStorage.getItem(qaUiStateStorageKey);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as Partial<QaUiState>;
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
};

const saveQaUiState = () => {
  const state: QaUiState = {
    sessionId: qaSessionId.value,
    answerId: qaAnswer.value?.id ?? "",
    selectedSourceId: qaSelectedSourceId.value,
    expandedMessages: expandedMessages.value,
    sessionFilter: qaSessionFilter.value,
    question: qaQuestion.value,
  };
  sessionStorage.setItem(qaUiStateStorageKey, JSON.stringify(state));
};

const currentSession = computed(
  () => qaSessions.value.find((item) => item.id === qaSessionId.value) ?? null,
);

const filteredSessions = computed(() => {
  const queryText = qaSessionFilter.value.trim().toLowerCase();
  if (!queryText) {
    return qaSessions.value;
  }

  return qaSessions.value.filter((session) => {
    const haystack = [
      session.title,
      session.created_at,
      session.updated_at,
      String(session.message_count),
    ].join(" ").toLowerCase();
    return haystack.includes(queryText);
  });
});

const sessionContextMenuItems = computed<ContextMenuItem[]>(() => [
  {
    key: "rename",
    label: t("page.appQa.renameSession"),
    icon: Pencil,
    disabled: !sessionMenuTarget.value,
    handler: () => {
      if (sessionMenuTarget.value) {
        renameSession(sessionMenuTarget.value);
      }
    },
  },
  {
    key: "copyMarkdown",
    label: t("page.appQa.copySession"),
    icon: ClipboardCopy,
    disabled: !sessionMenuTarget.value,
    handler: () => {
      if (sessionMenuTarget.value) {
        void copySessionMarkdown(sessionMenuTarget.value);
      }
    },
  },
  {
    key: "exportMarkdown",
    label: t("page.appQa.exportSession"),
    icon: FileDown,
    disabled: !sessionMenuTarget.value,
    handler: () => {
      if (sessionMenuTarget.value) {
        void exportSessionMarkdown(sessionMenuTarget.value);
      }
    },
  },
  { key: "divider-delete", label: "", divider: true },
  {
    key: "delete",
    label: t("page.appQa.deleteSession"),
    icon: Trash2,
    danger: true,
    disabled: !sessionMenuTarget.value,
    handler: () => {
      if (sessionMenuTarget.value) {
        void deleteSession(sessionMenuTarget.value.id);
      }
    },
  },
]);

const selectedSource = computed(() => {
  const message = qaAnswer.value;
  if (!message) {
    return null;
  }

  if (!qaSelectedSourceId.value) {
    return null;
  }

  return message.sources.find((item) => item.source_id === qaSelectedSourceId.value) ?? null;
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

const sessionDraftTitle = () => t("page.appQa.defaultSessionTitle");

const routeSessionId = computed(() => (typeof route.query.session === "string" ? route.query.session : ""));

const loadQaSettings = async () => {
  try {
    qaSettings.value = await docmindApi.getQaSettings();
  } catch (error) {
    console.error("[DocMind] getQaSettings failed", error);
  }
};

const setCurrentSession = async (
  sessionId: string,
  uiState: Partial<Pick<QaUiState, "answerId" | "selectedSourceId" | "expandedMessages">> = {},
) => {
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
  qaAnswer.value = messages.find((item) => item.id === uiState.answerId) ?? messages[messages.length - 1] ?? null;
  qaSelectedSourceId.value =
    qaAnswer.value?.sources.find((item) => item.source_id === uiState.selectedSourceId)?.source_id ?? "";
  qaSessionTitle.value = qaSessions.value.find((item) => item.id === sessionId)?.title ?? qaSessionTitle.value;
  expandedMessages.value = uiState.expandedMessages ?? {};
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

  const deletingCurrent = qaSessionId.value === sessionId;
  await docmindApi.removeQaSession(sessionId);
  if (deletingCurrent) {
    resetCurrentSessionState();
    await syncRouteSession("");
  }
  await refreshSessions(false);
  if (!deletingCurrent && routeSessionId.value === sessionId) {
    await syncRouteSession("");
  }
};

const clearSessionFilter = () => {
  qaSessionFilter.value = "";
};

const openSessionContextMenu = (session: QaSessionView, event: MouseEvent) => {
  sessionMenuTarget.value = session;
  sessionMenuPosition.value = { x: event.clientX, y: event.clientY };
  sessionMenuVisible.value = true;
};

const closeSelectedSource = () => {
  qaSelectedSourceId.value = "";
};

const resetCurrentSessionState = () => {
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
};

const loadInitialData = async () => {
  loading.value = true;
  try {
    const savedUiState = loadSavedQaUiState();
    qaSessionFilter.value = savedUiState.sessionFilter ?? qaSessionFilter.value;
    qaQuestion.value = savedUiState.question ?? qaQuestion.value;
    await loadQaSettings();
    await refreshSessions(false);
    const initialSessionId = routeSessionId.value || savedUiState.sessionId || "";
    if (initialSessionId) {
      const target = qaSessions.value.find((item) => item.id === initialSessionId);
      if (target) {
        await setCurrentSession(target.id, {
          answerId: savedUiState.answerId,
          selectedSourceId: savedUiState.selectedSourceId,
          expandedMessages: savedUiState.expandedMessages,
        });
      }
    } else if (qaSessions.value[0]) {
      await setCurrentSession(qaSessions.value[0].id);
      await syncRouteSession(qaSessions.value[0].id);
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

  qaSessionFilter.value = "";
  resetCurrentSessionState();
  await syncRouteSession("");
  await nextTick();
  qaQuestionInput.value?.focus();
};

const renameSession = async (session: QaSessionView) => {
  editingSessionId.value = session.id;
  editingSessionTitle.value = session.title;
};

const cancelRenameSession = () => {
  editingSessionId.value = "";
  editingSessionTitle.value = "";
};

const saveRenamedSession = async (session: QaSessionView) => {
  if (editingSessionId.value !== session.id) {
    return;
  }

  const nextTitle = editingSessionTitle.value.trim();
  cancelRenameSession();
  if (!nextTitle || nextTitle === session.title) {
    return;
  }

  try {
    await docmindApi.updateQaSessionTitle(session.id, nextTitle);
    qaSessions.value = qaSessions.value.map((item) =>
      item.id === session.id ? { ...item, title: nextTitle } : item,
    );
    if (qaSessionId.value === session.id) {
      qaSessionTitle.value = nextTitle;
    }
  } catch (error) {
    qaErrorMessage.value = formatDocmindError(error, t("page.appQa.renameFailed"));
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
    const recentQuestions = qaMessages.value
      .map((message) => message.question.trim())
      .filter(Boolean)
      .slice(-6);
    const started: QaAskStartView = await docmindApi.askQuestion(question, [], 6, sessionId, recentQuestions);
    qaActiveJobId.value = started.job_id;
    const startedMessage: QaMessageView = {
      ...started.status,
      session_id: sessionId,
      updated_at: started.status.created_at,
    };
    qaAnswer.value = startedMessage;
    qaMessages.value = [...qaMessages.value.filter((item) => item.id !== startedMessage.id), startedMessage];
    qaSelectedSourceId.value = "";
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
  qaSelectedSourceId.value = "";
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

const buildSessionMarkdown = (title: string, messages: QaMessageView[]) => {
  const lines = [`# ${title}`, ""];

  if (messages.length === 0) {
    lines.push(t("page.appQa.noAnswer"));
    return lines.join("\n");
  }

  messages.forEach((message, index) => {
    lines.push(`## ${index + 1}. ${message.question}`);
    lines.push("");
    lines.push(message.answer || t("page.appQa.noAnswer"));
    lines.push("");
    lines.push(`- ${t("page.appQa.sourceCount", { count: message.sources.length })}`);
    lines.push(`- ${t("page.appQa.messageStateLabel", { state: t(`page.appSearch.qa.state.${message.state}`) })}`);
    lines.push("");

    if (message.sources.length > 0) {
      lines.push(`### ${t("page.appQa.sourceSummary")}`);
      message.sources.forEach((source) => {
        lines.push(`- [${source.source_id}] ${source.file_name}`);
        lines.push(`  - ${source.path}`);
        if (source.title_path || source.heading) {
          lines.push(`  - ${source.title_path || source.heading}`);
        }
      });
      lines.push("");
    }
  });

  return lines.join("\n").replace(/\n{3,}/g, "\n\n").trimEnd();
};

const getSessionMarkdownPayload = async (session: QaSessionView | null = currentSession.value) => {
  const title = session?.title || qaSessionTitle.value || t("page.appQa.defaultSessionTitle");
  const messages =
    session && session.id !== qaSessionId.value
      ? await docmindApi.listQaMessages(session.id, 50)
      : qaMessages.value;

  return {
    title,
    messages,
    markdown: buildSessionMarkdown(title, messages),
  };
};

const exportSessionMarkdown = async (session: QaSessionView | null = currentSession.value) => {
  const { title, messages, markdown } = await getSessionMarkdownPayload(session);
  if (messages.length === 0) {
    return;
  }

  const safeName = title.replace(/[\\/:*?"<>|]+/g, "-").trim() || "qa-session";
  qaErrorMessage.value = "";

  try {
    const targetPath = await save({
      defaultPath: `${safeName}.md`,
      filters: [{ name: "Markdown", extensions: ["md"] }],
    });

    if (!targetPath) {
      return;
    }

    const savedPath = await docmindApi.exportQaSessionMarkdown(targetPath, markdown);
    qaInfoMessage.value = t("page.appQa.exportedMarkdown", { path: savedPath });
  } catch (error) {
    qaErrorMessage.value = formatDocmindError(error, t("page.appQa.exportFailed"));
  }
};

const copySessionMarkdown = async (session: QaSessionView | null = currentSession.value) => {
  const { messages, markdown } = await getSessionMarkdownPayload(session);
  if (messages.length === 0) {
    return;
  }

  await navigator.clipboard.writeText(markdown);
  qaInfoMessage.value = t("page.appQa.copiedMarkdown");
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
  saveQaUiState();
  unlistenQaProgress?.();
  unlistenQaProgress = null;
});

watch(qaMessages, () => {
  if (!qaAnswer.value && qaMessages.value.length > 0) {
    qaAnswer.value = qaMessages.value[qaMessages.value.length - 1] ?? null;
  }
});

watch(
  [qaSessionId, qaAnswer, qaSelectedSourceId, expandedMessages, qaSessionFilter, qaQuestion],
  saveQaUiState,
  { deep: true },
);

watch(routeSessionId, async (next, previous) => {
  if (route.path !== "/qa") {
    return;
  }

  if (next === previous) {
    return;
  }

  if (!next) {
    if (qaSessionId.value) {
      resetCurrentSessionState();
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
    <SplitPane :panels="panels">
      <template #sidebar>
        <aside class="flex h-full min-h-0 flex-col overflow-hidden border-r border-default bg-sidebar">
          <div class="border-b border-default px-4 py-3">
            <div class="flex items-center justify-between gap-2">
              <div class="docmind-section-label">{{ t("page.appQa.sessions") }}</div>
              <div class="flex items-center gap-1.5">
                <button
                  class="inline-flex h-8 w-8 items-center justify-center rounded-md bg-accent text-white shadow-sm hover:bg-accent/90"
                  :title="t('page.appQa.createSession')"
                  @click="newSession"
                >
                  <Plus :size="14" />
                </button>
                <button
                  class="inline-flex h-8 w-8 items-center justify-center rounded-md border border-default bg-surface text-secondary hover:bg-surface-hover"
                  :title="t('page.appQa.settings')"
                  @click="router.push('/settings')"
                >
                  <SlidersHorizontal :size="14" />
                </button>
                <button
                  class="inline-flex h-8 w-8 items-center justify-center rounded-md border border-default bg-surface text-secondary hover:bg-surface-hover"
                  :title="t('common.refresh')"
                  @click="refreshSessions()"
                >
                  <RefreshCw :size="14" />
                </button>
              </div>
            </div>
            <div class="mt-3 flex items-center gap-2 rounded-md border border-default bg-input px-3 py-2">
              <input
                v-model="qaSessionFilter"
                class="min-w-0 flex-1 bg-transparent text-xs text-primary outline-none placeholder:text-muted"
                :placeholder="t('page.appQa.searchSessions')"
              />
              <button
                v-if="qaSessionFilter"
                class="text-xs text-secondary hover:text-primary"
                type="button"
                @click="clearSessionFilter"
              >
                {{ t("page.appQa.clearSessionFilter") }}
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
            <div v-else-if="filteredSessions.length === 0" class="rounded-md border border-dashed border-default bg-surface px-4 py-6 text-xs text-muted">
              {{ t("page.appQa.noMatchingSessions") }}
            </div>
            <div v-else class="space-y-2">
              <div
                v-for="session in filteredSessions"
                :key="session.id"
                class="w-full rounded-lg border px-3 py-2 text-left transition"
                :class="qaSessionId === session.id ? 'border-accent bg-accent-soft' : 'border-default bg-surface hover:border-accent'"
                @contextmenu.prevent="openSessionContextMenu(session, $event)"
              >
                <div class="flex items-start justify-between gap-2">
                  <div
                    class="min-w-0 flex-1 text-left"
                    role="button"
                    tabindex="0"
                    @click="editingSessionId !== session.id && selectSession(session)"
                    @keydown.enter.prevent="editingSessionId !== session.id && selectSession(session)"
                  >
                    <input
                      v-if="editingSessionId === session.id"
                      v-model="editingSessionTitle"
                      class="w-full rounded-md border border-accent bg-input px-2 py-1 text-sm font-medium text-primary outline-none"
                      type="text"
                      autofocus
                      @click.stop
                      @keydown.enter.prevent="saveRenamedSession(session)"
                      @keydown.esc.prevent="cancelRenameSession"
                      @blur="saveRenamedSession(session)"
                    />
                    <div v-else class="truncate text-sm font-medium text-primary">{{ session.title }}</div>
                    <div class="mt-1 flex items-center gap-2 text-[11px] text-muted">
                      <DocMindBadge tone="default">{{ t("page.appQa.messageCount", { count: session.message_count }) }}</DocMindBadge>
                      <span class="truncate">{{ session.updated_at }}</span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </aside>
      </template>

      <template #center>
        <section class="flex h-full min-h-0 flex-1 flex-col overflow-hidden bg-panel/70">
          <div
            class="flex items-center justify-between gap-3 border-b border-default bg-surface px-4 py-2"
          >
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
                v-for="(message, index) in qaMessages"
                :key="message.id"
                :class="[
                  'relative',
                  index < qaMessages.length - 1 ? 'pb-6' : '',
                ]"
                @click="selectMessage(message)"
              >
                <div
                  v-if="index < qaMessages.length - 1"
                  class="absolute bottom-0 left-4 top-0 w-px bg-gradient-to-b from-accent/65 via-border to-transparent"
                />
                <div class="absolute left-4 top-4 z-10 h-3 w-3 -translate-x-1/2 rounded-full border-2 border-accent bg-surface shadow-sm" />

                <div class="pl-10">
                  <div class="flex justify-end">
                    <div class="max-w-[86%] rounded-2xl rounded-br-md bg-accent-soft px-4 py-3 shadow-sm">
                      <div class="mt-1 break-words text-sm leading-7 text-primary">{{ message.question }}</div>
                    </div>
                  </div>

                  <div class="mt-3 flex justify-start">
                    <div class="max-w-[86%] rounded-2xl rounded-bl-md border border-default bg-surface px-4 py-3 shadow-sm">
                      <div class="flex items-center justify-between gap-3">
                        <div class="flex items-center gap-2">
                          <DocMindBadge v-if="message.state !== 'answered'" tone="default">
                            {{ t(`page.appSearch.qa.state.${message.state}`) }}
                          </DocMindBadge>
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
                      <div class="mt-3 flex flex-wrap items-center gap-2 text-xs text-dim">
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
                        class="mt-4 rounded-xl border border-default bg-panel/40 p-3"
                      >
                        <div class="docmind-section-label">{{ t("page.appQa.sourceSummary") }}</div>
                        <div class="mt-3 overflow-x-auto rounded-lg border border-default bg-surface">
                          <table class="w-full min-w-[720px] table-fixed text-left text-xs">
                            <thead class="border-b border-default bg-panel/70 text-[11px] uppercase tracking-wide text-muted">
                              <tr>
                                <th class="w-16 px-3 py-2 font-medium">{{ t("page.appQa.sourceColumnId") }}</th>
                                <th class="w-[28%] px-3 py-2 font-medium">{{ t("page.appQa.sourceColumnFile") }}</th>
                                <th class="px-3 py-2 font-medium">{{ t("page.appQa.sourceColumnLocation") }}</th>
                                <th class="w-24 px-3 py-2 font-medium">{{ t("page.appQa.sourceColumnScore") }}</th>
                                <th class="w-[22%] px-3 py-2 font-medium">{{ t("page.appQa.sourceColumnReason") }}</th>
                              </tr>
                            </thead>
                            <tbody class="divide-y divide-light">
                              <tr
                                v-for="source in message.sources"
                                :key="source.source_id"
                                class="cursor-pointer transition"
                                :class="qaSelectedSourceId === source.source_id ? 'bg-accent-soft' : 'hover:bg-surface-hover'"
                                @click.stop="selectSource(source.source_id)"
                              >
                                <td class="px-3 py-2 align-top">
                                  <DocMindBadge tone="default">{{ source.source_id }}</DocMindBadge>
                                </td>
                                <td class="truncate px-3 py-2 align-top font-medium text-primary">
                                  {{ source.file_name }}
                                </td>
                                <td class="truncate px-3 py-2 align-top text-secondary">
                                  {{ source.title_path || source.heading || source.path }}
                                </td>
                                <td class="px-3 py-2 align-top text-secondary">
                                  {{ Math.round(source.score * 100) }}%
                                </td>
                                <td class="truncate px-3 py-2 align-top text-dim">
                                  {{ source.rank_reason }}
                                </td>
                              </tr>
                            </tbody>
                          </table>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              </article>
            </div>
            <div v-else class="m-4 rounded-md border border-dashed border-default bg-surface px-4 py-6 text-center text-xs text-muted">
              {{ t("page.appQa.enterQuestion") }}
            </div>
          </div>

          <div class="px-4 py-3">
            <div class="rounded-2xl border border-default bg-input px-3 py-2 shadow-sm">
              <textarea
                ref="qaQuestionInput"
                v-model="qaQuestion"
                rows="2"
                class="min-h-[44px] w-full resize-none border-0 bg-transparent px-1 py-1 text-sm leading-6 text-primary outline-none placeholder:text-muted"
                :placeholder="t('page.appQa.placeholder')"
                @keydown.enter.exact.prevent="runQa"
              />
              <div class="mt-1.5 flex items-end justify-between gap-3">
                <div class="pb-0.5 text-[11px] leading-4 text-muted">
                  {{ t("page.appQa.sendHint") }}
                </div>
                <button
                  v-if="qaLoading || qaCancelling"
                  class="inline-flex h-10 w-10 items-center justify-center rounded-full border border-default bg-surface text-secondary hover:bg-surface-hover"
                  type="button"
                  @click="stopQa"
                >
                  <X :size="16" />
                </button>
                <button
                  v-else
                  class="inline-flex h-10 w-10 items-center justify-center rounded-full bg-accent text-white shadow-sm transition hover:bg-accent/90 disabled:cursor-not-allowed disabled:opacity-70"
                  :disabled="qaLoading || qaCancelling || !qaQuestion.trim()"
                  type="button"
                  @click="runQa"
                >
                  <ArrowUp :size="16" />
                </button>
              </div>
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
              <div class="flex items-center gap-2">
                <DocMindBadge tone="default">{{ selectedSource.source_id }}</DocMindBadge>
                <button
                  class="inline-flex h-8 w-8 items-center justify-center rounded-md border border-default bg-surface text-secondary hover:bg-surface-hover hover:text-primary"
                  type="button"
                  :title="t('common.close')"
                  @click="closeSelectedSource"
                >
                  <X :size="14" />
                </button>
              </div>
            </div>
          </div>

          <div class="min-h-0 flex-1 overflow-y-auto p-4 space-y-4">
            <div class="rounded-lg border border-default bg-surface px-3 py-2.5">
              <div class="flex items-center justify-between gap-3">
                <div class="docmind-section-label">{{ t("page.appQa.sourceMeta") }}</div>
                <div class="flex shrink-0 items-center gap-1.5">
                  <DocMindBadge tone="default">{{ selectedSource.ext.toUpperCase() }}</DocMindBadge>
                  <DocMindBadge tone="default">
                    {{ selectedSource.page ? t("searchResultCard.page", { page: selectedSource.page }) : t("searchResultCard.paragraph", { para: selectedSource.paragraph ?? 0 }) }}
                  </DocMindBadge>
                </div>
              </div>
              <div class="mt-2 truncate rounded-md bg-panel/70 px-2.5 py-1.5 text-xs leading-5 text-secondary" :title="selectedSourceCitation">
                {{ selectedSourceCitation || t("common.none") }}
              </div>
            </div>

            <div class="rounded-lg border border-default bg-surface p-4">
              <div class="docmind-section-label">{{ t("page.appQa.referenceSnippet") }}</div>
              <div class="mt-3 whitespace-pre-wrap rounded-md border border-default bg-panel/70 px-3 py-3 text-sm leading-7 text-secondary">
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
    <DocMindContextMenu
      v-if="sessionMenuVisible"
      :items="sessionContextMenuItems"
      :x="sessionMenuPosition.x"
      :y="sessionMenuPosition.y"
      @close="sessionMenuVisible = false"
    />
  </section>
</template>
