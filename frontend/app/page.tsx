import { BubbleField } from "@/components/bubble/BubbleField";
import { ParticleField } from "@/components/background/ParticleField";
import { SearchBar } from "@/components/ui/SearchBar";
import { StatsBar } from "@/components/home/StatsBar";
import { HowItWorks } from "@/components/home/HowItWorks";
import { PixelDeco } from "@/components/ui/PixelDeco";

export default function HomePage() {
  return (
    <div className="relative">
      {/* ---- Hero ------------------------------------------------------- */}
      <section className="relative flex min-h-[100svh] flex-col items-center justify-center overflow-hidden px-6 pb-24 pt-24">
        {/* Interactive background: particles + the signature bubble field */}
        <div className="pointer-events-none absolute inset-0">
          <ParticleField />
          {/* The bubble field is interactive — re-enable pointer events on it */}
          <div className="pointer-events-auto absolute inset-0">
            <BubbleField />
          </div>
        </div>

        {/* Legibility scrim behind the text, so bubbles read as "through glass". */}
        <div className="pointer-events-none absolute inset-x-0 top-0 h-72 bg-gradient-to-b from-canvas/80 to-transparent" />
        <div className="pointer-events-none absolute inset-x-0 bottom-0 h-64 bg-gradient-to-t from-canvas/90 to-transparent" />

        <div className="relative z-20 mx-auto w-full max-w-3xl text-center">
          <div className="glass inline-flex items-center gap-2 rounded-full px-4 py-1.5 text-xs text-ink-dim">
            <span className="size-1.5 rounded-full bg-lime animate-pulse-glow" />
            Preserving open source since the first push
          </div>

          <h1 className="mt-6 text-balance text-4xl font-black tracking-tight md:text-7xl">
            Nothing open-source
            <br />
            should <span className="text-gradient">disappear</span> forever.
          </h1>

          <p className="mx-auto mt-6 max-w-xl text-pretty text-base text-ink-dim md:text-lg">
            Salsyx searches GitHub in real time, and if a repository is gone, we bring it
            back from the archive. Browse, download, and preserve — forever.
          </p>

          <div className="mt-10">
            <SearchBar size="lg" />
          </div>
        </div>

        {/* Pixel-art decorations — cyberpunk accent, tiny and non-intrusive */}
        <PixelDeco className="absolute left-[8%] top-[22%] hidden opacity-40 lg:block" />
        <PixelDeco variant="pink" className="absolute right-[10%] bottom-[26%] hidden opacity-40 lg:block" />

        <div className="absolute bottom-8 z-20 flex flex-col items-center gap-2 text-ink-faint">
          <span className="text-[10px] uppercase tracking-[0.3em]">scroll to explore</span>
          <div className="size-5 animate-bounce rotate-45 border-b border-r border-ink-faint" />
        </div>
      </section>

      {/* ---- Live stats strip ------------------------------------------- */}
      <StatsBar />

      {/* ---- How it works ----------------------------------------------- */}
      <HowItWorks />
    </div>
  );
}
