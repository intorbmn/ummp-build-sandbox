// Shapes returned by the Rust commands. Rust structs and golden JSON are the source of truth.

export type RiskLevel = 'low' | 'medium' | 'high';
export type JobState =
  | 'Pending'
  | 'Running'
  | 'Succeeded'
  | 'Failed'
  | 'Skipped'
  | 'TimedOut'
  | 'Cancelled';

export interface ModView {
  id: string;
  name: string;
  version: string;
  loader: string;
  minecraft: string;
  java: number | null;
}

export type Handling = 'auto' | 'assisted' | 'manual';

export interface TechFinding {
  tech: string;
  handling: Handling;
  file: string;
  line: number | null;
  message: string;
}

export interface SkippedView {
  description: string;
  reason: string;
}

export interface AnalysisView {
  kind: 'folder' | 'zip' | 'jar';
  mods: ModView[];
  build_system: string | null;
  languages: string[];
  technologies: TechFinding[];
}

export interface FindingView {
  rule: string;
  severity: RiskLevel;
  file: string;
  evidence: string;
}

export interface RiskView {
  level: RiskLevel | null;
  findings: FindingView[];
}

export interface SessionView {
  id: string;
  state: JobState;
  step: string;
  output: string | null;
  error: string | null;
}

export interface TargetVersion {
  minecraft: string;
  loaders: string[];
}

export interface TargetsView {
  loaders: string[];
  versions: TargetVersion[];
}

export type FileKind = 'modified' | 'created';

export interface FileDiffView {
  path: string;
  kind: FileKind;
  diff: string;
  replacements: number;
  needs_attention: boolean;
}

export interface PlanView {
  source_minecraft: string;
  target_minecraft: string;
  source_loader: string | null;
  target_loader: string;
  files: FileDiffView[];
  moves: { from: string; to: string }[];
  deletes: string[];
  technologies: TechFinding[];
  skipped: SkippedView[];
  warnings: string[];
  files_scanned: number;
}

export interface ReportView {
  files_changed: number;
  replacements: number;
  moves: number;
  deletes: number;
  skipped_mappings: number;
  breaking_changes: number;
  warnings: string[];
  technologies: TechFinding[];
  checkpoint_id: string | null;
}

export interface ProgressView {
  done: number;
  total: number;
}

export interface JobApprovalView {
  job: string;
  approval: string;
  path: string;
  diff: string;
  reviewer_approved: boolean;
  concerns: string[];
}

export interface JobView {
  id: string;
  session: string;
  step: string;
  state: JobState;
  progress: ProgressView | null;
  log: string[];
  result: Record<string, unknown> | null;
  error: string | null;
}

export interface PlanRequest {
  input: string;
  from: string | null;
  to: string;
  loader: string | null;
  min_confidence: number;
}

export interface PortRequest {
  input: string;
  output: string | null;
  to: string;
  from: string | null;
  loader: string | null;
  zip: boolean;
  min_confidence: number;
}

export interface SettingsView {
  data_dir: string;
  kb_path: string | null;
  output_root: string | null;
  sandbox_repo: string | null;
  ai_provider: string | null;
  language: 'vi' | 'en';
}

export interface BuildJobRequest {
  path: string;
  local: boolean;
  repo: string | null;
  stages: string[];
  timeout_secs: number;
  java: number | null;
  workdir: string | null;
  accept_high_risk: boolean;
  confirm_medium: boolean;
}

export interface PackRequest {
  input: string;
  to: string;
  loader: string;
  from_loader: string | null;
  from_mc: string | null;
  online: boolean;
  sources: string | null;
  side: 'client' | 'server' | 'both' | null;
  output: string | null;
  allow_risk: boolean;
  loader_version: string | null;
  formats: string[];
  min_confidence: number;
}

export interface PackItemView {
  path: string;
  mod_id: string;
  action: { action: string; [key: string]: unknown };
  reason: string;
}

export interface UiError {
  kind: 'input' | 'refused' | 'not_found' | 'cancelled' | 'internal';
  message: string;
}

/** Core's event envelope forwarded by the Tauri shell. */
export interface CoreEvent {
  id: string;
  kind: string;
  payload: Record<string, unknown>;
  timestamp: string;
}


export interface AiKeyStatus {
  saved: boolean;
  masked: string | null;
  base_url: string | null;
}

export interface AiKeySaved {
  masked: string;
  warning: string | null;
}
