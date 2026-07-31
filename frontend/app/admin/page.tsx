"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import { Gauge, KeyRound, Lock, RefreshCw } from "lucide-react";
import { api } from "@/lib/api";
import { formatBytes, type AdminOverview } from "@/lib/types";

const TOKEN_KEY = "salsyx.admin.token";

export default function AdminPage() {
  const [token, setToken] = useState("");
  const [unlocked, setUnlocked] = useState(false);
  const [data, setData] = useState<AdminOverview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    const saved = window.localStorage.getItem(TOKEN_KEY);
    if (saved) {
      setToken(saved);
      setUnlocked(true);
    }
  }, []);

  useEffect(() => {
    if (!unlocked) return;
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [unlocked]);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const overview = await api.adminOverview(token);
      setData(overview);
    } catch (e) {
      setError(e instanceof Error ? e.message : "failed to load overview");
    } finally {
      setLoading(false);
    }
  };

  const unlock = () => {
    window.localStorage.setItem(TOKEN_KEY, token);
    setUnlocked(true);
  };

  const lock = () => {
    window.localStorage.removeItem(TOKEN_KEY);
    setToken("");
    setUnlocked(false);
    setData(null);
  };

  return (
    <div className="mx-auto max-w-6xl px-6 pb-24 pt-28">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <span className="grid size-10 place-items-center rounded-xl border border-edge bg-panel-2">
            <Gauge className="size-5 text-neon" />
          </span>
          <div>
            <h1 className="text-2xl font-black tracking-tight">Operations</h1>
            <p className="text-xs text-ink-dim">Read-only platform overview</p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          {unlocked && (
            <>
              <button
                onClick={load}
                disabled={loading}
                className="glass flex items-center gap-2 rounded-lg px-3 py-2 text-xs font-semibold transition hover:border-neon/50 disabled:opacity-50"
              >
                <RefreshCw className={`size-3.5 ${loading ? "animate-spin" : ""}`} />
                Refresh
              </button>
              <button
                onClick={lock}
                className="glass flex items-center gap-2 rounded-lg px-3 py-2 text-xs font-semibold text-ink-dim transition hover:border-pink/50"
              >
                <Lock className="size-3.5" />
                Lock
              </button>
            </>
          )}
        </div>
      </div>

      {!unlocked ? (
        <div className="glass mx-auto mt-16 max-w-md rounded-2xl p-8">
          <div className="mx-auto grid size-12 place-items-center rounded-full border border-edge bg-panel-2">
            <KeyRound className="size-5 text-lime" />
          </div>
          <h2 className="mt-4 text-center text-lg font-bold">Admin token required</h2>
          <p className="mt-2 text-center text-sm text-ink-dim">
            This dashboard is gated by <code className="text-neon">AH_SERVER__ADMIN_TOKEN</code>.
            The token is only stored in this browser.
          </p>
          <input
            type="password"
            value={token}
            onChange={(e) => setToken(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && unlock()}
            placeholder="Bearer token"
            autoComplete="off"
            className="mt-6 w-full rounded-lg border border-edge bg-canvas px-4 py-3 text-sm outline-none transition focus:border-neon/60"
          />
          <button
            onClick={unlock}
            disabled={!token}
            className="mt-4 w-full rounded-lg bg-gradient-to-r from-neon to-violet px-4 py-3 text-sm font-bold text-canvas transition hover:opacity-90 disabled:opacity-40"
          >
            Unlock dashboard
          </button>
        </div>
      ) : error ? (
        <div className="glass mt-10 rounded-2xl p-6 text-center">
          <p className="text-sm text-pink">{error}</p>
          <p className="mt-2 text-xs text-ink-dim">
            Make sure the backend is reachable and the token matches{" "}
            <code className="text-neon">AH_SERVER__ADMIN_TOKEN</code>.
          </p>
        </div>
      ) : !data ? (
        <div className="mt-10 grid grid-cols-2 gap-4 md:grid-cols-4">
          {Array.from({ length: 8 }).map((_, i) => (
            <div key={i} className="glass h-24 animate-pulse rounded-2xl" />
          ))}
        </div>
      ) : (
        <>
          {/* Counts */}
          <div className="mt-8 grid grid-cols-2 gap-4 md:grid-cols-4">
            {[
              { label: "Repositories", value: data.counts?.total_repositories },
              { label: "Deleted", value: data.counts?.deleted_repositories },
              { label: "Archives", value: data.counts?.total_archives },
              { label: "Storage", value: formatBytes(data.counts?.total_storage_bytes ?? 0) },
            ].map((c) => (
              <div key={c.label} className="glass rounded-2xl p-5">
                <p className="text-[10px] font-bold uppercase tracking-[0.25em] text-ink-faint">
                  {c.label}
                </p>
                <p className="mt-2 text-2xl font-black tabular-nums">
                  {c.value ?? "—"}
                </p>
              </div>
            ))}
          </div>

          <div className="mt-6 grid gap-6 lg:grid-cols-2">
            {/* Queue */}
            <div className="glass rounded-2xl p-5">
              <h2 className="text-sm font-bold uppercase tracking-widest text-ink-dim">
                Crawl queue
              </h2>
              <p className="mt-1 text-xs text-ink-faint">
                {data.pending_jobs} pending jobs
              </p>
              <div className="mt-4 space-y-2">
                {data.job_breakdown.length === 0 && (
                  <p className="text-xs text-ink-faint">No jobs recorded yet.</p>
                )}
                {data.job_breakdown.map((j) => (
                  <div
                    key={`${j.job_type}:${j.status}`}
                    className="flex items-center justify-between rounded-lg border border-edge bg-panel-2 px-3 py-2 text-sm"
                  >
                    <span className="font-mono text-xs">{j.job_type}</span>
                    <span className="flex items-center gap-2">
                      <span
                        className={`rounded-full px-2 py-0.5 text-[10px] font-bold uppercase tracking-wider ${
                          j.status === "archived"
                            ? "bg-lime/10 text-lime"
                            : j.status === "failed"
                              ? "bg-pink/10 text-pink"
                              : "bg-neon/10 text-neon"
                        }`}
                      >
                        {j.status}
                      </span>
                      <span className="w-8 text-right font-mono tabular-nums">{j.count}</span>
                    </span>
                  </div>
                ))}
              </div>
            </div>

            {/* Stack */}
            <div className="glass rounded-2xl p-5">
              <h2 className="text-sm font-bold uppercase tracking-widest text-ink-dim">
                Stack
              </h2>
              <dl className="mt-4 space-y-3 text-sm">
                <div className="flex items-center justify-between">
                  <dt className="text-ink-dim">Storage provider</dt>
                  <dd className="font-mono text-xs">{data.storage.provider}</dd>
                </div>
                <div className="flex items-center justify-between">
                  <dt className="text-ink-dim">Chunk namespace</dt>
                  <dd className="font-mono text-xs">{data.storage.key_namespace}/*</dd>
                </div>
                <div className="flex items-center justify-between">
                  <dt className="text-ink-dim">Archive format</dt>
                  <dd className="font-mono text-xs">
                    {data.formats.filter((f) => f.default).map((f) => f.name).join(", ")}
                  </dd>
                </div>
              </dl>
              <h2 className="mt-6 text-sm font-bold uppercase tracking-widest text-ink-dim">
                Providers
              </h2>
              <ul className="mt-3 space-y-2">
                {data.providers.map((p) => (
                  <li key={p.name} className="flex items-center justify-between text-sm">
                    <span className="font-mono text-xs">{p.name}</span>
                    {p.enabled ? (
                      <span className="rounded-full bg-lime/10 px-2 py-0.5 text-[10px] font-bold uppercase tracking-wider text-lime">
                        enabled
                      </span>
                    ) : (
                      <span className="rounded-full bg-ink-faint/10 px-2 py-0.5 text-[10px] font-bold uppercase tracking-wider text-ink-faint">
                        disabled
                      </span>
                    )}
                  </li>
                ))}
              </ul>
            </div>
          </div>

          {/* Recent archives */}
          <div className="glass mt-6 rounded-2xl p-5">
            <h2 className="text-sm font-bold uppercase tracking-widest text-ink-dim">
              Recent archives
            </h2>
            <div className="mt-4 overflow-x-auto">
              <table className="w-full text-left text-sm">
                <thead>
                  <tr className="border-b border-edge text-[10px] uppercase tracking-widest text-ink-faint">
                    <th className="pb-2 pr-4 font-bold">Repository</th>
                    <th className="pb-2 pr-4 font-bold">Status</th>
                    <th className="pb-2 pr-4 font-bold">Format</th>
                    <th className="pb-2 pr-4 text-right font-bold">Size</th>
                    <th className="pb-2 text-right font-bold">Captured</th>
                  </tr>
                </thead>
                <tbody>
                  {data.recent_archives.length === 0 && (
                    <tr>
                      <td colSpan={5} className="pt-4 text-xs text-ink-faint">
                        Nothing archived yet.
                      </td>
                    </tr>
                  )}
                  {data.recent_archives.map((a) => (
                    <tr key={a.id} className="border-b border-edge/50 last:border-0">
                      <td className="py-2.5 pr-4">
                        <Link
                          href={`/repo/${a.full_name}`}
                          className="font-mono text-xs text-neon hover:underline"
                        >
                          {a.full_name}
                        </Link>
                      </td>
                      <td className="py-2.5 pr-4">
                        <span
                          className={`rounded-full px-2 py-0.5 text-[10px] font-bold uppercase tracking-wider ${
                            a.status === "archived"
                              ? "bg-lime/10 text-lime"
                              : a.status === "failed"
                                ? "bg-pink/10 text-pink"
                                : "bg-neon/10 text-neon"
                          }`}
                        >
                          {a.status}
                        </span>
                      </td>
                      <td className="py-2.5 pr-4 font-mono text-xs text-ink-dim">
                        {a.compression_method}
                      </td>
                      <td className="py-2.5 pr-4 text-right font-mono tabular-nums text-xs">
                        {a.size_bytes != null ? formatBytes(a.size_bytes) : "—"}
                      </td>
                      <td className="py-2.5 text-right font-mono text-xs text-ink-dim">
                        {a.archived_at
                          ? new Date(a.archived_at).toLocaleString(undefined, {
                              dateStyle: "medium",
                              timeStyle: "short",
                            })
                          : "—"}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
