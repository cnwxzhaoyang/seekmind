export interface IndexDirView {
  path: string;
  enabled: boolean;
  docs: number;
  chunks: number;
  status: string;
}

export interface SearchResultView {
  id: string;
  file_name: string;
  path: string;
  ext: string;
  heading: string;
  snippet: string;
  matched_field: string;
  match_origin: string;
  highlight_spans: HighlightSpan[];
  snippet_window_start: number;
  snippet_window_end: number;
  snippet_source_len: number;
  paragraph?: number | null;
  page?: number | null;
  modified: string;
  score: number;
}

export interface HighlightSpan {
  start: number;
  end: number;
}

export interface DocumentView {
  id: string;
  dir_path: string;
  path: string;
  file_name: string;
  ext: string;
  modified: string;
  chunks: number;
}

export interface ChunkView {
  id: string;
  heading: string;
  snippet: string;
  paragraph?: number | null;
  page?: number | null;
}

export interface FailedFileView {
  file: string;
  reason: string;
  category: string;
  code: string;
  retry_count: number;
  last_failed_at: string;
}

export interface CurrentTaskView {
  label: string;
  details: string;
  state: string;
  current_dir: string;
  current_file: string;
  progress: number;
  scanned: number;
  total: number;
  succeeded: number;
  failed: number;
  updated: number;
  skipped: number;
  deleted: number;
  pause_requested: boolean;
}

export interface IndexRunSummaryView {
  updated: number;
  skipped: number;
  deleted: number;
  scanned: number;
  total: number;
  succeeded: number;
  failed: number;
  completed_at: string;
}

export interface IndexStatusView {
  indexed_docs: number;
  indexed_chunks: number;
  scanned_docs: number;
  failed_files: number;
  current_task: CurrentTaskView | null;
  failed_items: FailedFileView[];
  last_run: IndexRunSummaryView | null;
}

export interface ParserRuntimeView {
  enabled: boolean;
  available: boolean;
  active: "python" | "rust";
  python_bin: string;
  script_path: string;
  timeout_ms: number;
}

export interface SearchDebugView {
  query: string;
  normalized_terms: string[];
  normalized_search_text: string;
  sqlite_documents: number;
  sqlite_chunks: number;
  tantivy_documents: number;
  hit_count: number;
  hits: SearchResultView[];
}

export interface SearchHistoryView {
  query: string;
  normalized_query: string;
  hit_count: number;
  last_hit_at: string;
}

export interface RecentDocumentView {
  path: string;
  title: string;
  file_name: string;
  ext: string;
  last_opened_at: string;
  open_count: number;
}

export interface FavoriteView {
  favorite_type: string;
  target: string;
  title: string;
  path: string;
  created_at: string;
  updated_at: string;
}

export interface IndexSettingsView {
  exclude_dirs: string[];
  exclude_exts: string[];
  max_file_size_mb: number;
}
