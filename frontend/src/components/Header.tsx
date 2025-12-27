"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { WalletMultiButton } from "@solana/wallet-adapter-react-ui";

export function Header() {
  const pathname = usePathname();
  
  const isActive = (path: string) => {
    if (path === '/') return pathname === '/';
    return pathname?.startsWith(path);
  };
  
  return (
    <header className="border-b border-slate-800/50 bg-slate-900/50 backdrop-blur-sm sticky top-0 z-10">
      <div className="max-w-7xl mx-auto px-4 py-4 flex items-center justify-between">
        <div className="flex items-center gap-6">
          <Link href="/" className="text-2xl font-bold bg-gradient-to-r from-amber-400 to-orange-500 bg-clip-text text-transparent">
            ORE Stats
          </Link>
          <nav className="flex items-center gap-4">
            <Link 
              href="/" 
              className={`text-sm transition-colors ${
                isActive('/') && !isActive('/miners') && !isActive('/autominers')
                  ? 'text-amber-400 font-medium' 
                  : 'text-slate-400 hover:text-white'
              }`}
            >
              Rounds
            </Link>
            <Link 
              href="/miners" 
              className={`text-sm transition-colors ${
                isActive('/miners') 
                  ? 'text-amber-400 font-medium' 
                  : 'text-slate-400 hover:text-white'
              }`}
            >
              Miners
            </Link>
            <Link 
              href="/autominers" 
              className={`text-sm transition-colors ${
                isActive('/autominers') 
                  ? 'text-amber-400 font-medium' 
                  : 'text-slate-400 hover:text-white'
              }`}
            >
              AutoMiners
            </Link>
            <Link 
              href="/manage" 
              className={`text-sm transition-colors ${
                isActive('/manage') 
                  ? 'text-amber-400 font-medium' 
                  : 'text-slate-400 hover:text-white'
              }`}
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
