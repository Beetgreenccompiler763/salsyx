"use client";

// Tiny pixel-art decorations — cyberpunk accents, drawn as literal pixels so
// they keep that retro-crisp feel. Non-interactive and GPU-cheap.

import { motion } from "motion/react";

const GRID = {
  cyan: [
    "......██",
    "....████",
    "..██████",
    "████████",
  ],
  pink: [
    "██......",
    "████....",
    "██████..",
    "████████",
  ],
};

export function PixelDeco({
  variant = "cyan",
  className = "",
}: {
  variant?: keyof typeof GRID;
  className?: string;
}) {
  const rows = GRID[variant];
  return (
    <motion.div
      aria-hidden
      initial={{ opacity: 0, scale: 0.6 }}
      whileInView={{ opacity: 1, scale: 1 }}
      viewport={{ once: true }}
      transition={{ type: "spring", stiffness: 200, damping: 16 }}
      className={`pointer-events-none select-none ${className}`}
    >
      <div className="pixel flex flex-col gap-[3px]">
        {rows.map((row, i) => (
          <div key={i} className="flex gap-[3px]">
            {row.split("").map((cell, j) =>
              cell === "█" ? (
                <span
                  key={j}
                  className={`size-[6px] ${
                    variant === "cyan" ? "bg-neon/60" : "bg-pink/60"
                  } shadow-glow-cyan`}
                />
              ) : (
                <span key={j} className="size-[6px] bg-transparent" />
              ),
            )}
          </div>
        ))}
      </div>
    </motion.div>
  );
}
