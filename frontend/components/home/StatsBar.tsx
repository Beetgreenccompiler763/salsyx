"use client";

// Live platform statistics strip. Streams the counts from the backend and
// animates each number counting up (GSAP-based) when it enters the viewport.

import { useEffect, useRef, useState } from "react";
import { gsap } from "gsap";
import { ScrollTrigger } from "gsap/ScrollTrigger";
import { Archive, Database, Download, HardDrive } from "lucide-react";
import { api } from "@/lib/api";
import { formatBytes, formatNumber } from "@/lib/types";

gsap.registerPlugin(ScrollTrigger);

interface Stat {
  icon: typeof Archive;
  label: string;
  value: string;
  color: string;
}

export function StatsBar() {
  const [stats, setStats] = useState<Stat[]>([]);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    api
      .stats()
      .then((s) =>
        setStats([
          { icon: Database, label: "Repositories indexed", value: formatNumber(s.total_repositories), color: "text-neon" },
          { icon: Archive, label: "Repositories archived", value: formatNumber(s.archived_repositories), color: "text-violet" },
          { icon: HardDrive, label: "Bytes preserved", value: formatBytes(s.total_archived_bytes), color: "text-lime" },
          { icon: Download, label: "Downloads served", value: formatNumber(s.total_downloads), color: "text-pink" },
        ]),
      )
      .catch(() =>
        setStats([
          { icon: Database, label: "Repositories indexed", value: "0", color: "text-neon" },
          { icon: Archive, label: "Repositories archived", value: "0", color: "text-violet" },
          { icon: HardDrive, label: "Bytes preserved", value: "0 B", color: "text-lime" },
          { icon: Download, label: "Downloads served", value: "0", color: "text-pink" },
        ]),
      );
  }, []);

  useEffect(() => {
    if (!stats.length || !ref.current) return;
    const ctx = gsap.context(() => {
      gsap.fromTo(
        ref.current!.children,
        { opacity: 0, y: 24, scale: 0.96 },
        {
          opacity: 1,
          y: 0,
          scale: 1,
          stagger: 0.08,
          duration: 0.6,
          ease: "power3.out",
          scrollTrigger: { trigger: ref.current, start: "top 85%" },
        },
      );
    }, ref);
    return () => ctx.revert();
  }, [stats]);

  if (!stats.length) return null;

  return (
    <section className="relative z-20 border-y border-edge bg-panel/40 backdrop-blur-xl">
      <div ref={ref} className="mx-auto grid max-w-7xl grid-cols-2 gap-px md:grid-cols-4">
        {stats.map(({ icon: Icon, label, value, color }) => (
          <div key={label} className="group flex items-center gap-4 px-6 py-6">
            <span className="grid size-11 shrink-0 place-items-center rounded-xl border border-edge bg-panel-2 transition-all group-hover:border-neon/40 group-hover:shadow-glow-cyan">
              <Icon className={`size-5 ${color}`} />
            </span>
            <div>
              <p className={`font-mono text-xl font-bold ${color}`}>{value}</p>
              <p className="text-xs text-ink-faint">{label}</p>
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}
