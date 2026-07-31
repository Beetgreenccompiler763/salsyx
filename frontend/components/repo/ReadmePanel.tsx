"use client";

// README preview panel. Serves the preserved copy when available (so a
// deleted repository's README still renders) and falls back to a live fetch.

import { useEffect, useState } from "react";
import { BookOpen, ShieldCheck, Globe, FileText } from "lucide-react";
import { api } from "@/lib/api";
import type { ReadmeResponse } from "@/lib/types";

export function ReadmePanel({ owner, repo }: { owner: string; repo: string }) {
  const [data, setData] = useState<ReadmeResponse | null>(null);
  const [error, setError] = useState(false);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let active = true;
    setLoading(true);
    setError(false);
    setData(null);
    api
      .readme(owner, repo)
      .then((res) => active && setData(res))
      .catch(() => active && setError(true))
      .finally(() => active && setLoading(false));
    return () => {
      active = false;
    };
  }, [owner, repo]);

  if (loading) {
    return (
      <div className="glass mt-6 overflow-hidden rounded-2xl">
        <div className="flex items-center gap-2 border-b border-edge px-5 py-3">
          <BookOpen className="size-4 text-neon" />
          <p className="text-xs uppercase tracking-wider text-ink-faint">README</p>
        </div>
        <div className="space-y-2 p-5">
          <div className="h-4 w-1/2 animate-pulse rounded bg-panel-2/70" />
          <div className="h-4 w-3/4 animate-pulse rounded bg-panel-2/60" />
          <div className="h-4 w-2/3 animate-pulse rounded bg-panel-2/60" />
        </div>
      </div>
    );
  }

  if (error || !data) return null;

  return (
    <div className="glass mt-6 overflow-hidden rounded-2xl">
      <div className="flex items-center justify-between gap-2 border-b border-edge px-5 py-3">
        <div className="flex items-center gap-2">
          <BookOpen className="size-4 text-neon" />
          <p className="text-xs uppercase tracking-wider text-ink-faint">README</p>
          {data.source === "salsyx" && (
            <span className="flex items-center gap-1 rounded-full bg-lime/10 px-2 py-0.5 text-[10px] text-lime">
              <ShieldCheck className="size-3" /> preserved
            </span>
          )}
          {data.source === "github" && (
            <span className="flex items-center gap-1 rounded-full bg-cyan/10 px-2 py-0.5 text-[10px] text-cyan">
              <Globe className="size-3" /> live
            </span>
          )}
        </div>
      </div>
      <div className="max-h-[480px] overflow-auto">
        <pre className="whitespace-pre-wrap break-words p-5 font-mono text-[13px] leading-relaxed text-ink-dim">
          {data.readme.trim() || <span className="text-ink-faint">No README content.</span>}
        </pre>
      </div>
    </div>
  );
}

export function ReadmePlaceholder({ className = "" }: { className?: string }) {
  return (
    <div className={`glass mt-6 overflow-hidden rounded-2xl ${className}`}>
      <div className="flex items-center gap-2 border-b border-edge px-5 py-3">
        <FileText className="size-4 text-ink-faint" />
        <p className="text-xs uppercase tracking-wider text-ink-faint">README</p>
      </div>
      <div className="flex items-center gap-3 p-6 text-sm text-ink-faint">
        <BookOpen className="size-5" /> No README preserved for this snapshot.
      </div>
    </div>
  );
}
