/* v8 ignore file -- type-only declarations, no runtime code */

export interface StudyPackSummary {
  id: string;
  name: string;
  authority_source: string | null;
  description: string | null;
  created_at: number;
  updated_at: number;
  item_count: number;
  indexed_count: number;
  needs_index_count: number;
  chunk_count: number;
  embedded_count: number;
  needs_embedding_count: number;
  active: boolean;
}

export interface StudyPackSummaryResponse {
  pack: StudyPackSummary;
}

export interface SaveContextResponse {
  item_id: string;
  chunks_saved: number;
  title: string;
  image_paths: string[];
}

export interface MlxVlmStatus {
  supported: boolean;
  apple_silicon: boolean;
  python_available: boolean;
  runtime_path: string | null;
  venv_python: string | null;
  package_installed: boolean;
  model_id: string;
  model_cached: boolean;
  ready: boolean;
  installed_versions: string | null;
  error: string | null;
}

export interface MlxVlmInstallResult {
  installed: boolean;
  status: MlxVlmStatus;
  message: string;
}

export interface MlxVlmDescribeResponse {
  model_id: string;
  notes: string;
}

export interface RetrievedContextChunk {
  id: string;
  item_id: string;
  source_id: string;
  source_label: string;
  chunk_text: string;
  score: number;
}

export interface RetrieveStudyContextResponse {
  pack: StudyPackSummary | null;
  chunks: RetrievedContextChunk[];
  context_block: string;
}

export interface ContextPromptResponse {
  prompt: string;
  context: RetrieveStudyContextResponse;
  enough_context: boolean;
}

export interface StudyPackIndexResponse {
  pack_id: string;
  total_items: number;
  indexed_items: number;
  chunks_saved: number;
}

export interface StudyPackEmbeddingIndexResponse {
  pack_id: string;
  model_id: string;
  total_chunks: number;
  embedded_chunks: number;
}
