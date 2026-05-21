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
  refreshIndex: () => invoke<IndexStatusView>("refresh_index"),
  refreshIndexDir: (path: string) =>
    invoke<IndexStatusView>("refresh_index_dir", { path }),
  addIndexDir: (path: string) => invoke<void>("add_index_dir", { path }),
  removeIndexDir: (path: string) => invoke<void>("remove_index_dir", { path }),
  setIndexDirEnabled: (path: string, enabled: boolean) =>
    invoke<void>("set_index_dir_enabled", { path, enabled }),
  retryFailedFile: (path: string) =>
    invoke<IndexStatusView>("retry_failed_file", { path }),
  openFile: (path: string) => invoke<void>("open_file", { path }),
};
