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

export interface FailedFileView {
  file: string;
  reason: string;
}

export interface CurrentTaskView {
  label: string;
  details: string;
  progress: number;
  scanned: number;
  total: number;
}

export interface IndexStatusView {
  indexed_docs: number;
  indexed_chunks: number;
  scanned_docs: number;
  failed_files: number;
  current_task: CurrentTaskView | null;
  failed_items: FailedFileView[];
}
