import { invoke } from "@tauri-apps/api/core";
import type {
  ChunkView,
  FavoriteView,
  DocumentView,
  DocumentRefreshStartView,
  IndexDirView,
  IndexStatusView,
  IndexSettingsView,
  EmbeddingModelView,
  ParserRuntimeView,
  RecentDocumentView,
  SearchHistoryView,
  SearchDebugView,
  SearchResultView,
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
  listRecentDocuments: (limit = 8) =>
    invoke<RecentDocumentView[]>("list_recent_documents", { limit }),
  listFavorites: (limit = 12) =>
    invoke<FavoriteView[]>("list_favorites", { limit }),
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
  importPaths: (paths: string[]) =>
    invoke<ImportPathsView>("import_paths", { paths }),
  addIndexDir: (path: string) => invoke<void>("add_index_dir", { path }),
  removeIndexDir: (path: string) => invoke<void>("remove_index_dir", { path }),
  setIndexDirEnabled: (path: string, enabled: boolean) =>
    invoke<void>("set_index_dir_enabled", { path, enabled }),
  retryFailedFile: (path: string) =>
    invoke<IndexStatusView>("retry_failed_file", { path }),
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
};
