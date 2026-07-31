/** @type {import('next').NextConfig} */
const API_ORIGIN = process.env.API_ORIGIN ?? "http://localhost:8080";

const nextConfig = {
  reactStrictMode: true,
  poweredByHeader: false,
  output: "standalone", // produces .next/standalone for slim Docker images
  experimental: {
    // Allow React Three Fiber's Canvas to use `three` without SSR noise.
    optimizePackageImports: ["lucide-react", "motion"],
  },
  async rewrites() {
    return [
      {
        source: "/api/:path*",
        destination: `${API_ORIGIN}/api/:path*`,
      },
    ];
  },
  images: {
    remotePatterns: [
      { protocol: "https", hostname: "avatars.githubusercontent.com" },
      { protocol: "https", hostname: "github.com" },
      { protocol: "https", hostname: "**.githubusercontent.com" },
    ],
  },
};

export default nextConfig;
