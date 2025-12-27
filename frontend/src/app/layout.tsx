import type { Metadata } from "next";
import "./globals.css";
import { WalletProvider } from "@/components/WalletProvider";
import { OreStatsProvider } from "@/context/OreStatsContext";

export const metadata: Metadata = {
  title: "ORE Stats - Mining Statistics & Tracker",
  description: "Track ORE mining rounds, deployments, and statistics",
  icons: {
    icon: "/icon.svg",
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
