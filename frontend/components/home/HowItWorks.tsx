"use client";

// The "How it works" section — a three-step journey shown as glass cards with
// staggered scroll animations.

import { motion } from "motion/react";
import Link from "next/link";
import { Search, Database, ArchiveRestore, MousePointerClick } from "lucide-react";

const STEPS = [
  {
    icon: Search,
    step: "01",
    title: "Search GitHub first",
    body: "Paste any public repository. Salsyx queries GitHub in real time — if it still exists, you're one click from it.",
    accent: "text-neon",
    ring: "group-hover:border-neon/50",
  },
  {
    icon: MousePointerClick,
    step: "02",
    title: "GitHub returns 404?",
    body: "If the repository has been deleted or made private, we fall back to our archive database instead of a dead link.",
    accent: "text-violet",
    ring: "group-hover:border-violet/50",
  },
  {
    icon: Database,
    step: "03",
    title: "Restore from the archive",
    body: "Browse files or download the preserved snapshot — full git history, commit refs, and checksums intact. Forever.",
    accent: "text-pink",
    ring: "group-hover:border-pink/50",
  },
];

export function HowItWorks() {
  return (
    <section className="relative z-20 mx-auto max-w-7xl px-6 py-24">
      <div className="text-center">
        <p className="text-xs font-semibold uppercase tracking-[0.3em] text-ink-faint">
          How it works
        </p>
        <h2 className="mt-3 text-3xl font-black tracking-tight md:text-5xl">
          Never lose a <span className="text-gradient">repository</span> again
        </h2>
      </div>

      <div className="mt-14 grid gap-6 md:grid-cols-3">
        {STEPS.map(({ icon: Icon, step, title, body, accent, ring }, i) => (
          <motion.div
            key={step}
            initial={{ opacity: 0, y: 40, filter: "blur(6px)" }}
            whileInView={{ opacity: 1, y: 0, filter: "blur(0px)" }}
            viewport={{ once: true, margin: "-80px" }}
            transition={{ duration: 0.55, delay: i * 0.12, ease: [0.22, 1, 0.36, 1] }}
            className={`glass group relative overflow-hidden rounded-2xl p-6 transition-all duration-300 hover:-translate-y-1.5 ${ring}`}
          >
            <div className="flex items-center justify-between">
              <span className={`grid size-12 place-items-center rounded-xl border border-edge bg-panel-2 ${accent}`}>
                <Icon className="size-6" />
              </span>
              <span className="font-mono text-4xl font-black text-ink-faint/40">{step}</span>
            </div>
            <h3 className="mt-5 text-lg font-bold">{title}</h3>
            <p className="mt-2 text-sm leading-relaxed text-ink-dim">{body}</p>
          </motion.div>
        ))}
      </div>

      <div className="mt-14 flex justify-center">
        <Link
          href="/search"
          className="group relative overflow-hidden rounded-full border border-edge bg-panel/70 px-8 py-3.5 text-sm font-semibold transition-all hover:border-neon/60 hover:shadow-glow-cyan"
        >
          <span className="relative z-10 flex items-center gap-2">
            Try it now
            <ArchiveRestore className="size-4 text-neon transition-transform group-hover:translate-x-1" />
          </span>
        </Link>
      </div>
    </section>
  );
}
