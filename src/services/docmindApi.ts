import { invoke } from "@tauri-apps/api/core";
import type {
  ChunkView,
  DocumentView,
  IndexDirView,
  IndexStatusView,
  IndexSettingsView,
  ParserRuntimeView,
  SearchDebugView,
  SearchResultView,
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
  searchDocuments: (query: string, limit = 20) =>
    invoke<SearchResultView[]>("search_documents", { query, limit }),
  getSearchDebugReport: (query: string, limit = 20) =>
    invoke<SearchDebugView>("get_search_debug_report", { query, limit }),
  getIndexStatus: () => invoke<IndexStatusView>("get_index_status"),
  getIndexSettings: () => invoke<IndexSettingsView>("get_index_settings"),
  saveIndexSettings: (settings: IndexSettingsView) =>
    invoke<void>("save_index_settings", { settings }),
  getParserRuntime: () => invoke<ParserRuntimeView>("get_parser_runtime"),
  refreshIndex: () => invoke<IndexStatusView>("refresh_index"),
  refreshIndexDir: (path: string) =>
    invoke<IndexStatusView>("refresh_index_dir", { path }),
  addIndexDir: (path: string) => invoke<void>("add_index_dir", { path }),
  removeIndexDir: (path: string) => invoke<void>("remove_index_dir", { path }),
  setIndexDirEnabled: (path: string, enabled: boolean) =>
    invoke<void>("set_index_dir_enabled", { path, enabled }),
  retryFailedFile: (path: string) =>
    invoke<IndexStatusView>("retry_failed_file", { path }),
  clearAllIndexes: () => invoke<IndexStatusView>("clear_all_indexes"),
  pauseIndexing: () => invoke<IndexStatusView>("pause_indexing"),
  resumeIndexing: () => invoke<IndexStatusView>("resume_indexing"),
  openFile: (path: string) => invoke<void>("open_file", { path }),
};
