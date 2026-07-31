"use client";

// Futuristic profile modal shown when a bubble pops. Loads the user's real
// archived repositories from Salsyx when available, otherwise shows a
// graceful fallback with their GitHub link.

import { motion, AnimatePresence } from "motion/react";
import { useEffect, useState } from "react";
import {
  X,
  Star,
  GitFork,
  Folder,
  ExternalLink,
  Code2,
  Flame,
  Github,
  Archive,
} from "lucide-react";
import { api } from "@/lib/api";
import { formatNumber, type BubbleProfile, type OwnerResponse, type SearchItem } from "@/lib/types";

export function BubbleModal({
  profile,
  onClose,
}: {
  profile: BubbleProfile | null;
  onClose: () => void;
}) {
  const [repos, setRepos] = useState<SearchItem[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [ownerData, setOwnerData] = useState<OwnerResponse | null>(null);

  useEffect(() => {
    if (!profile) return;
    setRepos(null);
    setLoading(true);
    setOwnerData(null);
    api
      .search({ owner: profile.login, per_page: 6 })
      .then((res) => setRepos(res.items.length ? res.items : null))
      .catch(() => setRepos(null))
      .finally(() => setLoading(false));

    api
      .owner(profile.login)
      .then(setOwnerData)
      .catch(() => setOwnerData(null));
  }, [profile]);

  const displayName = ownerData?.name ?? profile?.name ?? profile?.login ?? "";
  const displayBio = ownerData?.bio ?? profile?.bio;
  const displayAvatar = ownerData?.avatar_url ?? profile?.avatar;
  const followers = ownerData?.followers ?? 0;
  const following = ownerData?.following ?? 0;
  const repoCount = ownerData?.public_repos ?? profile?.repos ?? 0;
  const preserved = ownerData?.preserved_count ?? 0;
  const languages =
    ownerData && ownerData.top_repos.length
      ? Array.from(
          new Set(ownerData.top_repos.map((r) => r.language).filter((l): l is string => Boolean(l))),
        ).slice(0, 4)
      : profile?.languages ?? [];

  return (
    <AnimatePresence>
      {profile && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          className="fixed inset-0 z-[80] grid place-items-center bg-canvas/70 p-4 backdrop-blur-md"
          onClick={onClose}
          role="dialog"
          aria-modal="true"
        >
          <motion.div
            initial={{ opacity: 0, scale: 0.82, y: 24, filter: "blur(8px)" }}
            animate={{ opacity: 1, scale: 1, y: 0, filter: "blur(0px)" }}
            exit={{ opacity: 0, scale: 0.9, filter: "blur(8px)" }}
            transition={{ type: "spring", stiffness: 260, damping: 22 }}
            className="glass-strong neon-border w-full max-w-md rounded-2xl p-6"
            onClick={(e) => e.stopPropagation()}
          >
            {/* Header */}
            <div className="flex items-start justify-between">
              <div className="flex items-center gap-4">
                <div className="relative">
                  <div className="grid size-16 place-items-center overflow-hidden rounded-full border border-edge bg-panel-2">
                    {/* eslint-disable-next-line @next/next/no-img-element */}
                    <img
                      src={displayAvatar}
                      alt={profile.login}
                      className="size-full object-cover"
                      width={64}
                      height={64}
                    />
                  </div>
                  <span className="absolute -bottom-1 -right-1 grid size-6 place-items-center rounded-full border border-edge bg-canvas">
                    <Github className="size-3.5 text-neon" />
                  </span>
                </div>
                <div>
                  <h2 className="text-lg font-bold tracking-tight">{displayName}</h2>
                  <p className="font-mono text-sm text-neon">@{profile.login}</p>
                </div>
              </div>
              <button
                onClick={onClose}
                aria-label="Close"
                className="grid size-8 place-items-center rounded-lg border border-edge text-ink-dim transition-all hover:border-pink/60 hover:text-pink"
              >
                <X className="size-4" />
              </button>
            </div>

            {displayBio && <p className="mt-3 text-sm text-ink-dim">{displayBio}</p>}

            {/* Stats row */}
            <div className="mt-5 grid grid-cols-3 gap-3">
              {[
                { icon: Folder, label: "Repos", value: formatNumber(repoCount) },
                { icon: Star, label: "Followers", value: formatNumber(followers) },
                { icon: Flame, label: "Following", value: formatNumber(following) },
              ].map(({ icon: Icon, label, value }) => (
                <div key={label} className="glass rounded-xl px-3 py-2.5">
                  <Icon className="size-4 text-violet" />
                  <p className="mt-1 text-sm font-semibold">{value}</p>
                  <p className="text-[10px] uppercase tracking-wider text-ink-faint">{label}</p>
                </div>
              ))}
            </div>

            {/* Preserved count + languages */}
            <div className="mt-4 flex flex-wrap items-center gap-2">
              {preserved > 0 && (
                <span className="flex items-center gap-1.5 rounded-full bg-lime/10 px-2.5 py-1 text-xs text-lime">
                  <Archive className="size-3.5" /> {formatNumber(preserved)} preserved
                </span>
              )}
              {languages.map((lang) => (
                <span
                  key={lang}
                  className="flex items-center gap-1 rounded-full border border-edge bg-panel px-2.5 py-0.5 text-xs text-ink-dim"
                >
                  <Code2 className="size-3 text-ink-faint" />
                  {lang}
                </span>
              ))}
            </div>

            {/* Archived repositories from Salsyx */}
            <div className="mt-5">
              <p className="mb-2 text-xs uppercase tracking-wider text-ink-faint">
                Preserved repositories
              </p>
              {loading && (
                <div className="space-y-2">
                  {[0, 1, 2].map((i) => (
                    <div key={i} className="h-12 animate-pulse rounded-xl bg-panel-2/70" />
                  ))}
                </div>
              )}
              {!loading && repos === null && (
                <p className="rounded-xl border border-edge bg-panel/50 px-4 py-3 text-sm text-ink-faint">
                  No repositories indexed for this account yet.
                </p>
              )}
              {repos && (
                <div className="space-y-2">
                  {repos.map((repo) => (
                    <a
                      key={repo.id}
                      href={`/repo/${repo.owner}/${repo.name}`}
                      className="group flex items-center justify-between gap-3 rounded-xl border border-edge bg-panel/60 px-4 py-2.5 transition-all hover:border-neon/40 hover:bg-panel-2"
                    >
                      <div className="min-w-0">
                        <p className="truncate text-sm font-medium group-hover:text-neon">
                          {repo.full_name}
                        </p>
                        <p className="truncate text-xs text-ink-faint">
                          {repo.language ?? "unknown"}
                        </p>
                      </div>
                      <div className="flex shrink-0 items-center gap-3 text-xs text-ink-dim">
                        <span className="flex items-center gap-1">
                          <Star className="size-3.5 text-amber" />
                          {formatNumber(repo.stars_count)}
                        </span>
                        <span className="flex items-center gap-1">
                          <GitFork className="size-3.5" />
                          {formatNumber(repo.forks_count)}
                        </span>
                        {repo.has_archive && (
                          <span className="rounded-full bg-lime/10 px-2 py-0.5 text-[10px] text-lime">
                            ARCHIVED
                          </span>
                        )}
                      </div>
                    </a>
                  ))}
                </div>
              )}
            </div>

            <a
              href={`https://github.com/${profile.login}`}
              target="_blank"
              rel="noreferrer"
              className="mt-5 flex items-center justify-center gap-2 rounded-xl bg-gradient-to-r from-cyan-500/20 via-violet-500/20 to-pink-500/20 px-4 py-3 text-sm font-medium transition-all hover:from-cyan-500/30 hover:via-violet-500/30 hover:to-pink-500/30"
            >
              View on GitHub <ExternalLink className="size-4" />
            </a>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
