"use client";

// The signature large animated search bar. Submits to the search page with
// GSAP-driven focus glow and a shimmering sweep. Uses `useTransition` for a
// snappy router push.

import { motion } from "motion/react";
import { useRouter } from "next/navigation";
import { useEffect, useRef, useState, type FormEvent } from "react";
import { ArrowRight, Search, Sparkles } from "lucide-react";

export function SearchBar({
  size = "lg",
  autoFocus = false,
  placeholder = "Search any public repository… e.g. torvalds/linux",
  defaultValue = "",
}: {
  size?: "lg" | "md";
  autoFocus?: boolean;
  placeholder?: string;
  defaultValue?: string;
}) {
  const router = useRouter();
  const [value, setValue] = useState(defaultValue);
  const [focused, setFocused] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  // GSAP-powered glow sweep on focus.
  useEffect(() => {
    if (!focused) return;
    const sweep = document.createElement("div");
    sweep.className = "shimmer pointer-events-none absolute inset-0 rounded-full";
    const parent = inputRef.current?.parentElement;
    if (!parent) return;
    parent.appendChild(sweep);
    const timer = setTimeout(() => sweep.remove(), 700);
    return () => {
      clearTimeout(timer);
      sweep.remove();
    };
  }, [focused]);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    const q = value.trim();
    if (!q) return;
    router.push(`/search?q=${encodeURIComponent(q)}`);
  };

  return (
    <form onSubmit={submit} className="group relative w-full">
      <div
        className={`relative overflow-hidden rounded-full border transition-all duration-300 ${
          focused
            ? "border-neon/60 shadow-glow-cyan bg-panel-2"
            : "border-edge bg-panel/70 hover:border-neon/30"
        }`}
      >
        <Search
          className={`pointer-events-none absolute left-5 top-1/2 size-5 -translate-y-1/2 transition-colors ${
            focused ? "text-neon" : "text-ink-faint group-hover:text-ink-dim"
          }`}
        />
        <input
          ref={inputRef}
          type="text"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
          autoFocus={autoFocus}
          placeholder={placeholder}
          aria-label="Search GitHub repositories"
          className={`peer w-full bg-transparent font-mono text-ink outline-none placeholder:text-ink-faint ${
            size === "lg" ? "py-5 pl-14 pr-32 text-base md:text-lg" : "py-3.5 pl-12 pr-14 text-sm"
          }`}
        />
        <motion.button
          whileTap={{ scale: 0.94 }}
          type="submit"
          aria-label="Search"
          className={`absolute right-2 top-1/2 flex -translate-y-1/2 items-center gap-2 rounded-full bg-gradient-to-r from-cyan-500 to-violet-500 px-4 py-2 text-sm font-semibold text-white shadow-lg transition-all hover:from-cyan-400 hover:to-violet-400 hover:shadow-glow-cyan ${
            size === "lg" ? "px-5 py-2.5" : "px-3.5 py-1.5"
          }`}
        >
          <span className="hidden sm:inline">Search</span>
          <ArrowRight className="size-4" />
        </motion.button>
      </div>

      {size === "lg" && (
        <div className="mt-3 flex flex-wrap items-center justify-center gap-2 text-xs text-ink-faint">
          <Sparkles className="size-3.5 text-violet" />
          <span>Try:</span>
          {["torvalds/linux", "rust-lang/rust", "facebook/react", "shadcn/ui"].map((s) => (
            <button
              key={s}
              type="button"
              onClick={() => setValue(s)}
              className="rounded-full border border-edge px-2.5 py-1 font-mono transition-all hover:border-neon/50 hover:text-neon"
            >
              {s}
            </button>
          ))}
        </div>
      )}
    </form>
  );
}
