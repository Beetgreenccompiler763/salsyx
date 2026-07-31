// TypeScript mirror of the ArchiveHub API contract (backend/openapi.json).

export interface RepositoryOwner {
  id: string;
  github_id: number;
  login: string;
  name?: string | null;
  avatar_url?: string | null;
  bio?: string | null;
  owner_type: string;
  created_at: string;
  updated_at: string;
}

export interface Repository {
  id: string;
  owner: RepositoryOwner;
  github_id: number;
  name: string;
  full_name: string;
  description?: string | null;
  homepage?: string | null;
  default_branch?: string | null;
  language?: string | null;
  license?: string | null;
  topics: string[];
  stars_count: number;
  forks_count: number;
  watchers_count: number;
  open_issues_count: number;
  commit_count: number;
  size_bytes: number;
  source: string;
  visibility: string;
  is_github_archived: boolean;
  is_deleted: boolean;
  deleted_at?: string | null;
  pushed_at?: string | null;
  github_created_at?: string | null;
  last_checked_at?: string | null;
  created_at: string;
  updated_at: string;
}

export interface StorageLocation {
  provider: string;
  key: string;
}

export type CompressionMethod = "zip" | "git_bundle" | "tar" | "custom";
export type ArchiveStatus =
  | "pending"
  | "fetching"
  | "processing"
  | "archived"
  | "verification_failed"
  | "failed";

export interface Archive {
  id: string;
  repository_id: string;
  commit_ref?: string | null;
  commit_count?: number | null;
  checksum: string;
  size_bytes: number;
  storage: StorageLocation;
  compression: CompressionMethod;
  status: ArchiveStatus;
  deleted_at?: string | null;
  error_message?: string | null;
  archived_at: string;
  created_at: string;
  updated_at: string;
}

export type RepoStatus = "live" | "archived" | "not_found" | "not_archived";

export interface RepoResponse {
  source: "github" | "archivehub";
  status: RepoStatus;
  repository?: Repository | null;
  archive?: Archive | null;
  download_url?: string | null;
  message?: string | null;
}

export interface SearchItem {
  id: string;
  owner: string;
  name: string;
  full_name: string;
  description?: string | null;
  language?: string | null;
  license?: string | null;
  topics: string[];
  stars_count: number;
  forks_count: number;
  is_deleted: boolean;
  has_archive: boolean;
  archived_at?: string | null;
  html_url?: string | null;
  last_checked_at?: string | null;
}

export interface SearchResponse {
  total: number;
  page: number;
  per_page: number;
  query: string;
  items: SearchItem[];
}

export interface StatsResponse {
  total_repositories: number;
  archived_repositories: number;
  total_archives: number;
  total_archived_bytes: number;
  total_downloads: number;
  deleted_archived: number;
  total_owners: number;
  indexed_bytes: number;
}

export interface HealthResponse {
  status: string;
  version: string;
  database: "ok" | "unreachable";
  uptime_secs: number;
}

export interface ErrorBody {
  code: string;
  message: string;
  detail?: string | null;
}

/** Seed data for the landing-page bubble field when the API is unreachable. */
export interface BubbleProfile {
  login: string;
  avatar?: string;
  name?: string;
  bio?: string;
  repos?: number;
  stars?: number;
  languages?: string[];
}

export function formatBytes(bytes: number): string {
  if (!bytes || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const val = bytes / Math.pow(1024, i);
  return `${val >= 100 ? val.toFixed(0) : val.toFixed(1)} ${units[i]}`;
}

export function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1).replace(/\.0$/, "")}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1).replace(/\.0$/, "")}k`;
  return String(n);
}

export function formatDate(iso?: string | null): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "—";
  return d.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
}
