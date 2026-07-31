import type { Metadata, Viewport } from "next";
import "./globals.css";
import { SmoothScrollProvider } from "@/components/SmoothScroll";
import { Nav } from "@/components/ui/Nav";
import { Footer } from "@/components/ui/Footer";

export const metadata: Metadata = {
  title: {
    default: "ArchiveHub — nothing open-source should disappear forever",
    template: "%s · ArchiveHub",
  },
  description:
    "Search and preservation platform for public GitHub repositories. Browse, download, and archive code before it disappears.",
  keywords: ["github", "archive", "preservation", "open source", "search"],
  openGraph: {
    title: "ArchiveHub",
    description: "Nothing open-source should disappear forever.",
    type: "website",
  },
  icons: {
    icon: "/favicon.svg",
  },
};

export const viewport: Viewport = {
  themeColor: "#06070b",
  width: "device-width",
  initialScale: 1,
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className="dark">
      <body>
        <SmoothScrollProvider>
          <Nav />
          <main className="relative z-10 min-h-screen">{children}</main>
          <Footer />
        </SmoothScrollProvider>
      </body>
    </html>
  );
}
