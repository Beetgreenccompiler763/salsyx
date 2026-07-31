// Thin, typed client for the Salsyx REST API.
//
// The Next.js dev/prod proxy (next.config.mjs) forwards `/api/*` to the Rust
// backend, so the frontend always calls relative URLs and stays deployable
// on Cloudflare Pages / Vercel behind a reverse proxy.

import type {
  Archive,
  ErrorBody,
  HealthResponse,
  RepoResponse,
  SearchResponse,
  StatsResponse,
} from "./types";

export class ApiError extends Error {
  code: string;
  status: number;

  constructor(status: number, body: ErrorBody) {
    super(body.message);
    this.name = "ApiError";
    this.code = body.code;
    this.status = status;
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    headers: { "Content-Type": "application/json", ...(init?.headers ?? {}) },
    ...init,
    // Stream-friendly: keep connections warm, the backend sets Cache-Control.
    cache: init?.cache ?? "no-store",
  });

  if (!res.ok) {
    let body: ErrorBody = { code: "internal_error", message: res.statusText };
    try {
      body = (await res.json()) as ErrorBody;
    } catch {
      /* non-JSON error body */
    }
    throw new ApiError(res.status, body);
  }

  return (await res.json()) as T;
}

export interface SearchParams {
  q?: string;
  mode?: "exact" | "partial" | "fuzzy" | "full_text";
  owner?: string;
  language?: string;
  license?: string;
  topics?: string;
  min_stars?: number;
  include_deleted?: boolean;
  archived_only?: boolean;
  sort?: "relevance" | "stars" | "forks" | "name" | "updated_at" | "archived_at" | "commit_count";
  order?: "asc" | "desc";
  page?: number;
  per_page?: number;
}

export const api = {
  health(): Promise<HealthResponse> {
    return request<HealthResponse>("/api/v1/health");
  },

  search(params: SearchParams = {}): Promise<SearchResponse> {
    const query = new URLSearchParams();
    if (params.q) query.set("q", params.q);
    if (params.mode) query.set("mode", params.mode);
    if (params.owner) query.set("owner", params.owner);
    if (params.language) query.set("language", params.language);
    if (params.license) query.set("license", params.license);
    if (params.topics) query.set("topics", params.topics);
    if (params.min_stars != null) query.set("min_stars", String(params.min_stars));
    if (params.include_deleted) query.set("include_deleted", "true");
    if (params.archived_only) query.set("archived_only", "true");
    if (params.sort) query.set("sort", params.sort);
    if (params.order) query.set("order", params.order);
    query.set("page", String(params.page ?? 1));
    query.set("per_page", String(params.per_page ?? 20));

    return request<SearchResponse>(`/api/v1/search?${query.toString()}`);
  },

  repo(owner: string, repo: string): Promise<RepoResponse> {
    return request<RepoResponse>(`/api/v1/repo/${encodeURIComponent(owner)}/${encodeURIComponent(repo)}`);
  },

  archive(id: string): Promise<Archive> {
    return request<{ archive: Archive; download_url: string }>(`/api/v1/archive/${id}`).then(
      (r) => r.archive,
    );
  },

  stats(): Promise<StatsResponse> {
    return request<StatsResponse>("/api/v1/stats");
  },

  requestArchive(fullName: string): Promise<{ archive_id: string; status: string }> {
    return request<{ archive_id: string; status: string }>("/api/v1/archive", {
      method: "POST",
      body: JSON.stringify({ full_name: fullName }),
    });
  },

  downloadUrl(id: string): string {
    return `/api/v1/download/${id}`;
  },
};
