"use client";

import { motion } from "motion/react";
import Link from "next/link";
import { useEffect, useState } from "react";
import { Archive, GitFork, Search } from "lucide-react";
import { api } from "@/lib/api";

/** Navbar with a live health indicator + real platform stats. */
export function Nav() {
  const [stats, setStats] = useState<{ repos?: number; archives?: number } | null>(null);
  const [scrolled, setScrolled] = useState(false);

  useEffect(() => {
    api
      .stats()
      .then((s) => setStats({ repos: s.total_repositories, archives: s.total_archives }))
      .catch(() => setStats(null));

    const onScroll = () => setScrolled(window.scrollY > 16);
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, []);

  return (
    <motion.header
      initial={{ y: -48, opacity: 0 }}
      animate={{ y: 0, opacity: 1 }}
      transition={{ type: "spring", stiffness: 120, damping: 18 }}
      className={`fixed inset-x-0 top-0 z-50 transition-all duration-300 ${
        scrolled ? "bg-canvas/70 backdrop-blur-xl border-b border-edge" : "bg-transparent"
      }`}
    >
      <nav className="mx-auto flex h-16 max-w-7xl items-center justify-between px-6">
        <Link href="/" className="group flex items-center gap-2.5">
          <span className="grid size-8 place-items-center rounded-lg border border-edge bg-panel-2 transition-all group-hover:border-neon/60 group-hover:shadow-glow-cyan">
            <Archive className="size-4 text-neon" />
          </span>
          <span className="text-sm font-semibold tracking-tight">
            Sal<span className="text-gradient">syx</span>
          </span>
        </Link>

        <div className="flex items-center gap-3">
          {stats && (
            <div className="hidden items-center gap-3 rounded-full border border-edge bg-panel/60 px-4 py-1.5 text-xs text-ink-dim md:flex">
              <span className="flex items-center gap-1.5">
                <span className="size-1.5 rounded-full bg-lime animate-pulse-glow" />
                {stats.repos?.toLocaleString()} repos
              </span>
              <span className="text-edge">·</span>
              <span className="flex items-center gap-1.5">
                <GitFork className="size-3.5 text-violet" />
                {stats.archives?.toLocaleString()} archived
              </span>
            </div>
          )}

          <Link
            href="/search"
            className="flex items-center gap-2 rounded-full border border-edge bg-panel/60 px-4 py-2 text-sm text-ink-dim transition-all hover:border-neon/50 hover:text-ink hover:shadow-glow-cyan"
          >
            <Search className="size-4" />
            <span className="hidden sm:inline">Search</span>
          </Link>
        </div>
      </nav>
    </motion.header>
  );
}
