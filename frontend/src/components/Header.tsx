"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { WalletMultiButton } from "@solana/wallet-adapter-react-ui";

export function Header() {
  const pathname = usePathname();
  
  return (
    <header className="border-b border-zinc-800 bg-zinc-900/50">
      <div className="max-w-6xl mx-auto px-4 py-4 flex items-center justify-between">
        <div className="flex items-center gap-6">
          <Link href="/" className="flex items-center gap-2">
            <h1 className="text-xl font-bold text-white">Evore</h1>
          </Link>
          <nav className="flex items-center gap-4">
            <Link 
              href="/" 
              className={`text-sm ${pathname === '/' ? 'text-white' : 'text-zinc-400 hover:text-zinc-300'}`}
            >
              AutoMiners
            </Link>
            <Link 
              href="/rounds" 
              className={`text-sm ${pathname === '/rounds' ? 'text-white' : 'text-zinc-400 hover:text-zinc-300'}`}
            >
              Rounds
            </Link>
            <Link 
              href="/manage" 
              className={`text-sm ${pathname === '/manage' ? 'text-white' : 'text-zinc-400 hover:text-zinc-300'}`}
            >
              Advanced
            </Link>
          </nav>
        </div>
        <WalletMultiButton />
      </div>
    </header>
  );
}
