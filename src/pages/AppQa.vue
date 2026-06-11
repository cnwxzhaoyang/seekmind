/**
 * @author MorningSun
 * @CreatedDate 2026/06/03
 * @Description 问答面板，管理会话、回答内容与引用详情。
 */
<script setup lang="ts">
defineOptions({
  name: "AppQaPage",
});

import { computed, nextTick, onActivated, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";
import SeekMindToast from "../components/SeekMind/SeekMindToast.vue";
import {
  Plus,
  X,
  RefreshCw,
  ArrowUp,
  ClipboardCopy,
  BookMarked,
  Eye,
  FileDown,
  FolderPlus,
  SquareArrowOutUpRight,
  Pencil,
  SlidersHorizontal,
  Trash2,
  ChevronDown,
  ChevronRight,
} from "lucide-vue-next";
import SeekMindBadge from "../components/SeekMind/SeekMindBadge.vue";
import SeekMindDetailPanel from "../components/SeekMind/SeekMindDetailPanel.vue";
import SeekMindDetailSection from "../components/SeekMind/SeekMindDetailSection.vue";
import SeekMindCollectionPicker from "../components/SeekMind/SeekMindCollectionPicker.vue";
import SeekMindContextMenu from "../components/SeekMind/SeekMindContextMenu.vue";
import type { ContextMenuItem } from "../components/SeekMind/SeekMindContextMenu.vue";
import SeekMindMarkdownRenderer from "../components/SeekMind/SeekMindMarkdownRenderer.vue";
import SeekMindPreviewBlockRenderer from "../components/SeekMind/SeekMindPreviewBlockRenderer.vue";
import SplitPane from "../components/SplitPane.vue";
import { seekMindApi, formatSeekMindError } from "../services/seekMindApi";
import { listenQaConfigUpdated } from "../utils/qaConfigEvents";
import { buildDocumentLocationParts, formatDocumentCitation, resolveDocumentTitlePath } from "../utils/citation";
import type {
  PreviewBlockView,
  ChunkView,
  CollectionView,
  CollectionItemInput,
  QaAnswerProgressView,
  QaAskStartView,
  QaMessageView,
  QaModelProfileView,
  QaSessionView,
  QaSettingsView,
  QaSourceView,
} from "../types/SeekMind";

const { t, locale } = useI18n();
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
const qaModelProfiles = ref<QaModelProfileView[]>([]);
const qaProfileChoice = ref("__current__");
const collections = ref<CollectionView[]>([]);
const collectionPickerVisible = ref(false);
const collectionPickerTarget = ref<QaSourceView | null>(null);
const collectionPickerMode = ref<"qa_source" | "document">("qa_source");
const collectionPickerLoading = ref(false);
const expandedMessages = ref<Record<string, boolean>>({});
const editingSessionId = ref("");
const editingSessionTitle = ref("");
const sessionMenuVisible = ref(false);
const sessionMenuPosition = ref({ x: 0, y: 0 });
const sessionMenuTarget = ref<QaSessionView | null>(null);
const sourceMenuVisible = ref(false);
const sourceMenuPosition = ref({ x: 0, y: 0 });
const sourceMenuTarget = ref<QaSourceView | null>(null);
const loading = ref(false);
const loadingSelectedSourcePreview = ref(false);
const qaQuestionInput = ref<HTMLTextAreaElement | null>(null);
const qaChatScrollEl = ref<HTMLElement | null>(null);
const chatShouldFollowLatest = ref(true);
const selectedSourceHydratedPreviewBlocks = ref<PreviewBlockView[]>([]);
let selectedSourcePreviewRequestId = 0;
let unlistenQaProgress: null | (() => void) = null;
let unlistenQaConfigUpdated: null | (() => void) = null;

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

const qaUiStateStorageKey = "seekmind.qa.uiState";

interface QaUiState {
  sessionId: string;
  answerId: string;
  selectedSourceId: string;
  expandedMessages: Record<string, boolean>;
  sessionFilter: string;
  question: string;
  profileChoice: string;
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
    profileChoice: qaProfileChoice.value,
  };
  sessionStorage.setItem(qaUiStateStorageKey, JSON.stringify(state));
};

const isChatNearBottom = () => {
  const el = qaChatScrollEl.value;
  if (!el) {
    return true;
  }

  return el.scrollHeight - el.scrollTop - el.clientHeight < 120;
};

const scrollChatToBottom = async (force = false) => {
  await nextTick();
  const el = qaChatScrollEl.value;
  if (!el) {
    return;
  }

  if (!force && !chatShouldFollowLatest.value) {
    return;
  }

  el.scrollTop = el.scrollHeight;
};

const handleChatScroll = () => {
  chatShouldFollowLatest.value = isChatNearBottom();
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

const sourceContextMenuItems = computed<ContextMenuItem[]>(() => [
  {
    key: "open",
    label: t("common.openFile"),
    icon: SquareArrowOutUpRight,
    disabled: !sourceMenuTarget.value,
    handler: () => {
      void openSelectedQaFile();
    },
  },
  {
    key: "viewChunks",
    label: t("common.viewChunks"),
    icon: SlidersHorizontal,
    disabled: !sourceMenuTarget.value,
    handler: () => {
      void viewQaChunks();
    },
  },
  {
    key: "quickLook",
    label: t("page.appSearch.detail.quickLook"),
    icon: Eye,
    disabled: !sourceMenuTarget.value,
    handler: () => {
      void quickLookSelectedQaFile();
    },
  },
  {
    key: "copyPath",
    label: t("page.appSearch.detail.copyPath"),
    icon: ClipboardCopy,
    disabled: !sourceMenuTarget.value,
    handler: () => {
      void copySelectedQaPath();
    },
  },
  {
    key: "addToCollection",
    label: t("page.collections.addToCollection"),
    icon: FolderPlus,
    disabled: !sourceMenuTarget.value,
    handler: () => {
      if (sourceMenuTarget.value) {
        void openSourceCollectionPicker(sourceMenuTarget.value);
      }
    },
  },
  {
    key: "collectDocument",
    label: t("page.collections.collectDocument"),
    icon: BookMarked,
    disabled: !sourceMenuTarget.value,
    handler: () => {
      if (sourceMenuTarget.value) {
        void openSourceCollectionPicker(sourceMenuTarget.value, "document");
      }
    },
  },
  {
    key: "copyCitation",
    label: t("page.appSearch.detail.copyCitation"),
    icon: ClipboardCopy,
    disabled: !sourceMenuTarget.value,
    handler: () => {
      void copySelectedQaCitation();
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

const resolveSourceChunk = (chunks: ChunkView[], source: QaSourceView) => {
  const byId = chunks.find((chunk) => chunk.id === source.chunk_id);
  if (byId) {
    return byId;
  }

  const sourceSnippet = source.snippet.trim();
  if (!sourceSnippet) {
    return null;
  }

  return chunks.find((chunk) => {
    const chunkSnippet = chunk.snippet.trim();
    return chunkSnippet.includes(sourceSnippet) || sourceSnippet.includes(chunkSnippet);
  }) ?? null;
};

const selectedSourcePreviewBlocks = computed(() => {
  const sourceBlocks = selectedSource.value?.preview_blocks ?? [];
  return sourceBlocks.length > 0 ? sourceBlocks : selectedSourceHydratedPreviewBlocks.value;
});

const loadSelectedSourcePreviewBlocks = async (source: QaSourceView | null) => {
  const requestId = ++selectedSourcePreviewRequestId;
  selectedSourceHydratedPreviewBlocks.value = [];

  if (!source || source.preview_blocks?.length) {
    return;
  }

  loadingSelectedSourcePreview.value = true;
  try {
    const chunks = await seekMindApi.listDocumentChunks(source.path);
    if (requestId !== selectedSourcePreviewRequestId) {
      return;
    }

    const matchedChunk = resolveSourceChunk(chunks, source);
    selectedSourceHydratedPreviewBlocks.value = matchedChunk?.preview_blocks ?? [];
    console.debug("[SeekMind] qa source preview blocks loaded", {
      sourceId: source.source_id,
      chunkId: source.chunk_id,
      blockCount: selectedSourceHydratedPreviewBlocks.value.length,
    });
  } catch (error) {
    console.warn("[SeekMind] qa source preview blocks load failed", {
      sourceId: source.source_id,
      chunkId: source.chunk_id,
      path: source.path,
      error,
    });
  } finally {
    if (requestId === selectedSourcePreviewRequestId) {
      loadingSelectedSourcePreview.value = false;
    }
  }
};

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

const messageCitationSourceIds = (message: QaMessageView) =>
  message.sources.map((source) => source.source_id).filter(Boolean);

const qaCurrentModelLabel = computed(() => qaSettings.value?.model.trim() || t("common.none"));

const qaSelectedProfile = computed(() => {
  if (qaProfileChoice.value === "__current__") {
    return null;
  }

  return qaModelProfiles.value.find((item) => item.id === qaProfileChoice.value) ?? null;
});

const qaSelectedModel = computed(() => qaSelectedProfile.value?.model.trim() || qaSettings.value?.model.trim() || "");

// 修复：模型连接的启用态已经迁移到默认/保存逻辑，不再在问答页按 enabled 过滤，避免旧配置被隐藏。
const qaModelOptions = computed(() => qaModelProfiles.value);

// 失败态要区分“模型不可用”和“引用校验失败”，避免把校验问题误报成服务故障。
const isCitationValidationFailure = (message: QaMessageView) =>
  Boolean(
    message.error?.includes("来源标注") ||
      message.error?.includes("校验") ||
      message.warning?.includes("来源标注") ||
      message.warning?.includes("句子缺少"),
  );

const messageFailureTitle = (message: QaMessageView) =>
  isCitationValidationFailure(message)
    ? t("page.appQa.citationValidationFailed")
    : t("page.appQa.modelUnavailable");

const messageFailureHint = (message: QaMessageView) =>
  isCitationValidationFailure(message)
    ? message.error?.trim() || message.warning?.trim() || t("page.appQa.citationValidationHint")
    : message.error?.trim() || t("page.appQa.modelUnavailableHint");

const messageBodyMarkdown = (message: QaMessageView) => {
  if (message.answer.trim()) {
    return message.answer;
  }
  return t("page.appQa.noAnswer");
};

const messageBodyPlainText = (message: QaMessageView) => {
  if (message.answer.trim()) {
    return message.answer;
  }
  return isMessageStreaming(message) ? "" : t("page.appQa.noAnswer");
};

const isMessageStreaming = (message: QaMessageView) =>
  ["searching", "running", "generating", "streaming", "verifying"].includes(message.state);

const hasMessageAnswer = (message: QaMessageView) => message.answer.trim().length > 0;

const isMessagePending = (message: QaMessageView) =>
  isMessageStreaming(message) && !hasMessageAnswer(message);

const shouldShowMessageBody = (message: QaMessageView) =>
  hasMessageAnswer(message) || !isMessageStreaming(message);

const shouldShowMessageMeta = (message: QaMessageView) =>
  hasMessageAnswer(message) || !isMessageStreaming(message);

const formatLocalDateTime = (value: string) => {
  if (!value.trim()) {
    return t("common.none");
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }

  return new Intl.DateTimeFormat(locale.value, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).format(date);
};

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

const isQaConfigured = (settings: QaSettingsView | null) => {
  const activeBaseUrl = qaSelectedProfile.value?.base_url?.trim() || settings?.base_url.trim() || "";
  const activeModel = qaSelectedModel.value.trim();
  return Boolean(activeBaseUrl && activeModel);
};

const sessionDraftTitle = () => t("page.appQa.defaultSessionTitle");

const routeSessionId = computed(() => (typeof route.query.session === "string" ? route.query.session : ""));

const loadQaSettings = async () => {
  try {
    qaSettings.value = await seekMindApi.getQaSettings();
  } catch (error) {
    console.error("[SeekMind] getQaSettings failed", error);
  }
};

const loadQaModelProfiles = async () => {
  try {
    qaModelProfiles.value = await seekMindApi.listQaModelProfiles();
    if (
      qaProfileChoice.value !== "__current__" &&
      !qaModelProfiles.value.some((item) => item.id === qaProfileChoice.value)
    ) {
      qaProfileChoice.value = "__current__";
    }
  } catch (error) {
    console.error("[SeekMind] listQaModelProfiles failed", error);
  }
};

const refreshQaConfig = async () => {
  // 修复：问答页被 KeepAlive 缓存后，切回页面不会自动重拉模型配置，这里统一做显式同步。
  await loadQaSettings();
  await loadQaModelProfiles();
  console.info("[SeekMind] QA config refreshed");
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

  const messages = await seekMindApi.listQaMessages(sessionId, 100);
  qaMessages.value = messages;
  qaAnswer.value = messages.find((item) => item.id === uiState.answerId) ?? messages[messages.length - 1] ?? null;
  qaSelectedSourceId.value =
    qaAnswer.value?.sources.find((item) => item.source_id === uiState.selectedSourceId)?.source_id ?? "";
  qaSessionTitle.value = qaSessions.value.find((item) => item.id === sessionId)?.title ?? qaSessionTitle.value;
  expandedMessages.value = uiState.expandedMessages ?? {};
  await seekMindApi.recordRecentView("qa_session", sessionId, qaSessionTitle.value || t("page.appQa.defaultSessionTitle"), "");
  chatShouldFollowLatest.value = true;
  void scrollChatToBottom(true);
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
  const sessions = await seekMindApi.listQaSessions(50);
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
  await seekMindApi.removeQaSession(sessionId);
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

const openSourceContextMenu = (source: QaSourceView, event: MouseEvent) => {
  selectSource(source.source_id);
  sourceMenuTarget.value = source;
  sourceMenuPosition.value = { x: event.clientX, y: event.clientY };
  sourceMenuVisible.value = true;
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
  chatShouldFollowLatest.value = true;
  void scrollChatToBottom(true);
};

const loadInitialData = async () => {
  loading.value = true;
  try {
    const savedUiState = loadSavedQaUiState();
    qaSessionFilter.value = savedUiState.sessionFilter ?? qaSessionFilter.value;
    qaQuestion.value = savedUiState.question ?? qaQuestion.value;
    qaProfileChoice.value = savedUiState.profileChoice ?? qaProfileChoice.value;
    await refreshQaConfig();
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
    console.error("[SeekMind] loadInitialData failed", error);
  } finally {
    loading.value = false;
  }
};

const ensureSession = async (title: string) => {
  if (qaSessionId.value) {
    return qaSessionId.value;
  }

  const session = await seekMindApi.createQaSession(title.trim());
  qaSessions.value = [session, ...qaSessions.value.filter((item) => item.id !== session.id)];
  qaSessionId.value = session.id;
  qaSessionTitle.value = session.title;
  await syncRouteSession(session.id);
  await seekMindApi.recordRecentView("qa_session", session.id, session.title, "");
  return session.id;
};

const installQaProgressListener = async () => {
  if (unlistenQaProgress) {
    return;
  }

  unlistenQaProgress = await listen<QaAnswerProgressView>("seekmind:qa:answer-progress", (event) => {
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
      warning: payload.warning ?? null,
    };

    const messageIndex = qaMessages.value.findIndex((item) => item.id === payload.job_id);
    if (messageIndex >= 0) {
      qaMessages.value.splice(messageIndex, 1, nextMessage);
    } else {
      qaMessages.value.push(nextMessage);
    }
    qaAnswer.value = nextMessage;
    if (chatShouldFollowLatest.value) {
      void scrollChatToBottom(true);
    }

    if (payload.state === "searching") {
      qaLoading.value = true;
      qaInfoMessage.value = t("page.appQa.searching");
      qaErrorMessage.value = "";
      return;
    }

    // 修复：Python RAG 会在最终回答前发出 verifying 进度；这里必须继续视为进行中，避免提前清空 job 导致 answered 事件被过滤。
    if (["running", "generating", "streaming", "verifying"].includes(payload.state)) {
      qaLoading.value = true;
      qaInfoMessage.value = "";
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
            : payload.state === "failed"
              ? t("page.appQa.askFailed")
              : "";
    qaErrorMessage.value = payload.state === "failed" ? payload.error || t("page.appQa.modelUnavailable") : "";
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
    await seekMindApi.updateQaSessionTitle(session.id, nextTitle);
    qaSessions.value = qaSessions.value.map((item) =>
      item.id === session.id ? { ...item, title: nextTitle } : item,
    );
    if (qaSessionId.value === session.id) {
      qaSessionTitle.value = nextTitle;
    }
  } catch (error) {
    qaErrorMessage.value = formatSeekMindError(error, t("page.appQa.renameFailed"));
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
    await refreshQaConfig();
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
    const started: QaAskStartView = await seekMindApi.askQuestion(
      question,
      [],
      6,
      sessionId,
      recentQuestions,
      qaProfileChoice.value === "__current__" ? "" : qaProfileChoice.value,
    );
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
    chatShouldFollowLatest.value = true;
    void scrollChatToBottom(true);

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
    qaErrorMessage.value = formatSeekMindError(error, t("page.appQa.askFailed"));
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
    await seekMindApi.cancelQaQuestion(jobId);
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
  await seekMindApi.openFile(selectedSource.value.path);
};

const quickLookSelectedQaFile = async () => {
  if (!selectedSource.value) return;
  await seekMindApi.quickLookFile(selectedSource.value.path);
};

const copySelectedQaPath = async () => {
  if (!selectedSource.value) return;
  await navigator.clipboard.writeText(selectedSource.value.path);
};

const copySelectedQaCitation = async () => {
  if (!selectedSource.value) return;
  await navigator.clipboard.writeText(selectedSourceCitation.value);
};

const loadCollections = async () => {
  collectionPickerLoading.value = true;
  try {
    collections.value = await seekMindApi.listCollections();
  } catch (error) {
    console.error("[SeekMind] listCollections failed", error);
  } finally {
    collectionPickerLoading.value = false;
  }
};

const openSourceCollectionPicker = async (
  source: NonNullable<typeof selectedSource.value>,
  mode: "qa_source" | "document" = "qa_source",
) => {
  collectionPickerTarget.value = source;
  collectionPickerMode.value = mode;
  collectionPickerVisible.value = true;
  if (collections.value.length === 0) {
    await loadCollections();
  }
};

const addSelectedSourceToCollection = async (collectionId: string) => {
  if (!selectedSource.value || !qaAnswer.value) {
    return;
  }

  const source = selectedSource.value;
  const isDocumentCollection = collectionPickerMode.value === "document";
  const collection = collections.value.find((entry) => entry.id === collectionId);
  const input: CollectionItemInput = {
    collection_id: collectionId,
    item_type: collectionPickerMode.value,
    document_id: isDocumentCollection ? null : "",
    chunk_id: isDocumentCollection ? null : source.chunk_id,
    qa_session_id: isDocumentCollection ? null : qaAnswer.value.session_id,
    qa_message_id: isDocumentCollection ? null : qaAnswer.value.id,
    title: source.file_name,
    path: source.path,
    title_path: isDocumentCollection ? source.file_name : (source.title_path || source.heading),
    snippet: source.snippet,
    note: "",
    source_meta_json: JSON.stringify({
      mode: collectionPickerMode.value,
      source_id: source.source_id,
      score: source.score,
      rank_reason: source.rank_reason,
      ext: source.ext,
      paragraph: source.paragraph ?? null,
      page: source.page ?? null,
    }),
  };

  await seekMindApi.addCollectionItem(input);
  qaInfoMessage.value = t("page.collections.itemAddedToCollection", { name: collection?.name ?? t("common.none") });
  collectionPickerVisible.value = false;
  await loadCollections();
};

const createCollectionAndAddSource = async (name: string) => {
  const created = await seekMindApi.createCollection(name, "");
  collections.value = [created, ...collections.value.filter((item) => item.id !== created.id)];
  await addSelectedSourceToCollection(created.id);
};

const handleCollectionPickerSelect = async (collectionId: string) => {
  await addSelectedSourceToCollection(collectionId);
};

const handleCollectionPickerCreate = async (name: string) => {
  await createCollectionAndAddSource(name);
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
      ? await seekMindApi.listQaMessages(session.id, 50)
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

    const savedPath = await seekMindApi.exportQaSessionMarkdown(targetPath, markdown);
    qaInfoMessage.value = t("page.appQa.exportedMarkdown", { path: savedPath });
  } catch (error) {
    qaErrorMessage.value = formatSeekMindError(error, t("page.appQa.exportFailed"));
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
  unlistenQaConfigUpdated = listenQaConfigUpdated(() => {
    void refreshQaConfig();
  });
  await loadInitialData();
  await loadCollections();
});

onActivated(async () => {
  await refreshQaConfig();
});

onBeforeUnmount(() => {
  saveQaUiState();
  unlistenQaProgress?.();
  unlistenQaProgress = null;
  unlistenQaConfigUpdated?.();
  unlistenQaConfigUpdated = null;
});

watch(qaMessages, () => {
  if (!qaAnswer.value && qaMessages.value.length > 0) {
    qaAnswer.value = qaMessages.value[qaMessages.value.length - 1] ?? null;
  }
});

watch(
  [qaSessionId, qaAnswer, qaSelectedSourceId, expandedMessages, qaSessionFilter, qaQuestion, qaProfileChoice],
  saveQaUiState,
  { deep: true },
);

watch(
  selectedSource,
  (source) => {
    void loadSelectedSourcePreviewBlocks(source);
  },
  { immediate: true },
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
  <section class="m-3 flex h-full min-h-0 flex-col overflow-hidden bg-transparent">
    <SplitPane :panels="panels">
      <template #sidebar>
        <aside class="flex h-full min-h-0 flex-col overflow-hidden bg-[rgba(244,245,247,0.82)]">
          <div class="px-4 py-3">
            <div class="flex items-start justify-between gap-3">
              <div class="min-w-0">
                <div class="text-sm font-semibold text-primary">{{ t("page.appQa.sessions") }}</div>
                <div class="mt-0.5 text-[11px] text-muted">{{ t("page.appQa.sessionDesc") }}</div>
              </div>
              <div class="flex items-center gap-1.5">
                <button
                  class="inline-flex h-8 w-8 items-center justify-center rounded-full bg-accent text-white hover:bg-accent/90"
                  :title="t('page.appQa.createSession')"
                  @click="newSession"
                >
                  <Plus :size="14" />
                </button>
                <button
                  class="inline-flex h-8 w-8 items-center justify-center rounded-full bg-white/70 text-secondary hover:bg-surface-hover"
                  :title="t('page.appQa.settings')"
                  @click="router.push('/settings')"
                >
                  <SlidersHorizontal :size="14" />
                </button>
                <button
                  class="inline-flex h-8 w-8 items-center justify-center rounded-full bg-white/70 text-secondary hover:bg-surface-hover"
                  :title="t('common.refresh')"
                  @click="refreshSessions()"
                >
                  <RefreshCw :size="14" />
                </button>
              </div>
            </div>
            <div class="mt-3 flex items-center gap-2 rounded-full bg-[rgba(118,118,128,0.08)] px-3 py-2">
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

          <div class="min-h-0 flex-1 overflow-y-auto px-3 pb-3">
            <div v-if="loading" class="rounded-[18px] bg-surface/60 px-4 py-6 text-xs text-muted">
              {{ t("common.loading") }}
            </div>
            <div v-else-if="qaSessions.length === 0" class="rounded-[18px] bg-surface/60 px-4 py-6 text-xs text-muted">
              {{ t("page.appQa.emptySessions") }}
            </div>
            <div v-else-if="filteredSessions.length === 0" class="rounded-[18px] bg-surface/60 px-4 py-6 text-xs text-muted">
              {{ t("page.appQa.noMatchingSessions") }}
            </div>
            <div v-else class="space-y-2">
              <div
                v-for="session in filteredSessions"
                :key="session.id"
                class="w-full rounded-[14px] px-3 py-2.5 text-left transition"
                :class="qaSessionId === session.id ? 'bg-[rgba(0,122,255,0.12)]' : 'bg-transparent hover:bg-surface-hover/60'"
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
                      class="w-full rounded-md border border-transparent bg-white/70 px-2 py-1 text-sm font-medium text-primary outline-none"
                      type="text"
                      autofocus
                      @click.stop
                      @keydown.enter.prevent="saveRenamedSession(session)"
                      @keydown.esc.prevent="cancelRenameSession"
                      @blur="saveRenamedSession(session)"
                    />
                    <div v-else class="truncate text-sm font-medium text-primary">{{ session.title }}</div>
                    <div class="mt-1 flex items-center gap-2 text-[11px] text-muted">
                      <SeekMindBadge tone="default">{{ t("page.appQa.messageCount", { count: session.message_count }) }}</SeekMindBadge>
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
        <section class="seekmind-pane-center flex h-full min-h-0 flex-1 flex-col overflow-hidden">
          <div class="flex items-center justify-between gap-3 px-4 py-2">
            <div class="text-xs font-medium text-dim">
              {{ currentSession ? currentSession.title : t("page.appQa.currentSessionEmpty") }}
            </div>
            <div class="flex items-center gap-2">
              <SeekMindBadge tone="default">{{ qaMessages.length }}</SeekMindBadge>
              <SeekMindBadge :tone="isQaConfigured(qaSettings) ? 'success' : 'default'">
                {{ isQaConfigured(qaSettings) ? t("page.appQa.enabled") : t("page.appQa.disabled") }}
              </SeekMindBadge>
            </div>
          </div>

          <div ref="qaChatScrollEl" class="relative min-h-0 flex-1 overflow-y-auto px-2" @scroll.passive="handleChatScroll">
            <SeekMindToast v-if="qaErrorMessage" :message="qaErrorMessage" tone="error" />
            <div v-else-if="qaMessages.length" class="space-y-4 p-4">
              <article
                v-for="(message, index) in qaMessages"
                :key="message.id"
                class="relative"
                @click="selectMessage(message)"
              >
                <div class="space-y-2">
                  <div class="flex justify-end">
                    <div class="max-w-[86%] rounded-[16px] bg-[rgba(0,122,255,0.10)] px-4 py-3">
                      <div class="break-words text-sm leading-7 text-primary">{{ message.question }}</div>
                    </div>
                  </div>

                  <div class="flex justify-start">
                    <div
                      :class="[
                        isMessagePending(message)
                          ? 'inline-flex rounded-[16px] bg-surface/70 px-3 py-2'
                          : 'relative max-w-[86%] rounded-[16px] bg-surface/70 px-4 py-3',
                      ]"
                    >
                      <template v-if="isMessagePending(message)">
                        <span class="seekmind-typing-indicator" aria-hidden="true">
                          <span class="seekmind-typing-dot" />
                          <span class="seekmind-typing-dot" />
                          <span class="seekmind-typing-dot" />
                        </span>
                      </template>
                      <template v-else>
                        <div
                          v-if="isMessageStreaming(message)"
                          class="pointer-events-none absolute right-3 top-3 inline-flex items-center rounded-full border border-accent/20 bg-accent-soft/70 px-2 py-1 text-[11px] font-medium text-accent shadow-sm"
                          :title="t('page.appQa.generating')"
                          aria-label="answer streaming"
                        >
                          <span class="seekmind-typing-indicator" aria-hidden="true">
                            <span class="seekmind-typing-dot" />
                            <span class="seekmind-typing-dot" />
                            <span class="seekmind-typing-dot" />
                          </span>
                        </div>
                      <div class="flex items-center justify-between gap-3">
                        <div class="flex items-center gap-2">
                          <SeekMindBadge
                            v-if="['cancelled', 'failed', 'insufficient_evidence', 'model_not_configured'].includes(message.state)"
                            tone="default"
                          >
                            {{ t(`page.appSearch.qa.state.${message.state}`) }}
                          </SeekMindBadge>
                          <SeekMindBadge v-if="message.state === 'cancelled'" tone="danger">
                            {{ t("page.appSearch.qa.cancelledByUser") }}
                          </SeekMindBadge>
                        </div>
                      </div>
                      <div v-if="shouldShowMessageBody(message)" class="mt-3">
                        <SeekMindMarkdownRenderer
                          v-if="hasMessageAnswer(message)"
                          :block="emptyMarkdownBlock"
                          :markdown="messageBodyMarkdown(message)"
                          :citation-source-ids="messageCitationSourceIds(message)"
                          @citation-click="selectSource"
                        />
                        <div
                          v-else
                          class="whitespace-pre-wrap text-sm leading-7 text-secondary"
                        >
                          {{ messageBodyPlainText(message) }}
                        </div>
                      </div>
                      <div v-if="message.warning" class="mt-3 rounded-xl border border-amber-soft bg-amber-soft px-4 py-3 text-sm text-warning">
                        <div class="font-medium">{{ t("page.appQa.citationWarning") }}</div>
                        <div class="mt-1 text-xs leading-5 text-warning/90">{{ message.warning }}</div>
                      </div>
                      <div v-if="message.state === 'failed' || message.error" class="mt-3 rounded-xl border border-danger-soft bg-danger-soft px-4 py-3 text-sm text-danger">
                        <div class="font-medium">{{ messageFailureTitle(message) }}</div>
                        <div class="mt-1 text-xs leading-5 text-danger/90">{{ messageFailureHint(message) }}</div>
                      </div>
                      <div v-if="shouldShowMessageMeta(message)" class="mt-3 flex flex-wrap items-center gap-2 text-xs text-dim">
                        <SeekMindBadge tone="default">{{ message.model || t("common.none") }}</SeekMindBadge>
                        <SeekMindBadge tone="default" :title="message.created_at">{{ formatLocalDateTime(message.created_at) }}</SeekMindBadge>
                        <SeekMindBadge tone="default">{{ t("page.appQa.sourceCount", { count: message.sources.length }) }}</SeekMindBadge>
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
                      </template>

                      <div v-if="expandedMessages[message.id]" class="mt-4 rounded-[16px] bg-white/60 p-3">
                        <div class="seekmind-section-label">{{ t("page.appQa.sourceSummary") }}</div>
                        <div class="mt-3 overflow-x-auto rounded-[14px] bg-white/70">
                          <table class="w-full min-w-[720px] table-fixed text-left text-xs">
                            <thead class="bg-black/[0.02] text-[11px] uppercase tracking-wide text-muted">
                              <tr>
                                <th class="w-16 px-3 py-2 font-medium">{{ t("page.appQa.sourceColumnId") }}</th>
                                <th class="w-[28%] px-3 py-2 font-medium">{{ t("page.appQa.sourceColumnFile") }}</th>
                                <th class="px-3 py-2 font-medium">{{ t("page.appQa.sourceColumnLocation") }}</th>
                                <th class="w-24 px-3 py-2 font-medium">{{ t("page.appQa.sourceColumnScore") }}</th>
                                <th class="w-[22%] px-3 py-2 font-medium">{{ t("page.appQa.sourceColumnReason") }}</th>
                              </tr>
                            </thead>
                            <tbody>
                              <tr
                                v-for="source in message.sources"
                                :key="source.source_id"
                                class="cursor-pointer transition"
                                :class="qaSelectedSourceId === source.source_id ? 'bg-[rgba(0,122,255,0.10)]' : 'hover:bg-surface-hover/60'"
                                @click.stop="selectSource(source.source_id)"
                                @contextmenu.prevent="openSourceContextMenu(source, $event)"
                              >
                                <td class="px-3 py-2 align-top">
                                  <SeekMindBadge tone="default">{{ source.source_id }}</SeekMindBadge>
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
            <div v-else class="m-4 rounded-[18px] bg-surface/60 px-4 py-6 text-center text-xs text-muted">
              {{ t("page.appQa.enterQuestion") }}
            </div>
            <button
              v-if="qaMessages.length > 0 && !chatShouldFollowLatest"
              class="sticky bottom-4 ml-auto mr-4 mb-4 flex items-center gap-2 rounded-full bg-white/80 px-3 py-2 text-xs font-medium text-secondary hover:bg-surface-hover"
              type="button"
              @click="scrollChatToBottom(true)"
            >
              <ChevronDown :size="14" />
              {{ t("page.appQa.jumpToLatest") }}
            </button>
          </div>

          <div class="bg-[rgba(252,252,254,0.96)] px-4 py-3">
            <div class="rounded-[18px] bg-white/82 px-3 py-3">
              <div class="flex items-center gap-2">
                <select
                  v-model="qaProfileChoice"
                  class="seekmind-select min-w-0 flex-1 rounded-full px-3 py-2 text-[12px] text-secondary outline-none"
                >
                  <option value="__current__">
                    {{ t("page.appQa.defaultModelOption", { model: qaCurrentModelLabel }) }}
                  </option>
                  <option v-for="profile in qaModelOptions" :key="profile.id" :value="profile.id">
                    {{ profile.name }} · {{ profile.model }}
                  </option>
                </select>
              </div>
              <textarea
                ref="qaQuestionInput"
                v-model="qaQuestion"
                rows="2"
                class="min-h-[44px] w-full resize-none border-0 bg-transparent px-0 py-2 text-sm leading-6 text-primary outline-none placeholder:text-muted"
                :placeholder="t('page.appQa.placeholder')"
                @keydown.enter.exact.prevent="runQa"
              />
              <div class="mt-1.5 flex items-end justify-between gap-3">
                <div class="pb-0.5 text-[11px] leading-4 text-dim">
                  {{ t("page.appQa.sendHint") }}
                </div>
                <button
                  v-if="qaLoading || qaCancelling"
                  class="inline-flex h-10 w-10 items-center justify-center rounded-full bg-white text-secondary hover:bg-surface-hover"
                  type="button"
                  @click="stopQa"
                >
                  <X :size="16" />
                </button>
                <button
                  v-else
                  class="inline-flex h-10 w-10 items-center justify-center rounded-full bg-accent text-white transition hover:bg-accent/90 disabled:cursor-not-allowed disabled:opacity-70"
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
        <aside v-if="selectedSource" class="seekmind-pane-detail flex h-full min-h-0 flex-col overflow-hidden">
          <SeekMindDetailPanel>
            <template #header>
              <div class="flex items-start justify-between gap-3">
                <div class="min-w-0">
                  <div class="truncate text-base font-semibold leading-6 text-primary" :title="selectedSource.file_name">{{ selectedSource.file_name }}</div>
                  <div class="mt-0.5 break-all text-xs leading-5 text-muted" :title="selectedSource.path">{{ selectedSource.path }}</div>
                </div>
                <div class="flex items-center gap-2">
                  <SeekMindBadge tone="default">{{ selectedSource.source_id }}</SeekMindBadge>
                  <button
                    class="seekmind-close-button"
                    type="button"
                    :title="t('common.close')"
                    @click="closeSelectedSource"
                  >
                    <X :size="13" stroke-width="2.25" />
                  </button>
                </div>
              </div>
              <div v-if="selectedSourceTitlePath" class="mt-2" :title="selectedSourceTitlePath">
                <div class="text-[11px] font-semibold uppercase tracking-[0.16em] text-dim">
                  {{ t("page.appSearch.detail.titlePath") }}
                </div>
                <div class="mt-1 text-sm leading-6 text-primary">
                  {{ selectedSourceTitlePath }}
                </div>
              </div>
            </template>

            <SeekMindDetailSection :title="t('common.overview')" :subtitle="selectedSourceCitation">
              <div class="flex flex-wrap gap-1.5">
                <SeekMindBadge tone="default">{{ selectedSource.ext.toUpperCase() }}</SeekMindBadge>
                <SeekMindBadge tone="default">
                  {{ selectedSource.page ? t("searchResultCard.page", { page: selectedSource.page }) : t("searchResultCard.paragraph", { para: selectedSource.paragraph ?? 0 }) }}
                </SeekMindBadge>
                <SeekMindBadge v-if="selectedSource.page" tone="default">{{ t("page.appSearch.detail.pdfPage", { page: selectedSource.page }) }}</SeekMindBadge>
                <SeekMindBadge tone="success">{{ t("page.appSearch.qa.sourceId", { id: selectedSource.source_id }) }}</SeekMindBadge>
              </div>
              <div class="rounded-[14px] bg-white/70 px-3 py-3 text-sm leading-6 text-secondary">
                {{ qaAnswer?.retrieval.search_mode || t("common.none") }} · {{ qaAnswer?.retrieval.selected_count ?? 0 }}/{{ qaAnswer?.retrieval.candidate_count ?? 0 }}
              </div>
              <div class="rounded-[14px] bg-white/70 px-3 py-3 text-sm leading-6 text-secondary">
                {{ selectedSource.rank_reason || t("common.none") }}
              </div>
            </SeekMindDetailSection>

            <SeekMindDetailSection :title="t('common.originalText')">
              <div v-if="loadingSelectedSourcePreview && selectedSourcePreviewBlocks.length === 0" class="rounded-[14px] bg-white/70 px-3 py-3 text-sm text-muted">
                {{ t("common.loading") }}
              </div>
              <div v-else-if="selectedSourcePreviewBlocks.length > 0" class="space-y-2">
                <SeekMindPreviewBlockRenderer
                  v-for="block in selectedSourcePreviewBlocks"
                  :key="block.block_index"
                  :block="block"
                />
              </div>
              <div v-else class="whitespace-pre-wrap rounded-[14px] bg-white/78 px-3 py-3 text-sm leading-7 text-secondary">
                {{ selectedSource.snippet }}
              </div>
            </SeekMindDetailSection>

            <SeekMindDetailSection :title="t('common.context')">
              <div class="grid gap-2 text-sm leading-6 text-secondary">
                <div>{{ t("page.appSearch.qa.sourceMeta") }}：{{ selectedSourceCitation || t("common.none") }}</div>
                <div>{{ t("common.matchReason") }}：{{ selectedSource.rank_reason || t("common.none") }}</div>
              </div>
            </SeekMindDetailSection>

            <div class="flex flex-wrap gap-2">
              <button class="rounded-md border border-default bg-surface px-3 py-2 text-xs text-secondary hover:bg-surface-hover" @click="openSelectedQaFile">
                {{ t("page.appSearch.detail.openFile") }}
              </button>
              <button class="rounded-md border border-default bg-surface px-3 py-2 text-xs text-secondary hover:bg-surface-hover" @click="viewQaChunks">
                {{ t("page.appSearch.detail.viewChunks") }}
              </button>
              <button class="rounded-md border border-default bg-surface px-3 py-2 text-xs text-secondary hover:bg-surface-hover" @click="quickLookSelectedQaFile">
                {{ t("page.appSearch.detail.quickLook") }}
              </button>
              <button class="rounded-md border border-default bg-surface px-3 py-2 text-xs text-secondary hover:bg-surface-hover" @click="copySelectedQaPath">
                {{ t("page.appSearch.detail.copyPath") }}
              </button>
              <button class="rounded-md border border-default bg-surface px-3 py-2 text-xs text-secondary hover:bg-surface-hover" @click="copySelectedQaCitation">
                {{ t("page.appSearch.detail.copyCitation") }}
              </button>
            </div>
          </SeekMindDetailPanel>
        </aside>
        <aside v-else class="seekmind-pane-detail flex h-full min-h-0 items-center justify-center px-4 text-center text-xs text-muted">
          {{ qaMessages.length ? t("page.appQa.noSourceSelected") : t("page.appQa.noSourceYet") }}
        </aside>
      </template>
    </SplitPane>
    <SeekMindContextMenu
      v-if="sessionMenuVisible"
      :items="sessionContextMenuItems"
      :x="sessionMenuPosition.x"
      :y="sessionMenuPosition.y"
      @close="sessionMenuVisible = false"
    />
    <SeekMindContextMenu
      v-if="sourceMenuVisible"
      :items="sourceContextMenuItems"
      :x="sourceMenuPosition.x"
      :y="sourceMenuPosition.y"
      @close="sourceMenuVisible = false"
    />
    <SeekMindCollectionPicker
      :visible="collectionPickerVisible"
      :collections="collections"
      :loading="collectionPickerLoading"
      :title="collectionPickerTarget ? collectionPickerTarget.file_name : t('page.collections.pickerTitle')"
      :subtitle="collectionPickerTarget ? collectionPickerTarget.path : t('page.collections.pickerSubtitle')"
      @close="collectionPickerVisible = false"
      @select="handleCollectionPickerSelect"
      @create="handleCollectionPickerCreate"
    />
  </section>
</template>

<style scoped>
/* 修复：流式等待态使用绝对定位的三点提示，不占文档流，避免和最终回答的气泡布局不一致。 */
@keyframes seekmindTypingPulse {
  0%,
  80%,
  100% {
    transform: translateY(0);
    opacity: 0.35;
  }

  40% {
    transform: translateY(-2px);
    opacity: 1;
  }
}

.seekmind-typing-indicator {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  min-height: 10px;
}

.seekmind-typing-dot {
  width: 4px;
  height: 4px;
  border-radius: 9999px;
  background: currentColor;
  animation: seekmindTypingPulse 1.2s infinite ease-in-out;
}

.seekmind-typing-dot:nth-child(2) {
  animation-delay: 0.15s;
}

.seekmind-typing-dot:nth-child(3) {
  animation-delay: 0.3s;
}
</style>
