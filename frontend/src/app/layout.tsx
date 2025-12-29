import type { Metadata } from "next";
import "./globals.css";
import { WalletProvider } from "@/components/WalletProvider";
import { OreStatsProvider } from "@/context/OreStatsContext";

const siteUrl = process.env.NEXT_PUBLIC_SITE_URL || "https://ore-stats.com";

export const metadata: Metadata = {
  title: {
    default: "ORE Stats - Mining Statistics & Tracker",
    template: "%s | ORE Stats",
  },
  description: "Track ORE mining rounds, deployments, miner leaderboards, and real-time statistics on the Solana blockchain.",
  keywords: ["ORE", "mining", "Solana", "crypto", "statistics", "blockchain", "tracker", "leaderboard"],
  authors: [{ name: "ORE Stats Team" }],
  creator: "ORE Stats",
  publisher: "ORE Stats",
  metadataBase: new URL(siteUrl),
  alternates: {
    canonical: "/",
  },
  openGraph: {
    type: "website",
    locale: "en_US",
    url: siteUrl,
    siteName: "ORE Stats",
    title: "ORE Stats - Mining Statistics & Tracker",
    description: "Track ORE mining rounds, deployments, miner leaderboards, and real-time statistics on the Solana blockchain.",
  },
  twitter: {
    card: "summary_large_image",
    title: "ORE Stats - Mining Statistics & Tracker",
    description: "Track ORE mining rounds, deployments, miner leaderboards, and real-time statistics on the Solana blockchain.",
  },
  robots: {
    index: true,
    follow: true,
    googleBot: {
      index: true,
      follow: true,
      "max-video-preview": -1,
      "max-image-preview": "large",
      "max-snippet": -1,
    },
  },
  icons: {
    icon: "/icon.svg",
    apple: "/icon.svg",
  },
  verification: {
    // Add Google Search Console verification when available
    // google: "your-google-verification-code",
  },
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body className="antialiased">
        <WalletProvider>
          <OreStatsProvider>
            {children}
          </OreStatsProvider>
        </WalletProvider>
      </body>
    </html>
  );
}
