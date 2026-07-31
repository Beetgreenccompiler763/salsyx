"use client";

// "Trending deleted" — repositories that recently disappeared from GitHub but
// live on in the Salsyx archive. Surfaced on the homepage per the spec.

import { useEffect, useState } from "react";
import Link from "next/link";
import { Trash2, Star, Ghost, ArrowRight } from "lucide-react";
import { api } from "@/lib/api";
import { formatDate, formatNumber, type SearchItem } from "@/lib/types";

export function TrendingDeleted() {
  const [items, setItems] = useState<SearchItem[] | null>(null);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    let active = true;
    api
      .search({
        include_deleted: true,
        archived_only: true,
        sort: "archived_at",
        order: "desc",
        per_page: 6,
      })
      .then((res) => active && setItems(res.items.length ? res.items : null))
      .catch(() => active && setItems(null))
      .finally(() => active && setLoaded(true));
    return () => {
      active = false;
    };
  }, []);

  if (!loaded) return null;
  if (!items || items.length === 0) return null;

  return (
    <section className="mx-auto w-full max-w-7xl px-6 py-16">
      <div className="mb-6 flex items-center justify-between gap-4">
        <div>
          <h2 className="flex items-center gap-2 text-xl font-black tracking-tight md:text-2xl">
            <Ghost className="size-6 text-pink" />
            Trending deleted — <span className="text-gradient">still alive here</span>
          </h2>
          <p className="mt-1 text-sm text-ink-dim">
            Recently disappeared from GitHub, permanently preserved in the archive.
          </p>
        </div>
        <Link
          href="/search?include_deleted=true&archived_only=true&sort=archived_at"
          className="flex shrink-0 items-center gap-1.5 text-sm text-ink-faint transition-colors hover:text-neon"
        >
          View all <ArrowRight className="size-4" />
        </Link>
      </div>

      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
        {items.map((item) => (
          <Link
            key={item.id}
            href={`/repo/${item.owner}/${item.name}`}
            className="group glass relative overflow-hidden rounded-2xl p-5 transition-all hover:border-pink/40"
          >
            <div className="flex items-center justify-between gap-2">
              <p className="truncate font-mono text-sm font-semibold transition-colors group-hover:text-neon">
                {item.full_name}
              </p>
              <span className="flex shrink-0 items-center gap-1 rounded-full bg-pink/10 px-2 py-0.5 text-[10px] text-pink">
                <Trash2 className="size-3" /> deleted
              </span>
            </div>
            <p className="mt-1.5 line-clamp-2 min-h-8 text-xs text-ink-dim">
              {item.description ?? "No description."}
            </p>
            <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-ink-faint">
              {item.language && (
                <span className="flex items-center gap-1.5">
                  <span className="size-2 rounded-full bg-violet" /> {item.language}
                </span>
              )}
              <span className="flex items-center gap-1">
                <Star className="size-3.5 text-amber" /> {formatNumber(item.stars_count)}
              </span>
              {item.archived_at && <span>archived {formatDate(item.archived_at)}</span>}
            </div>
          </Link>
        ))}
      </div>
    </section>
  );
}
