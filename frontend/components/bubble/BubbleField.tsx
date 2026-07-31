"use client";

// Salsyx's signature visual: a field of hundreds of floating GitHub
// user bubbles. Canvas-2D based for performance (hundreds of particles at
// 60fps without touching the DOM), with soft-body collision avoidance,
// cursor repulsion, hover glow, and a "pop" interaction that opens a
// futuristic profile modal.

import { useCallback, useEffect, useRef, useState } from "react";
import type { BubbleProfile } from "@/lib/types";
import { BubbleModal } from "./BubbleModal";

interface Bubble {
  x: number;
  y: number;
  vx: number;
  vy: number;
  r: number;
  profile: BubbleProfile;
  phase: number;
  hue: number;
  glow: number;
  // Interaction state
  hovered: boolean;
  popping: boolean;
  popT: number;
  popParticles: { x: number; y: number; vx: number; vy: number; life: number }[];
}

// Curated seed of well-known GitHub accounts. In production this is replaced
// by a server-fed feed from the Salsyx database.
const SEED: BubbleProfile[] = [
  { login: "torvalds", name: "Linus Torvalds", repos: 5, stars: 250000, languages: ["C", "Rust"] },
  { login: "gaearon", name: "Dan Abramov", repos: 32, stars: 120000, languages: ["JS", "TS"] },
  { login: "yyx990803", name: "Evan You", repos: 150, stars: 320000, languages: ["JS", "TS"] },
  { login: "sindresorhus", name: "Sindre Sorhus", repos: 1100, stars: 580000, languages: ["JS"] },
  { login: "tj", name: "TJ Holowaychuk", repos: 300, stars: 400000, languages: ["JS", "Go"] },
  { login: "antirez", name: "Salvatore Sanfilippo", repos: 60, stars: 140000, languages: ["C"] },
  { login: "matz", name: "Yukihiro Matsumoto", repos: 40, stars: 90000, languages: ["Ruby"] },
  { login: "kelseyhightower", name: "Kelsey Hightower", repos: 70, stars: 100000, languages: ["Go"] },
  { login: "jashkenas", name: "Jeremy Ashkenas", repos: 40, stars: 130000, languages: ["JS"] },
  { login: "addyosmani", name: "Addy Osmani", repos: 220, stars: 260000, languages: ["JS"] },
  { login: "paulirish", name: "Paul Irish", repos: 80, stars: 90000, languages: ["JS"] },
  { login: "fogleman", name: "Michael Fogleman", repos: 170, stars: 150000, languages: ["Go"] },
  { login: "fat", name: "fat", repos: 100, stars: 80000, languages: ["JS"] },
  { login: "dhh", name: "David Heinemeier Hansson", repos: 90, stars: 160000, languages: ["Ruby"] },
  { login: "defunkt", name: "Chris Wanstrath", repos: 30, stars: 60000, languages: ["Ruby"] },
  { login: "mojombo", name: "Tom Preston-Werner", repos: 50, stars: 70000, languages: ["Ruby"] },
  { login: "matthewhanifan", name: "Matt Hanifan", repos: 20, stars: 5000, languages: ["JS"] },
  { login: "bmizerany", name: "Blake Mizerany", repos: 60, stars: 40000, languages: ["Go"] },
  { login: "jaredpalmer", name: "Jared Palmer", repos: 60, stars: 80000, languages: ["TS"] },
  { login: "shuding", name: "Shu Ding", repos: 40, stars: 90000, languages: ["TS"] },
  { login: "pacocoursey", name: "Paco Coursey", repos: 40, stars: 50000, languages: ["TS"] },
  { login: "rauchg", name: "Guillermo Rauch", repos: 90, stars: 150000, languages: ["JS"] },
  { login: "sophieschmieg", name: "Sophie Schmieg", repos: 10, stars: 3000, languages: ["Rust"] },
  { login: "Dianasaur", name: "Dianasaur", repos: 30, stars: 20000, languages: ["TS"] },
  { login: "mirshko", name: "Mirshko", repos: 20, stars: 8000, languages: ["TS"] },
  { login: "drwho", name: "Dr Who", repos: 15, stars: 9000, languages: ["Python"] },
  { login: "benhoyt", name: "Ben Hoyt", repos: 30, stars: 20000, languages: ["Go"] },
  { login: "aneesha", name: "Aneesha", repos: 20, stars: 15000, languages: ["Python"] },
  { login: "fatiherikli", name: "Fatih Erikli", repos: 30, stars: 25000, languages: ["Python"] },
  { login: "anishathalye", name: "Anish Athalye", repos: 25, stars: 30000, languages: ["Python"] },
  { login: "nicklacy", name: "Nick Lacy", repos: 15, stars: 5000, languages: ["Python"] },
  { login: "vdaubry", name: "Vincent Daubry", repos: 20, stars: 8000, languages: ["Go"] },
  { login: "mfornos", name: "Marc Fornos", repos: 10, stars: 5000, languages: ["Go"] },
  { login: "alesandroortiz", name: "Alesandro Ortiz", repos: 15, stars: 2000, languages: ["JS"] },
  { login: "aapo", name: "Aapo", repos: 20, stars: 9000, languages: ["JS"] },
  { login: "aleksa", name: "Aleksa", repos: 25, stars: 30000, languages: ["JS"] },
  { login: "daniel", name: "Daniel", repos: 30, stars: 40000, languages: ["JS"] },
  { login: "vitaly", name: "Vitaly", repos: 10, stars: 5000, languages: ["CSS"] },
  { login: "dieter", name: "Dieter", repos: 20, stars: 30000, languages: ["Python"] },
  { login: "martin", name: "Martin", repos: 15, stars: 40000, languages: ["PHP"] },
  { login: "codex", name: "Codex", repos: 20, stars: 20000, languages: ["Rust"] },
  { login: "valery", name: "Valery", repos: 30, stars: 50000, languages: ["Java"] },
  { login: "roman", name: "Roman", repos: 20, stars: 30000, languages: ["Ruby"] },
  { login: "oleg", name: "Oleg", repos: 25, stars: 40000, languages: ["Python"] },
  { login: "frank", name: "Frank", repos: 15, stars: 5000, languages: ["JS"] },
  { login: "john", name: "John", repos: 20, stars: 30000, languages: ["TS"] },
  { login: "igor", name: "Igor", repos: 15, stars: 8000, languages: ["C++"] },
  { login: "mike", name: "Mike", repos: 20, stars: 40000, languages: ["Go"] },
  { login: "peter", name: "Peter", repos: 30, stars: 20000, languages: ["Rust"] },
  { login: "serge", name: "Serge", repos: 15, stars: 9000, languages: ["Java"] },
];

const AVATAR = (login: string) => `https://avatars.githubusercontent.com/${login}?v=4&s=128`;

function avatarKey(login: string): string {
  return login.toLowerCase();
}

export function BubbleField({ count = 120 }: { count?: number }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const bubblesRef = useRef<Bubble[]>([]);
  const cursorRef = useRef({ x: -9999, y: -9999, active: false });
  const [selected, setSelected] = useState<BubbleProfile | null>(null);

  // Build the bubble set once (deterministic so SSR-hydration stays stable).
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const resize = () => {
      canvas.width = window.innerWidth;
      canvas.height = window.innerHeight;
    };
    resize();

    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const makeBubbles = () => {
      const w = canvas.width;
      const h = canvas.height;
      const list: Bubble[] = [];
      const radiusByIndex: number[] = [];
      for (let i = 0; i < count; i++) {
        const seed = SEED[i % SEED.length];
        const r = 14 + Math.random() * 22;
        // Rejection-sample positions so bubbles don't overlap at spawn.
        let x = 0;
        let y = 0;
        let attempts = 0;
        do {
          x = r + Math.random() * (w - 2 * r);
          y = r + Math.random() * (h - 2 * r);
          attempts++;
        } while (
          attempts < 40 &&
          radiusByIndex.some(
            (rr, j) => j < radiusByIndex.length && Math.hypot(list[j]?.x - x || 1e6, list[j]?.y - y || 1e6) < rr + r + 6,
          )
        );
        radiusByIndex.push(r);
        list.push({
          x,
          y,
          vx: (Math.random() - 0.5) * 0.4,
          vy: (Math.random() - 0.5) * 0.4,
          r,
          profile: { ...seed, avatar: AVATAR(seed.login) },
          phase: Math.random() * Math.PI * 2,
          hue: 180 + Math.random() * 120,
          glow: 0,
          hovered: false,
          popping: false,
          popT: 0,
          popParticles: [],
        });
      }
      bubblesRef.current = list;
    };
    makeBubbles();

    // Pre-render circle-clipped avatars into offscreen canvases.
    const avatarCache = new Map<string, HTMLCanvasElement>();
    const loadAvatar = (bubble: Bubble) => {
      const key = avatarKey(bubble.profile.login);
      const cached = avatarCache.get(key);
      if (cached) {
        bubble.profile.avatar = cached.toDataURL();
        return;
      }
      const img = new Image();
      img.crossOrigin = "anonymous";
      img.onload = () => {
        const size = 128;
        const off = document.createElement("canvas");
        off.width = off.height = size;
        const octx = off.getContext("2d")!;
        octx.beginPath();
        octx.arc(size / 2, size / 2, size / 2, 0, Math.PI * 2);
        octx.clip();
        octx.drawImage(img, 0, 0, size, size);
        avatarCache.set(key, off);
        bubble.profile.avatar = off.toDataURL();
      };
      img.src = AVATAR(bubble.profile.login);
    };
    bubblesRef.current.forEach(loadAvatar);

    const WINDOW_START = Date.now();

    const frame = () => {
      const ctx = canvasRef.current?.getContext("2d");
      if (!ctx) return;
      const w = canvas.width;
      const h = canvas.height;
      const t = (Date.now() - WINDOW_START) / 1000;
      const cursor = cursorRef.current;

      ctx.clearRect(0, 0, w, h);

      const list = bubblesRef.current;
      for (let i = 0; i < list.length; i++) {
        const b = list[i];

        // Gentle autonomous drift.
        b.phase += 0.005;
        b.x += b.vx + Math.sin(b.phase + i) * 0.06;
        b.y += b.vy + Math.cos(b.phase * 0.8 + i) * 0.06;

        // Cursor repulsion when active (or gentle attraction when idle).
        const dx = b.x - cursor.x;
        const dy = b.y - cursor.y;
        const dist = Math.hypot(dx, dy);
        const reach = 160;
        if (cursor.active && dist < reach && dist > 0.01) {
          const force = ((reach - dist) / reach) * 1.4;
          b.vx += (dx / dist) * force;
          b.vy += (dy / dist) * force;
        } else if (dist < reach * 1.6 && dist > 0.01) {
          const force = ((reach * 1.6 - dist) / (reach * 1.6)) * 0.06;
          b.vx += (-dx / dist) * force;
          b.vy += (-dy / dist) * force;
        }

        // Soft collision avoidance between bubbles.
        for (let j = i + 1; j < list.length; j++) {
          const o = list[j];
          const ddx = o.x - b.x;
          const ddy = o.y - b.y;
          const d = Math.hypot(ddx, ddy);
          const min = b.r + o.r + 6;
          if (d < min && d > 0.01) {
            const push = (min - d) / d;
            const p = push * 0.02;
            b.vx -= ddx * p;
            b.vy -= ddy * p;
            o.vx += ddx * p;
            o.vy += ddy * p;
          }
        }

        // Damping + boundary bounce.
        b.vx *= 0.985;
        b.vy *= 0.985;
        if (b.x < b.r) { b.x = b.r; b.vx = Math.abs(b.vx); }
        if (b.x > w - b.r) { b.x = w - b.r; b.vx = -Math.abs(b.vx); }
        if (b.y < b.r) { b.y = b.r; b.vy = Math.abs(b.vy); }
        if (b.y > h - b.r) { b.y = h - b.r; b.vy = -Math.abs(b.vy); }

        // Hover detection (nearest bubble within pointer).
        b.hovered = cursor.active && dist < b.r + 14;

        // Pop animation state.
        if (b.popping) {
          b.popT += 0.016;
          if (b.popT >= 0.6) {
            b.popping = false;
            b.popT = 0;
          }
        }

        // Draw.
        const bob = Math.sin(t * 1.4 + b.phase) * 3;
        const scale = b.hovered ? 1.25 : b.popping ? 1 + 0.2 * Math.sin(b.popT * 30) : 1;
        const r = b.r * scale;

        ctx.save();
        ctx.translate(b.x, b.y + bob);

        // Glow ring.
        if (b.hovered || b.popping) {
          ctx.shadowColor = `hsla(${b.hue}, 90%, 65%, 0.9)`;
          ctx.shadowBlur = 24;
        }

        // Bubble body.
        ctx.beginPath();
        ctx.arc(0, 0, r, 0, Math.PI * 2);
        ctx.fillStyle = `hsla(${b.hue}, 45%, 14%, ${b.popping ? 0.5 : 0.75})`;
        ctx.fill();
        ctx.lineWidth = 1.5;
        ctx.strokeStyle = b.hovered
          ? `hsla(${b.hue}, 90%, 70%, 0.95)`
          : `hsla(${b.hue}, 60%, 55%, 0.35)`;
        ctx.stroke();

        // Avatar inside (cached data URL once loaded).
        ctx.save();
        ctx.beginPath();
        ctx.arc(0, 0, r - 3, 0, Math.PI * 2);
        ctx.clip();
        const avatar = b.profile.avatar;
        if (avatar && avatar.startsWith("data:")) {
          ctx.drawImage(imgFromData(avatar), -r, -r, r * 2, r * 2);
        }
        ctx.restore();

        ctx.restore();
      }

      requestAnimationFrame(frame);
    };

    const imgCache = new Map<string, HTMLImageElement>();
    function imgFromData(data: string): HTMLImageElement {
      const hit = imgCache.get(data);
      if (hit) return hit;
      const img = new Image();
      img.src = data;
      imgCache.set(data, img);
      return img;
    }

    // Pointer tracking.
    const onMove = (e: PointerEvent) => {
      const rect = canvas.getBoundingClientRect();
      cursorRef.current = { x: e.clientX - rect.left, y: e.clientY - rect.top, active: true };
    };
    const onLeave = () => {
      cursorRef.current = { x: -9999, y: -9999, active: false };
      bubblesRef.current.forEach((b) => (b.hovered = false));
    };
    const onClick = (e: MouseEvent) => {
      const rect = canvas.getBoundingClientRect();
      const mx = e.clientX - rect.left;
      const my = e.clientY - rect.top;
      let best: Bubble | null = null;
      let bestDist = Infinity;
      for (const b of bubblesRef.current) {
        const d = Math.hypot(b.x - mx, b.y - my);
        if (d < b.r * 1.5 && d < bestDist) {
          best = b;
          bestDist = d;
        }
      }
      if (best) {
        best.popping = true;
        best.popT = 0;
        setSelected(best.profile);
      }
    };

    canvas.addEventListener("pointermove", onMove);
    canvas.addEventListener("pointerleave", onLeave);
    canvas.addEventListener("click", onClick);
    window.addEventListener("resize", resize);

    const raf = requestAnimationFrame(frame);
    return () => {
      cancelAnimationFrame(raf);
      canvas.removeEventListener("pointermove", onMove);
      canvas.removeEventListener("pointerleave", onLeave);
      canvas.removeEventListener("click", onClick);
      window.removeEventListener("resize", resize);
    };
  }, [count]);

  const close = useCallback(() => setSelected(null), []);

  return (
    <>
      <canvas
        ref={canvasRef}
        className="pointer-events-auto absolute inset-0 h-full w-full"
        aria-label="Floating GitHub user bubbles. Click a bubble to explore an account."
      />
      <BubbleModal profile={selected} onClose={close} />
    </>
  );
}
