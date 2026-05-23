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
  paragraph?: number | null;
  page?: number | null;
  modified: string;
  score: number;
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
}

export interface CurrentTaskView {
  label: string;
  details: string;
  current_dir: string;
  current_file: string;
  progress: number;
  scanned: number;
  total: number;
  succeeded: number;
  failed: number;
}

export interface IndexStatusView {
  indexed_docs: number;
  indexed_chunks: number;
  scanned_docs: number;
  failed_files: number;
  current_task: CurrentTaskView | null;
  failed_items: FailedFileView[];
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
