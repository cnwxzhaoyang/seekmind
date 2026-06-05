import { invoke } from "@tauri-apps/api/core";
import type {
  ChunkView,
  CollectionItemInput,
  CollectionItemPatchInput,
  CollectionItemView,
  CollectionPatchInput,
  CollectionView,
  FavoriteView,
  QaAnswerView,
  QaAskStartView,
  QaAnswerProgressView,
  QaConnectionTestView,
  QaHistoryView,
  QaMessageView,
  QaModelProfileUpsertView,
  QaModelProfileView,
  QaSessionView,
  NetworkProxySettingsView,
  QaSettingsView,
  DocumentView,
  DocumentRefreshStartView,
  IndexDirView,
  IndexStatusView,
  IndexSettingsView,
  EmbeddingModelView,
  ParserRuntimeView,
  RecentDocumentView,
  RecentViewEntry,
  SearchHistoryView,
  SearchDebugView,
  SearchResultView,
  TagPatchInput,
  TagView,
  SemanticRebuildStartView,
  SemanticDebugView,
  SemanticModelStatusView,
  IndexRefreshStartView,
  ImportPathsView,
} from "../types/docmind";

export const formatDocmindError = (error: unknown, fallback: string) => {
  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message;
  }

  if (typeof error === "string" && error.trim().length > 0) {
    return error;
  }

  try {
    const serialized = JSON.stringify(error);
    if (serialized && serialized !== "{}") {
      return serialized;
    }
  } catch {
    // ignore serialization errors
  }

  return fallback;
};

export const docmindApi = {
  listIndexDirs: () => invoke<IndexDirView[]>("list_index_dirs"),
  listDocumentsInDir: (path: string) =>
    invoke<DocumentView[]>("list_documents_in_dir", { path }),
  listDocumentChunks: (path: string) =>
    invoke<ChunkView[]>("list_document_chunks", { path }),
  refreshDocument: (path: string, dirPath: string) =>
    invoke<DocumentRefreshStartView>("refresh_document", { path, dirPath }),
  listSearchHistory: (limit = 12) =>
    invoke<SearchHistoryView[]>("list_search_history", { limit }),
  removeSearchHistory: (query: string) =>
    invoke<void>("remove_search_history", { query }),
  listRecentDocuments: (limit = 8) =>
    invoke<RecentDocumentView[]>("list_recent_documents", { limit }),
  removeRecentDocument: (path: string) =>
    invoke<void>("remove_recent_document", { path }),
  listRecentViews: (limit = 12) =>
    invoke<RecentViewEntry[]>("list_recent_views", { limit }),
  recordRecentView: (targetType: string, targetId: string, title: string, path = "") =>
    invoke<void>("record_recent_view", { targetType, targetId, title, path }),
  listTags: () => invoke<TagView[]>("list_tags"),
  listTargetTags: (targetType: string, targetId: string) =>
    invoke<TagView[]>("list_target_tags", { targetType, targetId }),
  createTag: (name: string, color = "") => invoke<TagView>("create_tag", { name, color }),
  updateTag: (tagId: string, patch: TagPatchInput) =>
    invoke<TagView>("update_tag", { tagId, patch }),
  deleteTag: (tagId: string) => invoke<void>("delete_tag", { tagId }),
  addTagToTarget: (targetType: string, targetId: string, name: string, color = "") =>
    invoke<TagView>("add_tag_to_target", { targetType, targetId, name, color }),
  removeTagFromTarget: (targetType: string, targetId: string, tagId: string) =>
    invoke<void>("remove_tag_from_target", { targetType, targetId, tagId }),
  listFavorites: (limit = 12) =>
    invoke<FavoriteView[]>("list_favorites", { limit }),
  removeFavorite: (target: string) =>
    invoke<void>("remove_favorite", { target }),
  listCollections: () => invoke<CollectionView[]>("list_collections"),
  createCollection: (name: string, description = "") =>
    invoke<CollectionView>("create_collection", { name, description }),
  updateCollection: (collectionId: string, patch: CollectionPatchInput) =>
    invoke<CollectionView>("update_collection", { collectionId, patch }),
  deleteCollection: (collectionId: string) =>
    invoke<void>("delete_collection", { collectionId }),
  listCollectionItems: (collectionId: string) =>
    invoke<CollectionItemView[]>("list_collection_items", { collectionId }),
  addCollectionItem: (input: CollectionItemInput) =>
    invoke<CollectionItemView>("add_collection_item", { input }),
  updateCollectionItemNote: (itemId: string, patch: CollectionItemPatchInput) =>
    invoke<CollectionItemView>("update_collection_item_note", { itemId, patch }),
  removeCollectionItem: (itemId: string) =>
    invoke<void>("remove_collection_item", { itemId }),
  exportCollectionMarkdown: (collectionId: string, path: string) =>
    invoke<string>("export_collection_markdown", { collectionId, path }),
  getQaSettings: () => invoke<QaSettingsView>("get_qa_settings"),
  saveQaSettings: (settings: QaSettingsView) =>
    invoke<QaSettingsView>("save_qa_settings", { settings }),
  getNetworkProxySettings: () => invoke<NetworkProxySettingsView>("get_network_proxy_settings"),
  saveNetworkProxySettings: (settings: NetworkProxySettingsView) =>
    invoke<NetworkProxySettingsView>("save_network_proxy_settings", { settings }),
  cancelQaQuestion: (jobId: string) =>
    invoke<void>("cancel_qa_question", { jobId }),
  askQuestion: (
    question: string,
    scopePaths: string[] = [],
    limit = 6,
    sessionId?: string,
    recentQuestions: string[] = [],
    profileId = "",
  ) =>
    invoke<QaAskStartView>("ask_question", { question, scopePaths, limit, sessionId, recentQuestions, profile_id: profileId }),
  testQaConnection: (settings: QaSettingsView) =>
    invoke<QaConnectionTestView>("test_qa_connection", { settings }),
  listQaModelProfiles: () => invoke<QaModelProfileView[]>("list_qa_model_profiles"),
  saveQaModelProfile: (profile: QaModelProfileUpsertView) =>
    invoke<QaModelProfileView>("save_qa_model_profile", { profile }),
  removeQaModelProfile: (profileId: string) =>
    invoke<void>("remove_qa_model_profile", { profile_id: profileId }),
  setDefaultQaModelProfile: (profileId: string) =>
    invoke<QaModelProfileView>("set_default_qa_model_profile", { profile_id: profileId }),
  listQaHistory: (limit = 12) =>
    invoke<QaHistoryView[]>("list_qa_history", { limit }),
  removeQaHistory: (id: string) =>
    invoke<void>("remove_qa_history", { id }),
  createQaSession: (title: string) =>
    invoke<QaSessionView>("create_qa_session", { title }),
  listQaSessions: (limit = 12) =>
    invoke<QaSessionView[]>("list_qa_sessions", { limit }),
  listQaMessages: (sessionId: string, limit = 50) =>
    invoke<QaMessageView[]>("list_qa_messages", { sessionId, limit }),
  removeQaSession: (sessionId: string) =>
    invoke<void>("remove_qa_session", { sessionId }),
  updateQaSessionTitle: (sessionId: string, title: string) =>
    invoke<void>("update_qa_session_title", { sessionId, title }),
  exportQaSessionMarkdown: (path: string, markdown: string) =>
    invoke<string>("export_qa_session_markdown", { path, markdown }),
  searchDocuments: (query: string, limit = 20) =>
    invoke<SearchResultView[]>("search_documents", { query, limit }),
  getSearchDebugReport: (query: string, limit = 20) =>
    invoke<SearchDebugView>("get_search_debug_report", { query, limit }),
  requestSearchDebugReport: (requestId: string, query: string, limit = 20) =>
    invoke<void>("request_search_debug_report", { request_id: requestId, query, limit }),
  getIndexStatus: () => invoke<IndexStatusView>("get_index_status"),
  getIndexSettings: () => invoke<IndexSettingsView>("get_index_settings"),
  saveIndexSettings: (settings: IndexSettingsView) =>
    invoke<void>("save_index_settings", { settings }),
  getParserRuntime: () => invoke<ParserRuntimeView>("get_parser_runtime"),
  getEmbeddingModelStatus: () => invoke<SemanticModelStatusView>("get_embedding_model_status"),
  listEmbeddingModels: () => invoke<EmbeddingModelView[]>("list_embedding_models"),
  setDefaultEmbeddingModel: (modelId: string) =>
    invoke<SemanticModelStatusView>("set_default_embedding_model", { model_id: modelId }),
  rebuildSemanticEmbeddings: () =>
    invoke<SemanticRebuildStartView>("rebuild_semantic_embeddings"),
  getSemanticDebugReport: (query: string, limit = 12) =>
    invoke<SemanticDebugView>("get_semantic_debug_report", { query, limit }),
  refreshIndex: () => invoke<IndexRefreshStartView>("refresh_index"),
  refreshIndexDir: (path: string) =>
    invoke<IndexRefreshStartView>("refresh_index_dir", { path }),
  refreshPdfOcrTasks: () => invoke<IndexRefreshStartView>("refresh_pdf_ocr_tasks"),
  importPaths: (paths: string[]) =>
    invoke<ImportPathsView>("import_paths", { paths }),
  addIndexDir: (path: string) => invoke<void>("add_index_dir", { path }),
  removeIndexDir: (path: string) => invoke<void>("remove_index_dir", { path }),
  setIndexDirEnabled: (path: string, enabled: boolean) =>
    invoke<void>("set_index_dir_enabled", { path, enabled }),
  retryFailedFile: (path: string) =>
    invoke<IndexStatusView>("retry_failed_file", { path }),
  refreshPdfOcrDocument: (path: string) =>
    invoke<IndexStatusView>("refresh_pdf_ocr_document", { path }),
  toggleResultFavorite: (
    path: string,
    heading: string,
    paragraph: number | null | undefined,
    page: number | null | undefined,
    fileName: string,
  ) => invoke<boolean>("toggle_result_favorite", { path, heading, paragraph, page, file_name: fileName }),
  clearAllIndexes: () => invoke<IndexStatusView>("clear_all_indexes"),
  pauseIndexing: () => invoke<IndexStatusView>("pause_indexing"),
  resumeIndexing: () => invoke<IndexStatusView>("resume_indexing"),
  openFile: (path: string) => invoke<void>("open_file", { path }),
  quickLookFile: (path: string) => invoke<void>("quick_look_file", { path }),
  deleteDocument: (path: string) => invoke<void>("delete_document", { path }),
};
