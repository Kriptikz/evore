"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { WalletMultiButton } from "@solana/wallet-adapter-react-ui";
import { useOreStats, formatSol, formatOre } from "@/context/OreStatsContext";
import { MinerBookmarksDropdown } from "./MinerBookmarksDropdown";
import { ChartsBookmarksDropdown } from "./ChartsBookmarksDropdown";

export function Header() {
  const pathname = usePathname();
  const { treasury, board, currentSlot, phase, slotsRemaining, slotsSinceEnd } = useOreStats();
  
  const isActive = (path: string) => {
    if (path === '/') return pathname === '/';
    return pathname?.startsWith(path);
  };

  // Phase indicator styling
  const phaseColors = {
    active: "bg-emerald-500",
    intermission: "bg-blue-500",
    awaiting_reset: "bg-orange-500",
    waiting: "bg-yellow-500",
    finalizing: "bg-purple-500",
  };

  const phaseLabels = {
    active: "Active",
    intermission: "Intermission",
    awaiting_reset: "Resetting",
    waiting: "Starting",
    finalizing: "Finalizing",
  };
  
  return (
    <header className="border-b border-slate-800/50 bg-slate-900/80 backdrop-blur-md sticky top-0 z-50">
      {/* Main Header Row */}
      <div className="max-w-7xl mx-auto px-4 py-3 flex items-center justify-between">
        <div className="flex items-center gap-6">
          <Link href="/" className="text-xl font-bold bg-gradient-to-r from-amber-400 to-orange-500 bg-clip-text text-transparent">
            ORE Stats
          </Link>
          <nav className="flex items-center gap-4">
            <Link 
              href="/" 
              className={`text-sm transition-colors ${
                isActive('/') && !isActive('/miners') && !isActive('/autominers') && !isActive('/leaderboard')
                  ? 'text-amber-400 font-medium' 
                  : 'text-slate-400 hover:text-white'
              }`}
            >
              Rounds
            </Link>
            <Link 
              href="/leaderboard" 
              className={`text-sm transition-colors ${
                isActive('/leaderboard') 
                  ? 'text-amber-400 font-medium' 
                  : 'text-slate-400 hover:text-white'
              }`}
            >
              Leaderboard
            </Link>
            <Link 
              href="/miners" 
              className={`text-sm transition-colors ${
                isActive('/miners') && !isActive('/autominers')
                  ? 'text-amber-400 font-medium' 
                  : 'text-slate-400 hover:text-white'
              }`}
            >
              Miners
            </Link>
            <Link 
              href="/charts" 
              className={`text-sm transition-colors ${
                isActive('/charts')
                  ? 'text-amber-400 font-medium' 
                  : 'text-slate-400 hover:text-white'
              }`}
            >
              Charts
            </Link>
            {/* AutoMiners link hidden - feature not ready for public yet */}
            {/* <Link 
              href="/autominers" 
              className={`text-sm transition-colors ${
                isActive('/autominers') || isActive('/manage')
                  ? 'text-amber-400 font-medium' 
                  : 'text-slate-400 hover:text-white'
              }`}
            >
              AutoMiners
            </Link> */}
          </nav>
        </div>
        <div className="flex items-center gap-2">
          <MinerBookmarksDropdown />
          <ChartsBookmarksDropdown />
          <WalletMultiButton />
        </div>
      </div>

      {/* Stats Bar */}
      <div className="border-t border-slate-800/30 bg-slate-950/50">
        <div className="max-w-7xl mx-auto px-4 py-2 flex items-center justify-between text-xs">
          {/* Left: Round Status */}
          <div className="flex items-center gap-4">
            {/* Phase Indicator */}
            {phase && (
              <div className="flex items-center gap-2">
                <span className={`w-2 h-2 rounded-full ${phaseColors[phase]} ${phase !== "active" ? "animate-pulse" : ""}`} />
                <span className="text-slate-400">
                  Round <span className="text-white font-mono">#{board?.round_id}</span>
                </span>
                <span className={`px-1.5 py-0.5 rounded text-[10px] font-medium ${
                  phase === "active" ? "bg-emerald-500/20 text-emerald-400" :
                  phase === "intermission" ? "bg-blue-500/20 text-blue-400" :
                  phase === "awaiting_reset" ? "bg-orange-500/20 text-orange-400" :
                  phase === "waiting" ? "bg-yellow-500/20 text-yellow-400" :
                  "bg-purple-500/20 text-purple-400"
                }`}>
                  {phaseLabels[phase]}
                </span>
              </div>
            )}

            {/* Slots Info */}
            <div className="text-slate-500">
              {phase === "active" && slotsRemaining > 0 && (
                <span>
                  <span className="text-amber-400 font-mono">{slotsRemaining}</span> slots left
                </span>
              )}
              {phase === "intermission" && (
                <span>
                  <span className="text-blue-400 font-mono">~{35 - slotsSinceEnd}</span> slots to reset
                </span>
              )}
              {phase === "awaiting_reset" && (
                <span className="text-orange-400">Waiting for reset tx...</span>
              )}
              {phase === "waiting" && (
                <span className="text-yellow-400">Waiting for first deploy...</span>
              )}
            </div>

            {/* Current Slot */}
            <div className="text-slate-500 border-l border-slate-700 pl-4">
              Slot <span className="text-slate-300 font-mono">{currentSlot.toLocaleString()}</span>
            </div>
          </div>

          {/* Right: Treasury Stats */}
          {treasury && (
            <div className="flex items-center gap-4 text-slate-500">
              <div className="flex items-center gap-1.5">
                <span className="text-amber-400">💰</span>
                <span>HaWG:</span>
                <span className="text-amber-400 font-mono">{formatSol(treasury.balance)}</span>
              </div>
              
              {treasury.motherlode > 0 && (
                <div className="flex items-center gap-1.5 px-2 py-0.5 bg-purple-500/10 border border-purple-500/30 rounded">
                  <span>💎</span>
                  <span className="text-purple-400 font-medium">Motherlode:</span>
                  <span className="text-purple-300 font-mono">{formatOre(treasury.motherlode)} ORE</span>
                </div>
              )}
              
              <div className="hidden md:flex items-center gap-1.5">
                <span>Unclaimed:</span>
                <span className="text-slate-300 font-mono">{formatOre(treasury.total_unclaimed)}</span>
              </div>
              
              <div className="hidden lg:flex items-center gap-1.5">
                <span>Staked:</span>
                <span className="text-slate-300 font-mono">{formatOre(treasury.total_staked)}</span>
              </div>
              
              <div className="hidden xl:flex items-center gap-1.5">
                <span>Refined:</span>
                <span className="text-slate-300 font-mono">{formatOre(treasury.total_refined)}</span>
              </div>
            </div>
          )}
        </div>
      </div>
    </header>
  );
}
