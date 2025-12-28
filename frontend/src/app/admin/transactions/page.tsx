"use client";

import { useEffect, useState, useCallback } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { useAdmin } from "@/context/AdminContext";
import { 
  api, 
  FullAnalysisResponse, 
  FullTransactionAnalysis, 
  InstructionAnalysis,
  ParsedInstruction,
  RawTransaction 
} from "@/lib/api";

// ============================================================================
// Admin Shell
// ============================================================================

function AdminShell({ children }: { children: React.ReactNode }) {
  return (
    <div className="min-h-screen bg-[#0a0a0f] text-gray-100 flex">
      <aside className="w-56 border-r border-gray-800/50 p-4 space-y-2 shrink-0">
        <h2 className="text-sm font-semibold text-gray-400 mb-4 uppercase tracking-wider">Admin</h2>
        <nav className="space-y-1">
          {[
            { href: "/admin", label: "Dashboard" },
            { href: "/admin/server", label: "Server Metrics" },
            { href: "/admin/rpc", label: "RPC Metrics" },
            { href: "/admin/websocket", label: "WebSocket" },
            { href: "/admin/requests", label: "Request Logs" },
            { href: "/admin/database", label: "Database" },
            { href: "/admin/backfill", label: "Backfill" },
            { href: "/admin/transactions", label: "Transactions" },
            { href: "/admin/blacklist", label: "Blacklist" },
          ].map(({ href, label }) => (
            <Link
              key={href}
              href={href}
              className="block px-3 py-2 rounded-lg text-sm text-gray-300 hover:bg-gray-800/50 hover:text-white transition-colors"
            >
              {label}
            </Link>
          ))}
        </nav>
      </aside>
      <main className="flex-1 p-6 overflow-auto">{children}</main>
    </div>
  );
}

// ============================================================================
// Utility Components
// ============================================================================

function StatusBadge({ success }: { success: boolean }) {
  return success ? (
    <span className="px-2 py-0.5 text-xs font-medium rounded-full bg-emerald-500/20 text-emerald-400 border border-emerald-500/30">
      Success
    </span>
  ) : (
    <span className="px-2 py-0.5 text-xs font-medium rounded-full bg-red-500/20 text-red-400 border border-red-500/30">
      Failed
    </span>
  );
}

function ProgramBadge({ name }: { name: string }) {
  const colors: Record<string, string> = {
    "System Program": "bg-blue-500/20 text-blue-400 border-blue-500/30",
    "Compute Budget": "bg-purple-500/20 text-purple-400 border-purple-500/30",
    "ORE Program": "bg-amber-500/20 text-amber-400 border-amber-500/30",
    "Token Program": "bg-cyan-500/20 text-cyan-400 border-cyan-500/30",
    "Associated Token": "bg-teal-500/20 text-teal-400 border-teal-500/30",
    "EVORE Program": "bg-emerald-500/20 text-emerald-400 border-emerald-500/30",
    "Memo": "bg-pink-500/20 text-pink-400 border-pink-500/30",
  };
  
  const colorClass = colors[name] || "bg-gray-500/20 text-gray-400 border-gray-500/30";
  
  return (
    <span className={`px-2 py-0.5 text-xs font-medium rounded-full border ${colorClass}`}>
      {name}
    </span>
  );
}

function SolAmount({ lamports, showLabel = true }: { lamports: number; showLabel?: boolean }) {
  const sol = lamports / 1e9;
  const isNegative = sol < 0;
  const color = isNegative ? "text-red-400" : sol > 0 ? "text-emerald-400" : "text-gray-400";
  
  return (
    <span className={color}>
      {isNegative ? "" : "+"}{sol.toFixed(9)} {showLabel && "SOL"}
    </span>
  );
}

function Pubkey({ address, short = false }: { address: string; short?: boolean }) {
  const display = short ? `${address.slice(0, 4)}...${address.slice(-4)}` : address;
  
  return (
    <a
      href={`https://solscan.io/account/${address}`}
      target="_blank"
      rel="noopener noreferrer"
      className="font-mono text-xs text-blue-400 hover:text-blue-300"
      title={address}
    >
      {display}
    </a>
  );
}

function SquaresGrid({ squares }: { squares: number[] }) {
  const grid = Array(25).fill(false);
  squares.forEach(s => grid[s] = true);
  
  return (
    <div className="grid grid-cols-5 gap-0.5 w-20">
      {grid.map((active, i) => (
        <div
          key={i}
          className={`w-3 h-3 rounded-sm ${active ? "bg-emerald-500" : "bg-gray-700"}`}
          title={`Square ${i + 1}`}
        />
      ))}
    </div>
  );
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  
  const handleCopy = () => {
    navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };
  
  return (
    <button
      onClick={handleCopy}
      className="text-gray-500 hover:text-gray-300 text-xs"
      title="Copy"
    >
      {copied ? "✓" : "📋"}
    </button>
  );
}

// ============================================================================
// Parsed Instruction Display
// ============================================================================

function ParsedInstructionDisplay({ parsed }: { parsed: ParsedInstruction }) {
  switch (parsed.type) {
    case "SystemTransfer":
      return (
        <div className="space-y-1 text-sm">
          <div className="flex gap-2">
            <span className="text-gray-500 w-16">From:</span>
            <Pubkey address={parsed.from} short />
          </div>
          <div className="flex gap-2">
            <span className="text-gray-500 w-16">To:</span>
            <Pubkey address={parsed.to} short />
          </div>
          <div className="flex gap-2">
            <span className="text-gray-500 w-16">Amount:</span>
            <span className="text-emerald-400">{parsed.sol.toFixed(9)} SOL</span>
          </div>
        </div>
      );
    
    case "SystemCreateAccount":
      return (
        <div className="space-y-1 text-sm">
          <div className="flex gap-2">
            <span className="text-gray-500 w-20">Funder:</span>
            <Pubkey address={parsed.from} short />
          </div>
          <div className="flex gap-2">
            <span className="text-gray-500 w-20">New:</span>
            <Pubkey address={parsed.new_account} short />
          </div>
          <div className="flex gap-2">
            <span className="text-gray-500 w-20">Lamports:</span>
            <span>{parsed.lamports.toLocaleString()}</span>
          </div>
          <div className="flex gap-2">
            <span className="text-gray-500 w-20">Space:</span>
            <span>{parsed.space} bytes</span>
          </div>
          <div className="flex gap-2">
            <span className="text-gray-500 w-20">Owner:</span>
            <Pubkey address={parsed.owner} short />
          </div>
        </div>
      );
    
    case "ComputeSetLimit":
      return (
        <div className="text-sm">
          <span className="text-gray-500">Units:</span>{" "}
          <span className="text-purple-400">{parsed.units.toLocaleString()}</span>
        </div>
      );
    
    case "ComputeSetPrice":
      return (
        <div className="text-sm">
          <span className="text-gray-500">Price:</span>{" "}
          <span className="text-purple-400">{parsed.micro_lamports.toLocaleString()} µL</span>
        </div>
      );
    
    case "OreDeploy":
      return (
        <div className="space-y-2 text-sm">
          <div className="grid grid-cols-2 gap-2">
            <div>
              <span className="text-gray-500">Signer:</span>
              <div><Pubkey address={parsed.signer} short /></div>
            </div>
            <div>
              <span className="text-gray-500">Miner:</span>
              <div><Pubkey address={parsed.miner} short /></div>
            </div>
          </div>
          <div className="flex gap-4 items-center">
            <div>
              <span className="text-gray-500">Per Square:</span>{" "}
              <span className="text-amber-400">{parsed.amount_sol.toFixed(6)} SOL</span>
            </div>
            <div>
              <span className="text-gray-500">Total:</span>{" "}
              <span className="text-amber-400 font-semibold">{parsed.total_sol.toFixed(6)} SOL</span>
            </div>
          </div>
          <div className="flex gap-4 items-center">
            <div>
              <span className="text-gray-500">Squares ({parsed.squares.length}):</span>
            </div>
            <SquaresGrid squares={parsed.squares} />
          </div>
        </div>
      );
    
    case "OreReset":
      return (
        <div className="text-sm">
          <span className="text-gray-500">Signer:</span>{" "}
          <Pubkey address={parsed.signer} short />
        </div>
      );
    
    case "OreLog":
      return (
        <div className="text-sm">
          <span className="text-gray-500">Event:</span>{" "}
          <span className="text-blue-400">{parsed.event_type}</span>
        </div>
      );
    
    case "TokenTransfer":
      return (
        <div className="space-y-1 text-sm">
          <div className="flex gap-2">
            <span className="text-gray-500 w-16">Source:</span>
            <Pubkey address={parsed.source} short />
          </div>
          <div className="flex gap-2">
            <span className="text-gray-500 w-16">Dest:</span>
            <Pubkey address={parsed.destination} short />
          </div>
          <div className="flex gap-2">
            <span className="text-gray-500 w-16">Amount:</span>
            <span>{parsed.amount.toLocaleString()}</span>
          </div>
        </div>
      );
    
    case "Memo":
      return (
        <div className="text-sm">
          <span className="text-gray-500">Message:</span>{" "}
          <span className="text-pink-400 font-mono">&quot;{parsed.message}&quot;</span>
        </div>
      );
    
    case "Unknown":
      return (
        <div className="text-sm text-gray-500">
          <span className="font-mono text-xs">{parsed.data_preview}</span>
        </div>
      );
    
    default:
      return (
        <div className="text-sm text-gray-400">
          {JSON.stringify(parsed, null, 2)}
        </div>
      );
  }
}

// ============================================================================
// Instruction Card
// ============================================================================

function InstructionCard({ 
  ix, 
  isInner = false,
  parentIndex 
}: { 
  ix: InstructionAnalysis; 
  isInner?: boolean;
  parentIndex?: number;
}) {
  const [expanded, setExpanded] = useState(false);
  
  return (
    <div className={`border rounded-lg ${isInner ? "border-gray-700/50 bg-gray-900/30" : "border-gray-800/50 bg-gray-800/20"}`}>
      <div 
        className="p-3 flex items-center justify-between cursor-pointer hover:bg-gray-800/30"
        onClick={() => setExpanded(!expanded)}
      >
        <div className="flex items-center gap-3">
          <span className="text-xs text-gray-500">
            {isInner ? `${parentIndex}.${ix.index}` : `#${ix.index}`}
          </span>
          <ProgramBadge name={ix.program_name} />
          <span className="text-sm font-medium">{ix.instruction_type}</span>
          {ix.parse_error && (
            <span className="text-xs text-red-400">⚠ Parse error</span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <span className="text-xs text-gray-500">{ix.data_length} bytes</span>
          <span className="text-gray-400">{expanded ? "▼" : "▶"}</span>
        </div>
      </div>
      
      {expanded && (
        <div className="p-4 border-t border-gray-800/50 space-y-4">
          {/* Parsed data */}
          {ix.parsed && (
            <div className="p-3 bg-gray-900/50 rounded-lg">
              <h4 className="text-xs text-gray-500 uppercase mb-2">Parsed Data</h4>
              <ParsedInstructionDisplay parsed={ix.parsed} />
            </div>
          )}
          
          {ix.parse_error && (
            <div className="p-3 bg-red-500/10 rounded-lg border border-red-500/30">
              <h4 className="text-xs text-red-400 uppercase mb-1">Parse Error</h4>
              <p className="text-sm text-red-300">{ix.parse_error}</p>
            </div>
          )}
          
          {/* Accounts */}
          <div>
            <h4 className="text-xs text-gray-500 uppercase mb-2">Accounts ({ix.accounts.length})</h4>
            <div className="space-y-1">
              {ix.accounts.map((acc, i) => (
                <div key={i} className="flex items-center gap-2 text-xs">
                  <span className="text-gray-600 w-6">#{i}</span>
                  <Pubkey address={acc.pubkey} short />
                  {acc.role && <span className="text-gray-500">({acc.role})</span>}
                </div>
              ))}
            </div>
          </div>
          
          {/* Raw data */}
          <div>
            <h4 className="text-xs text-gray-500 uppercase mb-2 flex items-center gap-2">
              Raw Data <CopyButton text={ix.data_hex} />
            </h4>
            <div className="p-2 bg-gray-900 rounded text-xs font-mono text-gray-400 break-all max-h-20 overflow-auto">
              {ix.data_hex}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// ============================================================================
// Transaction Detail Modal
// ============================================================================

function TransactionDetailModal({
  tx,
  onClose,
}: {
  tx: FullTransactionAnalysis;
  onClose: () => void;
}) {
  const [tab, setTab] = useState<"overview" | "instructions" | "accounts" | "logs">("overview");
  
  return (
    <div className="fixed inset-0 bg-black/80 flex items-center justify-center z-50 p-4" onClick={onClose}>
      <div 
        className="bg-[#12121a] border border-gray-800 rounded-xl w-full max-w-5xl max-h-[90vh] overflow-hidden flex flex-col"
        onClick={e => e.stopPropagation()}
      >
        {/* Header */}
        <div className="p-4 border-b border-gray-800 flex justify-between items-center shrink-0">
          <div className="flex items-center gap-4">
            <h2 className="text-lg font-semibold">Transaction Details</h2>
            <StatusBadge success={tx.success} />
          </div>
          <button onClick={onClose} className="text-gray-400 hover:text-white p-1">
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
        
        {/* Tabs */}
        <div className="border-b border-gray-800 flex shrink-0">
          {["overview", "instructions", "accounts", "logs"].map(t => (
            <button
              key={t}
              onClick={() => setTab(t as typeof tab)}
              className={`px-4 py-2 text-sm font-medium transition-colors ${
                tab === t ? "text-emerald-400 border-b-2 border-emerald-400" : "text-gray-400 hover:text-white"
              }`}
            >
              {t.charAt(0).toUpperCase() + t.slice(1)}
            </button>
          ))}
        </div>
        
        {/* Content */}
        <div className="flex-1 overflow-auto p-4">
          {tab === "overview" && (
            <div className="space-y-6">
              {/* Basic Info */}
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="text-xs text-gray-500 uppercase">Signature</label>
                  <div className="flex items-center gap-2">
                    <a
                      href={`https://solscan.io/tx/${tx.signature}`}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="font-mono text-sm text-blue-400 hover:text-blue-300 break-all"
                    >
                      {tx.signature}
                    </a>
                    <CopyButton text={tx.signature} />
                  </div>
                </div>
                <div>
                  <label className="text-xs text-gray-500 uppercase">Block Time</label>
                  <p className="text-sm">{tx.block_time_formatted}</p>
                </div>
              </div>
              
              <div className="grid grid-cols-4 gap-4">
                <div className="bg-gray-800/30 rounded-lg p-3">
                  <div className="text-lg font-bold">{tx.slot.toLocaleString()}</div>
                  <div className="text-xs text-gray-500 uppercase">Slot</div>
                </div>
                <div className="bg-gray-800/30 rounded-lg p-3">
                  <div className="text-lg font-bold">{(tx.fee / 1e9).toFixed(6)}</div>
                  <div className="text-xs text-gray-500 uppercase">Fee (SOL)</div>
                </div>
                <div className="bg-gray-800/30 rounded-lg p-3">
                  <div className="text-lg font-bold">{tx.compute_units_consumed?.toLocaleString() || "N/A"}</div>
                  <div className="text-xs text-gray-500 uppercase">Compute Units</div>
                </div>
                <div className="bg-gray-800/30 rounded-lg p-3">
                  <div className="text-lg font-bold">{tx.summary.primary_action}</div>
                  <div className="text-xs text-gray-500 uppercase">Action</div>
                </div>
              </div>
              
              {/* ORE Analysis */}
              {tx.ore_analysis && (
                <div className="bg-amber-500/10 border border-amber-500/30 rounded-lg p-4">
                  <h3 className="text-sm font-semibold text-amber-400 mb-3">ORE Deployment Analysis</h3>
                  <div className="grid grid-cols-3 gap-4 mb-4">
                    <div>
                      <div className="text-2xl font-bold text-amber-400">{tx.ore_analysis.deploy_count}</div>
                      <div className="text-xs text-gray-500">Deployments</div>
                    </div>
                    <div>
                      <div className="text-2xl font-bold text-amber-400">{tx.ore_analysis.total_deployed_sol.toFixed(6)}</div>
                      <div className="text-xs text-gray-500">Total SOL</div>
                    </div>
                    <div>
                      <div className="text-2xl font-bold text-amber-400">
                        {tx.ore_analysis.deployments.reduce((sum, d) => sum + d.squares.length, 0)}
                      </div>
                      <div className="text-xs text-gray-500">Squares</div>
                    </div>
                  </div>
                  {tx.ore_analysis.deployments.map((d, i) => (
                    <div key={i} className="p-3 bg-gray-900/50 rounded-lg mt-2">
                      <div className="flex justify-between items-start">
                        <div>
                          <div className="flex items-center gap-2">
                            <span className={d.round_matches ? "text-emerald-400" : "text-red-400"}>
                              {d.round_matches ? "✓ Matches" : "✗ Wrong Round"}
                            </span>
                            <span className="text-gray-500">|</span>
                            <span className="text-gray-400">{d.total_sol.toFixed(6)} SOL</span>
                          </div>
                          <div className="text-xs text-gray-500 mt-1">
                            Miner: <Pubkey address={d.miner} short />
                          </div>
                        </div>
                        <SquaresGrid squares={d.squares} />
                      </div>
                    </div>
                  ))}
                </div>
              )}
              
              {/* Balance Changes */}
              {tx.balance_changes.length > 0 && (
                <div>
                  <h3 className="text-sm font-semibold text-gray-300 mb-3">Balance Changes</h3>
                  <div className="space-y-2">
                    {tx.balance_changes.map((bc, i) => (
                      <div key={i} className="flex justify-between items-center p-2 bg-gray-800/30 rounded-lg">
                        <Pubkey address={bc.account} short />
                        <SolAmount lamports={bc.change} />
                      </div>
                    ))}
                  </div>
                </div>
              )}
              
              {/* Programs Used */}
              <div>
                <h3 className="text-sm font-semibold text-gray-300 mb-3">Programs Invoked</h3>
                <div className="flex flex-wrap gap-2">
                  {tx.programs_invoked.map((p, i) => (
                    <div key={i} className="flex items-center gap-2">
                      <ProgramBadge name={p.name} />
                      <span className="text-xs text-gray-500">×{p.invocation_count}</span>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          )}
          
          {tab === "instructions" && (
            <div className="space-y-3">
              <h3 className="text-sm font-semibold text-gray-300">
                Outer Instructions ({tx.instructions.length})
              </h3>
              {tx.instructions.map((ix) => (
                <InstructionCard key={ix.index} ix={ix} />
              ))}
              
              {tx.inner_instructions.length > 0 && (
                <>
                  <h3 className="text-sm font-semibold text-gray-300 mt-6">
                    Inner Instructions ({tx.inner_instructions.reduce((sum, g) => sum + g.instructions.length, 0)})
                  </h3>
                  {tx.inner_instructions.map((group) => (
                    <div key={group.parent_index} className="space-y-2 ml-4">
                      <div className="text-xs text-gray-500">Parent #{group.parent_index}</div>
                      {group.instructions.map((ix) => (
                        <InstructionCard key={ix.index} ix={ix} isInner parentIndex={group.parent_index} />
                      ))}
                    </div>
                  ))}
                </>
              )}
            </div>
          )}
          
          {tab === "accounts" && (
            <div className="space-y-2">
              {tx.all_accounts.map((acc) => (
                <div 
                  key={acc.index} 
                  className={`flex items-center justify-between p-3 rounded-lg ${
                    acc.is_program ? "bg-purple-500/10" : acc.balance_change !== 0 ? "bg-gray-800/30" : "bg-gray-900/30"
                  }`}
                >
                  <div className="flex items-center gap-3">
                    <span className="text-xs text-gray-500 w-6">#{acc.index}</span>
                    <Pubkey address={acc.pubkey} />
                    <div className="flex gap-1">
                      {acc.is_signer && (
                        <span className="px-1.5 py-0.5 text-xs rounded bg-blue-500/20 text-blue-400">Signer</span>
                      )}
                      {acc.is_writable && (
                        <span className="px-1.5 py-0.5 text-xs rounded bg-amber-500/20 text-amber-400">Writable</span>
                      )}
                      {acc.is_program && (
                        <span className="px-1.5 py-0.5 text-xs rounded bg-purple-500/20 text-purple-400">
                          {acc.program_name || "Program"}
                        </span>
                      )}
                    </div>
                  </div>
                  {!acc.is_program && acc.balance_change !== 0 && (
                    <SolAmount lamports={acc.balance_change} />
                  )}
                </div>
              ))}
            </div>
          )}
          
          {tab === "logs" && (
            <div className="space-y-1 font-mono text-xs">
              {tx.logs.length > 0 ? (
                tx.logs.map((log, i) => (
                  <div 
                    key={i} 
                    className={`p-2 rounded ${
                      log.includes("Error") || log.includes("failed") 
                        ? "bg-red-500/10 text-red-300"
                        : log.includes("success")
                        ? "bg-emerald-500/10 text-emerald-300"
                        : "bg-gray-800/30 text-gray-400"
                    }`}
                  >
                    {log}
                  </div>
                ))
              ) : (
                <div className="text-gray-500">No logs available</div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// Main Page
// ============================================================================

export default function TransactionsPage() {
  const router = useRouter();
  const { isAuthenticated, isLoading: authLoading } = useAdmin();
  
  const [roundId, setRoundId] = useState<string>("");
  const [signature, setSignature] = useState<string>("");
  const [searchMode, setSearchMode] = useState<"round" | "signature">("round");
  const [data, setData] = useState<FullAnalysisResponse | null>(null);
  const [singleTx, setSingleTx] = useState<FullTransactionAnalysis | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedTx, setSelectedTx] = useState<FullTransactionAnalysis | null>(null);
  const [offset, setOffset] = useState(0);
  const limit = 25;
  const [downloading, setDownloading] = useState(false);

  useEffect(() => {
    if (!authLoading && !isAuthenticated) {
      router.push("/admin");
    }
  }, [authLoading, isAuthenticated, router]);

  const fetchRoundData = useCallback(async (roundIdNum: number, newOffset: number = 0) => {
    setLoading(true);
    setError(null);
    setSingleTx(null);
    try {
      const result = await api.getFullTransactionAnalysis(roundIdNum, { limit, offset: newOffset });
      setData(result);
      setOffset(newOffset);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to fetch");
    } finally {
      setLoading(false);
    }
  }, []);

  const fetchSingleTx = useCallback(async (sig: string) => {
    setLoading(true);
    setError(null);
    setData(null);
    try {
      const result = await api.getSingleTransaction(sig);
      setSingleTx(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to fetch transaction");
    } finally {
      setLoading(false);
    }
  }, []);

  const handleSearch = () => {
    if (searchMode === "round") {
      const roundIdNum = parseInt(roundId);
      if (isNaN(roundIdNum) || roundIdNum < 0) {
        setError("Please enter a valid round ID");
        return;
      }
      fetchRoundData(roundIdNum, 0);
    } else {
      if (!signature.trim()) {
        setError("Please enter a signature");
        return;
      }
      fetchSingleTx(signature.trim());
    }
  };

  const handleDownloadRaw = async () => {
    const roundIdNum = parseInt(roundId);
    if (isNaN(roundIdNum)) return;
    
    setDownloading(true);
    try {
      const rawTxns = await api.getRawTransactions(roundIdNum);
      const blob = new Blob([JSON.stringify(rawTxns, null, 2)], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `round_${roundIdNum}_transactions.json`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to download");
    } finally {
      setDownloading(false);
    }
  };

  if (authLoading) {
    return (
      <AdminShell>
        <div className="flex items-center justify-center h-64">
          <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-emerald-500" />
        </div>
      </AdminShell>
    );
  }

  if (!isAuthenticated) return null;

  return (
    <AdminShell>
      <div className="space-y-6">
        <h1 className="text-2xl font-bold">Transaction Analyzer</h1>
        
        {/* Search */}
        <div className="bg-[#12121a] border border-gray-800/50 rounded-xl p-4">
          <div className="flex gap-4 mb-4">
            <button
              onClick={() => setSearchMode("round")}
              className={`px-3 py-1.5 text-sm rounded-lg ${
                searchMode === "round" ? "bg-emerald-500 text-black" : "bg-gray-800 text-gray-400"
              }`}
            >
              By Round
            </button>
            <button
              onClick={() => setSearchMode("signature")}
              className={`px-3 py-1.5 text-sm rounded-lg ${
                searchMode === "signature" ? "bg-emerald-500 text-black" : "bg-gray-800 text-gray-400"
              }`}
            >
              By Signature
            </button>
          </div>
          
          <div className="flex gap-4 items-end">
            {searchMode === "round" ? (
              <div className="flex-1 max-w-xs">
                <label className="block text-sm text-gray-400 mb-1">Round ID</label>
                <input
                  type="number"
                  value={roundId}
                  onChange={(e) => setRoundId(e.target.value)}
                  placeholder="Enter round ID"
                  className="w-full px-3 py-2 bg-gray-900 border border-gray-700 rounded-lg text-white focus:outline-none focus:border-emerald-500"
                  onKeyDown={(e) => e.key === "Enter" && handleSearch()}
                />
              </div>
            ) : (
              <div className="flex-1">
                <label className="block text-sm text-gray-400 mb-1">Transaction Signature</label>
                <input
                  type="text"
                  value={signature}
                  onChange={(e) => setSignature(e.target.value)}
                  placeholder="Enter transaction signature"
                  className="w-full px-3 py-2 bg-gray-900 border border-gray-700 rounded-lg text-white font-mono text-sm focus:outline-none focus:border-emerald-500"
                  onKeyDown={(e) => e.key === "Enter" && handleSearch()}
                />
              </div>
            )}
            <button
              onClick={handleSearch}
              disabled={loading}
              className="px-4 py-2 bg-emerald-500 text-black font-medium rounded-lg hover:bg-emerald-400 disabled:opacity-50"
            >
              {loading ? "Loading..." : "Analyze"}
            </button>
            {data && searchMode === "round" && (
              <button
                onClick={handleDownloadRaw}
                disabled={downloading}
                className="px-4 py-2 bg-gray-700 text-white font-medium rounded-lg hover:bg-gray-600 disabled:opacity-50"
              >
                {downloading ? "..." : "Download Raw"}
              </button>
            )}
          </div>
          
          {error && (
            <div className="mt-3 p-3 bg-red-500/10 border border-red-500/30 rounded-lg text-red-400 text-sm">
              {error}
            </div>
          )}
        </div>
        
        {/* Single Transaction View */}
        {singleTx && (
          <div className="bg-[#12121a] border border-gray-800/50 rounded-xl p-4">
            <TransactionDetailModal tx={singleTx} onClose={() => setSingleTx(null)} />
          </div>
        )}
        
        {/* Round Summary */}
        {data && (
          <>
            <div className="bg-[#12121a] border border-gray-800/50 rounded-xl p-4">
              <h2 className="text-lg font-semibold mb-4">Round {data.round_id} Summary</h2>
              
              <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-6 gap-4 mb-6">
                <div className="bg-gray-800/30 rounded-lg p-3">
                  <div className="text-2xl font-bold">{data.round_summary.total_transactions}</div>
                  <div className="text-xs text-gray-500">Total Txns</div>
                </div>
                <div className="bg-emerald-500/10 border border-emerald-500/30 rounded-lg p-3">
                  <div className="text-2xl font-bold text-emerald-400">{data.round_summary.successful_transactions}</div>
                  <div className="text-xs text-gray-500">Successful</div>
                </div>
                <div className="bg-red-500/10 border border-red-500/30 rounded-lg p-3">
                  <div className="text-2xl font-bold text-red-400">{data.round_summary.failed_transactions}</div>
                  <div className="text-xs text-gray-500">Failed</div>
                </div>
                <div className="bg-gray-800/30 rounded-lg p-3">
                  <div className="text-2xl font-bold">{data.round_summary.total_fee_sol.toFixed(4)}</div>
                  <div className="text-xs text-gray-500">Total Fees (SOL)</div>
                </div>
                <div className="bg-gray-800/30 rounded-lg p-3">
                  <div className="text-2xl font-bold">{data.round_summary.unique_signers}</div>
                  <div className="text-xs text-gray-500">Unique Signers</div>
                </div>
                <div className="bg-gray-800/30 rounded-lg p-3">
                  <div className="text-2xl font-bold">{(data.round_summary.total_compute_units / 1e6).toFixed(2)}M</div>
                  <div className="text-xs text-gray-500">Compute Units</div>
                </div>
              </div>
              
              {/* ORE Summary */}
              {data.round_summary.ore_summary && (
                <div className="bg-amber-500/10 border border-amber-500/30 rounded-lg p-4 mb-4">
                  <h3 className="text-sm font-semibold text-amber-400 mb-3">ORE Deployment Summary</h3>
                  <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-4">
                    <div>
                      <div className="text-2xl font-bold text-amber-400">{data.round_summary.ore_summary.total_deployments}</div>
                      <div className="text-xs text-gray-500">Total Deploys</div>
                    </div>
                    <div>
                      <div className="text-2xl font-bold text-emerald-400">{data.round_summary.ore_summary.deployments_matching_round}</div>
                      <div className="text-xs text-gray-500">Matching Round</div>
                    </div>
                    <div>
                      <div className="text-2xl font-bold text-red-400">{data.round_summary.ore_summary.deployments_wrong_round}</div>
                      <div className="text-xs text-gray-500">Wrong Round</div>
                    </div>
                    <div>
                      <div className="text-2xl font-bold text-amber-400">{data.round_summary.ore_summary.unique_miners}</div>
                      <div className="text-xs text-gray-500">Unique Miners</div>
                    </div>
                  </div>
                  <div className="flex items-center gap-4">
                    <div className="text-lg">
                      Total Deployed: <span className="font-bold text-amber-400">{data.round_summary.ore_summary.total_deployed_sol.toFixed(6)} SOL</span>
                    </div>
                  </div>
                  
                  {/* Squares breakdown */}
                  <div className="mt-4">
                    <div className="text-xs text-gray-500 uppercase mb-2">Squares Breakdown</div>
                    <div className="grid grid-cols-5 gap-2">
                      {data.round_summary.ore_summary.squares_deployed.map(sq => (
                        <div key={sq.square} className="bg-gray-900/50 rounded p-2 text-center">
                          <div className="text-sm font-bold text-amber-400">{sq.square + 1}</div>
                          <div className="text-xs text-gray-500">{sq.deployment_count} deploys</div>
                          <div className="text-xs text-gray-400">{(sq.total_lamports / 1e9).toFixed(4)}</div>
                        </div>
                      ))}
                    </div>
                  </div>
                </div>
              )}
              
              {/* Programs Used */}
              <div>
                <h3 className="text-sm font-semibold text-gray-300 mb-2">Programs Used</h3>
                <div className="flex flex-wrap gap-2">
                  {data.round_summary.programs_used.map((p, i) => (
                    <div key={i} className="flex items-center gap-2 bg-gray-800/30 rounded-lg px-3 py-1">
                      <ProgramBadge name={p.name} />
                      <span className="text-xs text-gray-500">×{p.invocation_count}</span>
                    </div>
                  ))}
                </div>
              </div>
            </div>
            
            {/* Transactions Table */}
            <div className="bg-[#12121a] border border-gray-800/50 rounded-xl overflow-hidden">
              <div className="p-4 border-b border-gray-800/50 flex justify-between items-center">
                <h2 className="text-lg font-semibold">
                  Transactions ({offset + 1}-{Math.min(offset + data.transactions.length, data.total_transactions)} of {data.total_transactions})
                </h2>
                <div className="flex gap-2">
                  <button
                    onClick={() => fetchRoundData(data.round_id, Math.max(0, offset - limit))}
                    disabled={offset === 0 || loading}
                    className="px-3 py-1 bg-gray-700 text-sm rounded-lg hover:bg-gray-600 disabled:opacity-50"
                  >
                    Previous
                  </button>
                  <button
                    onClick={() => fetchRoundData(data.round_id, offset + limit)}
                    disabled={offset + data.transactions.length >= data.total_transactions || loading}
                    className="px-3 py-1 bg-gray-700 text-sm rounded-lg hover:bg-gray-600 disabled:opacity-50"
                  >
                    Next
                  </button>
                </div>
              </div>
              
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead className="bg-gray-900/50 text-gray-400 text-xs uppercase">
                    <tr>
                      <th className="px-4 py-3 text-left">Signature</th>
                      <th className="px-4 py-3 text-center">Status</th>
                      <th className="px-4 py-3 text-right">Slot</th>
                      <th className="px-4 py-3 text-left">Action</th>
                      <th className="px-4 py-3 text-center">IXs</th>
                      <th className="px-4 py-3 text-right">Fee</th>
                      <th className="px-4 py-3 text-center">Details</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-gray-800/50">
                    {data.transactions.map((tx) => (
                      <tr key={tx.signature} className="hover:bg-gray-800/30 transition-colors">
                        <td className="px-4 py-3">
                          <a
                            href={`https://solscan.io/tx/${tx.signature}`}
                            target="_blank"
                            rel="noopener noreferrer"
                            className="font-mono text-xs text-blue-400 hover:text-blue-300"
                          >
                            {tx.signature.slice(0, 8)}...{tx.signature.slice(-8)}
                          </a>
                        </td>
                        <td className="px-4 py-3 text-center">
                          <StatusBadge success={tx.success} />
                        </td>
                        <td className="px-4 py-3 text-right font-mono text-xs">
                          {tx.slot.toLocaleString()}
                        </td>
                        <td className="px-4 py-3">
                          <span className={`text-xs ${tx.ore_analysis ? "text-amber-400" : "text-gray-400"}`}>
                            {tx.summary.primary_action}
                          </span>
                        </td>
                        <td className="px-4 py-3 text-center text-xs">
                          {tx.summary.total_instructions}+{tx.summary.total_inner_instructions}
                        </td>
                        <td className="px-4 py-3 text-right font-mono text-xs">
                          {(tx.fee / 1e9).toFixed(6)}
                        </td>
                        <td className="px-4 py-3 text-center">
                          <button
                            onClick={() => setSelectedTx(tx)}
                            className="px-2 py-1 bg-gray-700 text-xs rounded hover:bg-gray-600"
                          >
                            View
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          </>
        )}
        
        {/* Empty state */}
        {!data && !singleTx && !loading && !error && (
          <div className="bg-[#12121a] border border-gray-800/50 rounded-xl p-12 text-center">
            <div className="text-4xl mb-4">🔍</div>
            <div className="text-gray-400 mb-2">Comprehensive Transaction Analyzer</div>
            <div className="text-sm text-gray-500 max-w-md mx-auto">
              Analyze transactions by round ID or signature. View detailed instruction parsing, 
              account changes, program invocations, and ORE-specific deployment data.
            </div>
          </div>
        )}
      </div>
      
      {selectedTx && (
        <TransactionDetailModal tx={selectedTx} onClose={() => setSelectedTx(null)} />
      )}
    </AdminShell>
  );
}
