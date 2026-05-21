import { invoke } from "@tauri-apps/api/core";
import type {
  IndexDirView,
  IndexStatusView,
  SearchResultView,
} from "../types/docmind";

export const docmindApi = {
  listIndexDirs: () => invoke<IndexDirView[]>("list_index_dirs"),
  searchDocuments: (query: string, limit = 20) =>
    invoke<SearchResultView[]>("search_documents", { query, limit }),
  getIndexStatus: () => invoke<IndexStatusView>("get_index_status"),
  openFile: (path: string) => invoke<void>("open_file", { path }),
};
