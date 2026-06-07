/**
 * @author MorningSun
 * @CreatedDate 2026/06/03
 * @Description SeekMind 前端视图类型定义。
 */
export interface IndexDirView {
  path: string;
  enabled: boolean;
  docs: number;
  chunks: number;
  status: string;
  is_explicit: boolean;
}

export interface ImportedPathView {
  path: string;
  dir_path: string;
  is_virtual: boolean;
}

export interface ImportPathsView {
  added_dirs: string[];
  imported_files: ImportedPathView[];
  virtual_dir: string;
  skipped: string[];
  unsupported: string[];
}

export interface SearchResultView {
  id: string;
  file_name: string;
  path: string;
  ext: string;
  heading: string;
  title_path: string;
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
  rank_reason: SearchRankReasonView;
  preview_blocks?: PreviewBlockView[];
}

export interface SearchRankReasonView {
  summary: string;
  match_origin: string;
  boosts: string[];
  keyword_hit: boolean;
  semantic_hit: boolean;
  favorite_boost: boolean;
  recent_open_count: number;
  history_expanded: boolean;
  keyword_score: number;
  semantic_score: number;
  title_score: number;
  filename_score: number;
  preference_score: number;
  base_score: number;
  raw_score: number;
  original_rank: number;
  final_rank: number;
  rank_delta: number;
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

export interface PreviewBlockView {
  block_index: number;
  block_type: string;
  text: string;
  heading: string;
  level?: number | null;
  page?: number | null;
  language?: string | null;
  markdown: string;
  html: string;
  asset_path?: string;
  alt_text?: string;
  caption?: string;
  ocr_text?: string;
}

export interface ChunkView {
  id: string;
  heading: string;
  title_path: string;
  snippet: string;
  paragraph?: number | null;
  page?: number | null;
  preview_blocks?: PreviewBlockView[];
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
  started_at: number;
  progress: number;
  scanned: number;
  total: number;
  succeeded: number;
  failed: number;
  updated: number;
  skipped: number;
  deleted: number;
  warning?: string | null;
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
  pdf_ocr_tasks: number;
  scanned_docs: number;
  failed_files: number;
  current_task: CurrentTaskView | null;
  failed_items: FailedFileView[];
  last_run: IndexRunSummaryView | null;
}

export interface DocumentRefreshStartView {
  job_id: string;
  status: IndexStatusView;
}

export interface DocumentRefreshProgressView {
  job_id: string;
  state: string;
  message: string;
  path: string;
  file_name: string;
  parser_source: "python" | "rust";
  warning: string | null;
  status: IndexStatusView;
  updated_at: string;
}

export interface IndexRefreshStartView {
  job_id: string;
  status: IndexStatusView;
}

export interface IndexRefreshProgressView {
  job_id: string;
  state: string;
  message: string;
  scope: string;
  path: string;
  status: IndexStatusView;
  updated_at: string;
}

export interface ParserRuntimeView {
  enabled: boolean;
  available: boolean;
  active: "python" | "rust";
  system_locale: string;
  system_language: string;
  vision_ocr_languages: string[];
  chinese_ocr_available: boolean;
  chinese_ocr_warning?: string | null;
  pdf_ocr_available: boolean;
  pdf_ocr_message: string;
  python_bin: string;
  script_path: string;
  timeout_ms: number;
  office_enabled: boolean;
  office_available: boolean;
  office_bin: string;
  office_message: string;
  office_platform: string;
}

export interface SearchDebugView {
  query: string;
  normalized_terms: string[];
  normalized_search_text: string;
  rewritten_query: string;
  rewritten_terms: string[];
  query_rewrite_applied: boolean;
  history_terms: string[];
  history_rewrite_applied: boolean;
  expanded_query: string;
  sqlite_documents: number;
  sqlite_chunks: number;
  tantivy_documents: number;
  semantic_enabled: boolean;
  semantic_weight: number;
  semantic_threshold: number;
  keyword_hit_count: number;
  semantic_hit_count: number;
  semantic_candidate_count: number;
  semantic_filtered_count: number;
  semantic_fallback: boolean;
  semantic_fallback_reason: string;
  search_mode: string;
  hit_count: number;
  hits: SearchResultView[];
}

export interface SearchDebugReportEventView {
  request_id: string;
  state: 'running' | 'completed' | 'failed';
  query: string;
  report: SearchDebugView | null;
  error: string | null;
  updated_at: string;
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

export interface RecentViewEntry {
  target_type: string;
  target_id: string;
  title: string;
  path: string;
  viewed_at: string;
}

export interface TagView {
  id: string;
  name: string;
  color: string;
  target_count: number;
  created_at: string;
  updated_at: string;
}

export interface TagPatchInput {
  name?: string;
  color?: string;
}

export interface FavoriteView {
  favorite_type: string;
  target: string;
  title: string;
  path: string;
  created_at: string;
  updated_at: string;
}

export interface CollectionView {
  id: string;
  name: string;
  description: string;
  color: string;
  sort_order: number;
  item_count: number;
  created_at: string;
  updated_at: string;
}

export interface CollectionItemView {
  id: string;
  collection_id: string;
  item_type: string;
  document_id: string;
  chunk_id: string;
  qa_session_id: string;
  qa_message_id: string;
  title: string;
  path: string;
  title_path: string;
  snippet: string;
  note: string;
  source_meta_json: string;
  sort_order: number;
  created_at: string;
  updated_at: string;
}

export interface CollectionPatchInput {
  name?: string | null;
  description?: string | null;
  color?: string | null;
}

export interface CollectionItemInput {
  collection_id: string;
  item_type: string;
  document_id?: string | null;
  chunk_id?: string | null;
  qa_session_id?: string | null;
  qa_message_id?: string | null;
  title: string;
  path?: string | null;
  title_path?: string | null;
  snippet?: string | null;
  note?: string | null;
  source_meta_json?: string | null;
}

export interface CollectionItemPatchInput {
  note?: string | null;
}

export interface QaSettingsView {
  enabled: boolean;
  provider: string;
  base_url: string;
  api_key: string;
  model: string;
  temperature: number;
  max_output_tokens: number;
  context_chunk_limit: number;
  context_token_budget: number;
  min_evidence_count: number;
  min_retrieval_score: number;
  updated_at: string;
}

export interface NetworkProxySettingsView {
  enabled: boolean;
  proxy_url: string;
  updated_at: string;
}

export interface QaModelProfileView {
  id: string;
  name: string;
  provider: string;
  base_url: string;
  api_key: string;
  model: string;
  enabled: boolean;
  is_default: boolean;
  updated_at: string;
  created_at: string;
}

export interface QaModelProfileUpsertView {
  id?: string | null;
  name: string;
  provider: string;
  base_url: string;
  api_key: string;
  model: string;
  enabled: boolean;
  is_default?: boolean;
}

export interface QaSourceView {
  source_id: string;
  chunk_id: string;
  file_name: string;
  path: string;
  ext: string;
  title_path: string;
  heading: string;
  paragraph?: number | null;
  page?: number | null;
  snippet: string;
  score: number;
  rank_reason: string;
  preview_blocks?: PreviewBlockView[];
}

export interface QaRetrievalView {
  search_mode: string;
  candidate_count: number;
  selected_count: number;
  semantic_enabled: boolean;
  semantic_fallback: boolean;
  semantic_fallback_reason: string;
}

export interface QaAnswerView {
  id: string;
  question: string;
  answer: string;
  state: string;
  sources: QaSourceView[];
  retrieval: QaRetrievalView;
  model: string;
  created_at: string;
  error?: string | null;
  warning?: string | null;
}

export interface QaHistoryView extends QaAnswerView {}

export interface QaSessionView {
  id: string;
  title: string;
  message_count: number;
  created_at: string;
  updated_at: string;
}

export interface QaMessageView extends QaAnswerView {
  session_id: string;
  updated_at: string;
}

export interface QaConnectionTestView {
  ok: boolean;
  message: string;
}

export interface QaAskStartView {
  job_id: string;
  status: QaAnswerView;
}

export interface QaAnswerProgressView {
  job_id: string;
  state: string;
  question: string;
  answer: string;
  answer_delta?: string;
  sources: QaSourceView[];
  retrieval: QaRetrievalView;
  model: string;
  error?: string | null;
  warning?: string | null;
  updated_at: string;
}

export interface IndexSettingsView {
  exclude_dirs: string[];
  exclude_exts: string[];
  max_file_size_mb: number;
  semantic_search_enabled: boolean;
  semantic_weight: number;
  semantic_threshold: number;
  title_weight: number;
  filename_weight: number;
  preference_weight: number;
  prefer_favorites_enabled: boolean;
  prefer_recent_enabled: boolean;
  prefer_history_enabled: boolean;
}

export interface EmbeddingModelView {
  id: string;
  name: string;
  provider: string;
  model_path: string;
  dimension: number;
  enabled: boolean;
  available: boolean;
  is_default: boolean;
  status: string;
  created_at: string;
  updated_at: string;
}

export interface SemanticModelStatusView {
  model: EmbeddingModelView;
  sqlite_chunks: number;
  embedded_chunks: number;
  needs_rebuild: boolean;
  last_indexed_at: string;
  last_error: string;
  index_status: string;
}

export interface SemanticRebuildStartView {
  job_id: string;
  status: SemanticModelStatusView;
}

export interface SemanticRebuildProgressView {
  job_id: string;
  state: string;
  message: string;
  source: 'rebuild' | 'document';
  model: EmbeddingModelView;
  total_chunks: number;
  processed_chunks: number;
  embedded_chunks: number;
  current_document: string;
  current_chunk: string;
  percent: number;
  last_error: string;
  updated_at: string;
}

export interface SemanticDebugHitView {
  chunk_id: string;
  document_path: string;
  file_name: string;
  heading: string;
  title_path: string;
  snippet: string;
  paragraph?: number | null;
  page?: number | null;
  score: number;
}

export interface SemanticDebugView {
  query: string;
  normalized_query: string;
  query_vector_dim: number;
  query_vector_ready: boolean;
  query_vector_norm: number;
  model: EmbeddingModelView;
  sqlite_chunks: number;
  embedded_chunks: number;
  hit_count: number;
  hits: SemanticDebugHitView[];
  index_status: string;
  last_error: string;
}
