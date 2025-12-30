"use client";

import { useEffect, useState, useCallback, Suspense } from "react";
import { useSearchParams } from "next/navigation";
import Link from "next/link";
import { AdminShell } from "@/components/admin/AdminShell";
import { 
  api, 
  FullAnalysisResponse, 
  FullTransactionAnalysis, 
  InstructionAnalysis,
  ParsedInstruction,
  RoundTransactionInfo,
  ExternalComparisonSummary,
  fetchExternalDeployments,
  calculateExternalSummary,
} from "@/lib/api";

// ============================================================================
// Utility Components
// ============================================================================

function StatusBadge({ success }: { success: boolean }) {
  return success ? (
    <span className="px-2 py-0.5 text-xs font-medium rounded-full bg-green-500/20 text-green-400 border border-green-500/30">
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
    "EVORE Program": "bg-green-500/20 text-green-400 border-green-500/30",
    "Memo": "bg-pink-500/20 text-pink-400 border-pink-500/30",
  };
  
  const colorClass = colors[name] || "bg-slate-500/20 text-slate-400 border-slate-500/30";
  
  return (
    <span className={`px-2 py-0.5 text-xs font-medium rounded-full border ${colorClass}`}>
      {name}
    </span>
  );
}

function SolAmount({ lamports }: { lamports: number }) {
  const sol = lamports / 1e9;
  const isNegative = sol < 0;
  const color = isNegative ? "text-red-400" : sol > 0 ? "text-green-400" : "text-slate-400";
  
  return (
    <span className={color}>
      {isNegative ? "" : "+"}{sol.toFixed(9)} SOL
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
    <div className="grid grid-cols-5 gap-0.5 w-16">
      {grid.map((active, i) => (
        <div
          key={i}
          className={`w-2.5 h-2.5 rounded-sm ${active ? "bg-amber-500" : "bg-slate-700"}`}
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
      className="text-slate-500 hover:text-slate-300 text-xs"
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
            <span className="text-slate-500 w-16">From:</span>
            <Pubkey address={parsed.from} short />
          </div>
          <div className="flex gap-2">
            <span className="text-slate-500 w-16">To:</span>
            <Pubkey address={parsed.to} short />
          </div>
          <div className="flex gap-2">
            <span className="text-slate-500 w-16">Amount:</span>
            <span className="text-green-400">{parsed.sol.toFixed(9)} SOL</span>
          </div>
        </div>
      );
    
    case "ComputeSetLimit":
      return (
        <div className="text-sm">
          <span className="text-slate-500">Units:</span>{" "}
          <span className="text-purple-400">{parsed.units.toLocaleString()}</span>
        </div>
      );
    
    case "ComputeSetPrice":
      return (
        <div className="text-sm">
          <span className="text-slate-500">Price:</span>{" "}
          <span className="text-purple-400">{parsed.micro_lamports.toLocaleString()} µL</span>
        </div>
      );
    
    case "OreDeploy":
      return (
        <div className="space-y-2 text-sm">
          <div className="grid grid-cols-2 gap-2">
            <div>
              <span className="text-slate-500">Signer:</span>
              <div><Pubkey address={parsed.signer} short /></div>
            </div>
            <div>
              <span className="text-slate-500">Miner:</span>
              <div><Pubkey address={parsed.miner} short /></div>
            </div>
          </div>
          <div className="flex gap-4 items-center">
            <div>
              <span className="text-slate-500">Round ID:</span>{" "}
              <span className="text-blue-400 font-mono">{parsed.round_id ?? "Unknown"}</span>
            </div>
          </div>
          <div className="flex gap-4 items-center">
            <div>
              <span className="text-slate-500">Per Square:</span>{" "}
              <span className="text-amber-400">{parsed.amount_sol.toFixed(6)} SOL</span>
            </div>
            <div>
              <span className="text-slate-500">Total:</span>{" "}
              <span className="text-amber-400 font-semibold">{parsed.total_sol.toFixed(6)} SOL</span>
            </div>
          </div>
          <div className="flex gap-4 items-center">
            <div>
              <span className="text-slate-500">Squares ({parsed.squares.length}):</span>
            </div>
            <SquaresGrid squares={parsed.squares} />
          </div>
        </div>
      );
    
    case "OreCheckpoint":
      return (
        <div className="space-y-2 text-sm">
          <div className="p-2 bg-purple-500/10 rounded border border-purple-500/30">
            <span className="text-purple-400 font-medium">Checkpoint - Previous Round</span>
          </div>
          <div className="grid grid-cols-2 gap-2">
            <div>
              <span className="text-slate-500">Signer:</span>
              <div><Pubkey address={parsed.signer} short /></div>
            </div>
            <div>
              <span className="text-slate-500">Miner:</span>
              <div><Pubkey address={parsed.miner} short /></div>
            </div>
          </div>
          <div>
            <span className="text-slate-500">Round ID:</span>{" "}
            <span className="text-purple-400 font-mono">{parsed.round_id ?? "Unknown"}</span>
            <span className="text-slate-500 text-xs ml-2">(previous round being checkpointed)</span>
          </div>
        </div>
      );
    
    case "OreClaim":
      return (
        <div className="space-y-2 text-sm">
          <div className="grid grid-cols-2 gap-2">
            <div>
              <span className="text-slate-500">Signer:</span>
              <div><Pubkey address={parsed.signer} short /></div>
            </div>
            <div>
              <span className="text-slate-500">Miner:</span>
              <div><Pubkey address={parsed.miner} short /></div>
            </div>
          </div>
          <div>
            <span className="text-slate-500">Beneficiary:</span>
            <div><Pubkey address={parsed.beneficiary} short /></div>
          </div>
        </div>
      );
    
    case "OreAutomate":
      return (
        <div className="space-y-2 text-sm">
          <div className="grid grid-cols-2 gap-2">
            <div>
              <span className="text-slate-500">Signer:</span>
              <div><Pubkey address={parsed.signer} short /></div>
            </div>
            <div>
              <span className="text-slate-500">Authority:</span>
              <div><Pubkey address={parsed.authority} short /></div>
            </div>
          </div>
          <div>
            <span className="text-slate-500">Automation PDA:</span>
            <div><Pubkey address={parsed.automation_pda} short /></div>
          </div>
        </div>
      );
    
    case "OreReset":
      return (
        <div className="space-y-2 text-sm">
          <div className="flex gap-4">
            <div>
              <span className="text-slate-500">Signer:</span>
              <div><Pubkey address={parsed.signer} short /></div>
            </div>
            <div>
              <span className="text-slate-500">Board:</span>
              <div><Pubkey address={parsed.board} short /></div>
            </div>
          </div>
        </div>
      );
    
    case "OreOther":
      return (
        <div className="text-sm">
          <span className="text-slate-500">Instruction:</span>{" "}
          <span className="text-amber-400">{parsed.instruction_name}</span>
          <span className="text-slate-500 ml-2">({parsed.accounts_count} accounts)</span>
        </div>
      );
    
    case "Memo":
      return (
        <div className="text-sm">
          <span className="text-slate-500">Message:</span>{" "}
          <span className="text-pink-400 font-mono">&quot;{parsed.message}&quot;</span>
        </div>
      );
    
    case "Unknown":
      return (
        <div className="text-sm text-slate-500">
          <span className="font-mono text-xs">{parsed.data_preview}</span>
        </div>
      );
    
    default:
      return (
        <div className="text-sm text-slate-400">
          <pre className="text-xs overflow-auto">{JSON.stringify(parsed, null, 2)}</pre>
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
    <div className={`border rounded-lg ${isInner ? "border-slate-700/50 bg-slate-800/30" : "border-slate-700/50 bg-slate-800/20"}`}>
      <div 
        className="p-3 flex items-center justify-between cursor-pointer hover:bg-slate-700/30"
        onClick={() => setExpanded(!expanded)}
      >
        <div className="flex items-center gap-3">
          <span className="text-xs text-slate-500">
            {isInner ? `${parentIndex}.${ix.index}` : `#${ix.index}`}
          </span>
          <ProgramBadge name={ix.program_name} />
          <span className="text-sm font-medium text-white">{ix.instruction_type}</span>
          {ix.parse_error && (
            <span className="text-xs text-red-400">⚠ Parse error</span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <span className="text-xs text-slate-500">{ix.data_length} bytes</span>
          <span className="text-slate-400">{expanded ? "▼" : "▶"}</span>
        </div>
      </div>
      
      {expanded && (
        <div className="p-4 border-t border-slate-700/50 space-y-4">
          {ix.parsed && (
            <div className="p-3 bg-slate-900/50 rounded-lg">
              <h4 className="text-xs text-slate-500 uppercase mb-2">Parsed Data</h4>
              <ParsedInstructionDisplay parsed={ix.parsed} />
            </div>
          )}
          
          {ix.parse_error && (
            <div className="p-3 bg-red-500/10 rounded-lg border border-red-500/30">
              <h4 className="text-xs text-red-400 uppercase mb-1">Parse Error</h4>
              <p className="text-sm text-red-300">{ix.parse_error}</p>
            </div>
          )}
          
          <div>
            <h4 className="text-xs text-slate-500 uppercase mb-2">Accounts ({ix.accounts.length})</h4>
            <div className="space-y-1">
              {ix.accounts.map((acc, i) => (
                <div key={i} className="flex items-center gap-2 text-xs">
                  <span className="text-slate-600 w-6">#{i}</span>
                  <Pubkey address={acc.pubkey} short />
                  {acc.role && <span className="text-slate-500">({acc.role})</span>}
                </div>
              ))}
            </div>
          </div>
          
          <div>
            <h4 className="text-xs text-slate-500 uppercase mb-2 flex items-center gap-2">
              Raw Data <CopyButton text={ix.data_hex} />
            </h4>
            <div className="p-2 bg-slate-900 rounded text-xs font-mono text-slate-400 break-all max-h-20 overflow-auto">
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
        className="bg-slate-900 border border-slate-700 rounded-xl w-full max-w-5xl max-h-[90vh] overflow-hidden flex flex-col"
        onClick={e => e.stopPropagation()}
      >
        <div className="p-4 border-b border-slate-700 flex justify-between items-center shrink-0">
          <div className="flex items-center gap-4">
            <h2 className="text-lg font-semibold text-white">Transaction Details</h2>
            <StatusBadge success={tx.success} />
          </div>
          <button onClick={onClose} className="text-slate-400 hover:text-white p-1">
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
        
        <div className="border-b border-slate-700 flex shrink-0">
          {["overview", "instructions", "accounts", "logs"].map(t => (
            <button
              key={t}
              onClick={() => setTab(t as typeof tab)}
              className={`px-4 py-2 text-sm font-medium transition-colors ${
                tab === t ? "text-blue-400 border-b-2 border-blue-400" : "text-slate-400 hover:text-white"
              }`}
            >
              {t.charAt(0).toUpperCase() + t.slice(1)}
            </button>
          ))}
        </div>
        
        <div className="flex-1 overflow-auto p-4">
          {tab === "overview" && (
            <div className="space-y-6">
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="text-xs text-slate-500 uppercase">Signature</label>
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
                  <label className="text-xs text-slate-500 uppercase">Block Time</label>
                  <p className="text-sm text-white">{tx.block_time_formatted}</p>
                </div>
              </div>
              
              <div className="grid grid-cols-4 gap-4">
                <div className="bg-slate-800/50 rounded-lg p-3">
                  <div className="text-lg font-bold text-white">{tx.slot.toLocaleString()}</div>
                  <div className="text-xs text-slate-500 uppercase">Slot</div>
                </div>
                <div className="bg-slate-800/50 rounded-lg p-3">
                  <div className="text-lg font-bold text-white">{(tx.fee / 1e9).toFixed(6)}</div>
                  <div className="text-xs text-slate-500 uppercase">Fee (SOL)</div>
                </div>
                <div className="bg-slate-800/50 rounded-lg p-3">
                  <div className="text-lg font-bold text-white">{tx.compute_units_consumed?.toLocaleString() || "N/A"}</div>
                  <div className="text-xs text-slate-500 uppercase">Compute Units</div>
                </div>
                <div className="bg-slate-800/50 rounded-lg p-3">
                  <div className="text-lg font-bold text-white">{tx.summary.primary_action}</div>
                  <div className="text-xs text-slate-500 uppercase">Action</div>
                </div>
              </div>
              
              {tx.ore_analysis && (
                <div className="bg-amber-500/10 border border-amber-500/30 rounded-lg p-4">
                  <h3 className="text-sm font-semibold text-amber-400 mb-3">ORE Deployment Analysis</h3>
                  <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-4">
                    <div>
                      <div className="text-2xl font-bold text-amber-400">{tx.ore_analysis.deploy_count}</div>
                      <div className="text-xs text-slate-500">Parsed Deployments</div>
                    </div>
                    <div>
                      <div className="text-2xl font-bold text-cyan-400">{tx.ore_analysis.logged_deploy_count}</div>
                      <div className="text-xs text-slate-500">Logged Deployments</div>
                    </div>
                    <div>
                      <div className="text-2xl font-bold text-amber-400">{tx.ore_analysis.total_deployed_sol.toFixed(6)}</div>
                      <div className="text-xs text-slate-500">Parsed SOL</div>
                    </div>
                    <div>
                      <div className="text-2xl font-bold text-cyan-400">{tx.ore_analysis.logged_deployed_sol.toFixed(6)}</div>
                      <div className="text-xs text-slate-500">Logged SOL</div>
                    </div>
                  </div>
                  
                  {/* Parsed Deployments */}
                  {tx.ore_analysis.deployments.length > 0 && (
                    <div className="mb-4">
                      <h4 className="text-xs font-semibold text-amber-400 mb-2">Parsed Deployments</h4>
                      {tx.ore_analysis.deployments.map((d, i) => (
                        <div key={i} className="p-3 bg-slate-900/50 rounded-lg mt-2">
                          <div className="flex justify-between items-start">
                            <div>
                              <div className="flex items-center gap-2 flex-wrap">
                                <span className={d.round_matches ? "text-green-400" : "text-red-400"}>
                                  {d.round_matches ? "✓ Matches" : "✗ Wrong Round"}
                                </span>
                                <span className="text-slate-500">|</span>
                                <span className="text-slate-400">
                                  Round {d.round_id ?? "?"} 
                                  {d.expected_round_id && d.round_id !== d.expected_round_id && (
                                    <span className="text-slate-500"> (expected {d.expected_round_id})</span>
                                  )}
                                </span>
                                <span className="text-slate-500">|</span>
                                <span className="text-amber-400">{d.total_sol.toFixed(6)} SOL</span>
                              </div>
                              <div className="text-xs text-slate-500 mt-1">
                                Authority: <Pubkey address={d.authority} short />
                              </div>
                            </div>
                            <SquaresGrid squares={d.squares} />
                          </div>
                        </div>
                      ))}
                    </div>
                  )}
                  
                  {/* Logged Deployments */}
                  {tx.ore_analysis.logged_deployments.length > 0 && (
                    <div>
                      <h4 className="text-xs font-semibold text-cyan-400 mb-2">Logged Deployments (from tx logs)</h4>
                      {tx.ore_analysis.logged_deployments.map((d, i) => (
                        <div key={i} className={`p-3 rounded-lg mt-2 ${d.matched_parsed ? "bg-slate-900/50" : "bg-orange-900/30 border border-orange-500/30"}`}>
                          <div className="flex items-center gap-2 flex-wrap">
                            <span className={d.round_matches ? "text-green-400" : "text-slate-400"}>
                              {d.round_matches ? "✓ Round Match" : "Other Round"}
                            </span>
                            <span className="text-slate-500">|</span>
                            <span className="text-slate-400">Round {d.round_id}</span>
                            <span className="text-slate-500">|</span>
                            <span className="text-cyan-400">{d.total_sol.toFixed(6)} SOL</span>
                            <span className="text-slate-500">|</span>
                            <span className="text-slate-400">{d.squares_count} squares × {d.amount_per_square_sol} SOL</span>
                            {!d.matched_parsed && (
                              <>
                                <span className="text-slate-500">|</span>
                                <span className="text-orange-400">⚠ No matching parsed deploy</span>
                              </>
                            )}
                          </div>
                          {d.authority && (
                            <div className="text-xs text-slate-500 mt-1">
                              Authority: <Pubkey address={d.authority} short />
                            </div>
                          )}
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              )}
              
              {tx.balance_changes.length > 0 && (
                <div>
                  <h3 className="text-sm font-semibold text-slate-300 mb-3">Balance Changes</h3>
                  <div className="space-y-2">
                    {tx.balance_changes.map((bc, i) => (
                      <div key={i} className="flex justify-between items-center p-2 bg-slate-800/30 rounded-lg">
                        <Pubkey address={bc.account} short />
                        <SolAmount lamports={bc.change} />
                      </div>
                    ))}
                  </div>
                </div>
              )}
              
              <div>
                <h3 className="text-sm font-semibold text-slate-300 mb-3">Programs Invoked</h3>
                <div className="flex flex-wrap gap-2">
                  {tx.programs_invoked.map((p, i) => (
                    <div key={i} className="flex items-center gap-2">
                      <ProgramBadge name={p.name} />
                      <span className="text-xs text-slate-500">×{p.invocation_count}</span>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          )}
          
          {tab === "instructions" && (
            <div className="space-y-3">
              <h3 className="text-sm font-semibold text-slate-300">
                Outer Instructions ({tx.instructions.length})
              </h3>
              {tx.instructions.map((ix) => (
                <InstructionCard key={ix.index} ix={ix} />
              ))}
              
              {tx.inner_instructions.length > 0 && (
                <>
                  <h3 className="text-sm font-semibold text-slate-300 mt-6">
                    Inner Instructions ({tx.inner_instructions.reduce((sum, g) => sum + g.instructions.length, 0)})
                  </h3>
                  {tx.inner_instructions.map((group) => (
                    <div key={group.parent_index} className="space-y-2 ml-4">
                      <div className="text-xs text-slate-500">Parent #{group.parent_index}</div>
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
                    acc.is_program ? "bg-purple-500/10" : acc.balance_change !== 0 ? "bg-slate-800/30" : "bg-slate-900/30"
                  }`}
                >
                  <div className="flex items-center gap-3">
                    <span className="text-xs text-slate-500 w-6">#{acc.index}</span>
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
                        ? "bg-green-500/10 text-green-300"
                        : "bg-slate-800/30 text-slate-400"
                    }`}
                  >
                    {log}
                  </div>
                ))
              ) : (
                <div className="text-slate-500">No logs available</div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// Miner Comparison Modal
// ============================================================================

interface MinerComparisonData {
  authority: string;
  parsed_lamports: number;
  logged_lamports: number;
  external_lamports: number;
}

function MinerComparisonModal({
  data,
  externalData,
  onClose,
}: {
  data: FullAnalysisResponse;
  externalData: ExternalComparisonSummary;
  onClose: () => void;
}) {
  // Build a map of all miners with their totals from each source
  const minerMap = new Map<string, MinerComparisonData>();
  
  // Add parsed deployments
  if (data.round_summary.ore_summary) {
    for (const tx of data.transactions) {
      if (tx.ore_analysis) {
        for (const d of tx.ore_analysis.deployments) {
          if (d.round_matches && d.authority) {
            const existing = minerMap.get(d.authority) || {
              authority: d.authority,
              parsed_lamports: 0,
              logged_lamports: 0,
              external_lamports: 0,
            };
            existing.parsed_lamports += d.total_lamports;
            minerMap.set(d.authority, existing);
          }
        }
      }
    }
  }
  
  // Add logged deployments
  for (const tx of data.transactions) {
    if (tx.ore_analysis) {
      for (const d of tx.ore_analysis.logged_deployments) {
        if (d.round_matches && d.authority) {
          const existing = minerMap.get(d.authority) || {
            authority: d.authority,
            parsed_lamports: 0,
            logged_lamports: 0,
            external_lamports: 0,
          };
          existing.logged_lamports += d.total_lamports;
          minerMap.set(d.authority, existing);
        }
      }
    }
  }
  
  // Add external deployments
  for (const d of externalData.deployments) {
    const existing = minerMap.get(d.pubkey) || {
      authority: d.pubkey,
      parsed_lamports: 0,
      logged_lamports: 0,
      external_lamports: 0,
    };
    existing.external_lamports += d.sol_deployed;
    minerMap.set(d.pubkey, existing);
  }
  
  // Convert to array and sort by external amount (descending)
  const miners = Array.from(minerMap.values()).sort(
    (a, b) => b.external_lamports - a.external_lamports
  );
  
  // Calculate totals
  const totals = miners.reduce(
    (acc, m) => ({
      parsed: acc.parsed + m.parsed_lamports,
      logged: acc.logged + m.logged_lamports,
      external: acc.external + m.external_lamports,
    }),
    { parsed: 0, logged: 0, external: 0 }
  );
  
  return (
    <div className="fixed inset-0 bg-black/80 flex items-center justify-center z-50 p-4" onClick={onClose}>
      <div 
        className="bg-slate-900 border border-slate-700 rounded-xl w-full max-w-4xl max-h-[90vh] overflow-hidden flex flex-col"
        onClick={e => e.stopPropagation()}
      >
        <div className="p-4 border-b border-slate-700 flex justify-between items-center shrink-0">
          <div>
            <h2 className="text-lg font-semibold text-white">Miner Deployment Comparison</h2>
            <div className="text-xs text-slate-400">Round {data.round_id} • {miners.length} unique miners</div>
          </div>
          <button onClick={onClose} className="text-slate-400 hover:text-white p-1">
            ✕
          </button>
        </div>
        
        <div className="flex-1 overflow-auto p-4">
          <table className="w-full text-sm">
            <thead className="bg-slate-800/50 text-slate-400 text-xs uppercase sticky top-0">
              <tr>
                <th className="px-3 py-2 text-left">Authority</th>
                <th className="px-3 py-2 text-right">Parsed (SOL)</th>
                <th className="px-3 py-2 text-right">Logged (SOL)</th>
                <th className="px-3 py-2 text-right">External (SOL)</th>
                <th className="px-3 py-2 text-right">Diff (Ext - Parsed)</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-700/50">
              {miners.map((m) => {
                const parsedSol = m.parsed_lamports / 1e9;
                const loggedSol = m.logged_lamports / 1e9;
                const externalSol = m.external_lamports / 1e9;
                const diff = externalSol - parsedSol;
                const hasDiff = Math.abs(diff) > 0.000001;
                
                return (
                  <tr key={m.authority} className={`hover:bg-slate-800/30 ${hasDiff ? "bg-yellow-900/10" : ""}`}>
                    <td className="px-3 py-2">
                      <a
                        href={`https://solscan.io/account/${m.authority}`}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="font-mono text-xs text-blue-400 hover:text-blue-300"
                        title={m.authority}
                      >
                        {m.authority.slice(0, 4)}...{m.authority.slice(-4)}
                      </a>
                    </td>
                    <td className="px-3 py-2 text-right font-mono text-xs text-amber-400">
                      {parsedSol.toFixed(6)}
                    </td>
                    <td className="px-3 py-2 text-right font-mono text-xs text-cyan-400">
                      {loggedSol.toFixed(6)}
                    </td>
                    <td className="px-3 py-2 text-right font-mono text-xs text-purple-400">
                      {externalSol.toFixed(6)}
                    </td>
                    <td className={`px-3 py-2 text-right font-mono text-xs ${hasDiff ? (diff > 0 ? "text-yellow-400" : "text-red-400") : "text-green-400"}`}>
                      {diff >= 0 ? "+" : ""}{diff.toFixed(6)}
                    </td>
                  </tr>
                );
              })}
            </tbody>
            <tfoot className="bg-slate-800/50 text-slate-300 text-xs font-semibold sticky bottom-0">
              <tr>
                <td className="px-3 py-2">TOTAL</td>
                <td className="px-3 py-2 text-right font-mono text-amber-400">
                  {(totals.parsed / 1e9).toFixed(6)}
                </td>
                <td className="px-3 py-2 text-right font-mono text-cyan-400">
                  {(totals.logged / 1e9).toFixed(6)}
                </td>
                <td className="px-3 py-2 text-right font-mono text-purple-400">
                  {(totals.external / 1e9).toFixed(6)}
                </td>
                <td className={`px-3 py-2 text-right font-mono ${Math.abs(totals.external - totals.parsed) > 1 ? "text-yellow-400" : "text-green-400"}`}>
                  {(totals.external - totals.parsed) / 1e9 >= 0 ? "+" : ""}{((totals.external - totals.parsed) / 1e9).toFixed(6)}
                </td>
              </tr>
            </tfoot>
          </table>
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// Main Page Content
// ============================================================================

function TransactionsPageContent() {
  const searchParams = useSearchParams();
  const initialRoundId = searchParams.get("round_id");
  
  const [roundId, setRoundId] = useState<string>(initialRoundId || "");
  const [signature, setSignature] = useState<string>("");
  const [searchMode, setSearchMode] = useState<"round" | "signature">("round");
  const [data, setData] = useState<FullAnalysisResponse | null>(null);
  const [singleTx, setSingleTx] = useState<FullTransactionAnalysis | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedTx, setSelectedTx] = useState<FullTransactionAnalysis | null>(null);
  const [offset, setOffset] = useState(0);
  const limit = 500;
  const [downloading, setDownloading] = useState(false);
  
  // Available rounds
  const [availableRounds, setAvailableRounds] = useState<RoundTransactionInfo[]>([]);
  const [roundsLoading, setRoundsLoading] = useState(true);
  const [roundsPage, setRoundsPage] = useState(1);
  const [roundsTotal, setRoundsTotal] = useState(0);
  
  // External API comparison
  const [externalData, setExternalData] = useState<ExternalComparisonSummary | null>(null);
  const [externalLoading, setExternalLoading] = useState(false);
  const [externalError, setExternalError] = useState<string | null>(null);
  const [showMinerComparison, setShowMinerComparison] = useState(false);

  // Load available rounds on mount
  useEffect(() => {
    const fetchRounds = async () => {
      setRoundsLoading(true);
      try {
        const result = await api.getRoundsWithTransactions(roundsPage, 20);
        setAvailableRounds(result.rounds);
        setRoundsTotal(result.total);
      } catch (err) {
        console.error("Failed to load rounds:", err);
      } finally {
        setRoundsLoading(false);
      }
    };
    fetchRounds();
  }, [roundsPage]);

  // Auto-load if round_id in URL
  useEffect(() => {
    if (initialRoundId) {
      const roundIdNum = parseInt(initialRoundId);
      if (!isNaN(roundIdNum)) {
        fetchRoundData(roundIdNum, 0);
      }
    }
  }, [initialRoundId]);

  const fetchRoundData = useCallback(async (roundIdNum: number, newOffset: number = 0) => {
    setLoading(true);
    setError(null);
    setSingleTx(null);
    // Clear external comparison when switching rounds
    setExternalData(null);
    setExternalError(null);
    try {
      const result = await api.getFullTransactionAnalysis(roundIdNum, { limit, offset: newOffset });
      setData(result);
      setOffset(newOffset);
      setRoundId(roundIdNum.toString());
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

  const fetchExternalComparison = useCallback(async (roundIdNum: number) => {
    setExternalLoading(true);
    setExternalError(null);
    setExternalData(null);
    
    const { data: deployments, error } = await fetchExternalDeployments(roundIdNum);
    
    if (error) {
      setExternalError(error);
      setExternalLoading(false);
      return;
    }
    
    if (!deployments || deployments.length === 0) {
      setExternalError("No deployment data found for this round");
      setExternalLoading(false);
      return;
    }
    
    const summary = calculateExternalSummary(deployments);
    setExternalData(summary);
    setExternalLoading(false);
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

  return (
    <AdminShell title="Transaction Analyzer" subtitle="Blockchain explorer-level transaction analysis">
      <div className="space-y-6">
        {/* Search + Available Rounds */}
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          {/* Search Panel */}
          <div className="lg:col-span-2 bg-slate-900 border border-slate-700 rounded-xl p-4">
            <div className="flex gap-4 mb-4">
              <button
                onClick={() => setSearchMode("round")}
                className={`px-3 py-1.5 text-sm rounded-lg ${
                  searchMode === "round" ? "bg-blue-500 text-white" : "bg-slate-800 text-slate-400"
                }`}
              >
                By Round
              </button>
              <button
                onClick={() => setSearchMode("signature")}
                className={`px-3 py-1.5 text-sm rounded-lg ${
                  searchMode === "signature" ? "bg-blue-500 text-white" : "bg-slate-800 text-slate-400"
                }`}
              >
                By Signature
              </button>
            </div>
            
            <div className="flex gap-4 items-end">
              {searchMode === "round" ? (
                <div className="flex-1">
                  <label className="block text-sm text-slate-400 mb-1">Round ID</label>
                  <input
                    type="number"
                    value={roundId}
                    onChange={(e) => setRoundId(e.target.value)}
                    placeholder="Enter round ID"
                    className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white focus:outline-none focus:border-blue-500"
                    onKeyDown={(e) => e.key === "Enter" && handleSearch()}
                  />
                </div>
              ) : (
                <div className="flex-1">
                  <label className="block text-sm text-slate-400 mb-1">Transaction Signature</label>
                  <input
                    type="text"
                    value={signature}
                    onChange={(e) => setSignature(e.target.value)}
                    placeholder="Enter transaction signature"
                    className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white font-mono text-sm focus:outline-none focus:border-blue-500"
                    onKeyDown={(e) => e.key === "Enter" && handleSearch()}
                  />
                </div>
              )}
              <button
                onClick={handleSearch}
                disabled={loading}
                className="px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white font-medium rounded-lg disabled:opacity-50"
              >
                {loading ? "Loading..." : "Analyze"}
              </button>
              {data && searchMode === "round" && (
                <button
                  onClick={handleDownloadRaw}
                  disabled={downloading}
                  className="px-4 py-2 bg-slate-700 hover:bg-slate-600 text-white font-medium rounded-lg disabled:opacity-50"
                >
                  {downloading ? "..." : "Download"}
                </button>
              )}
            </div>
            
            {error && (
              <div className="mt-3 p-3 bg-red-500/10 border border-red-500/30 rounded-lg text-red-400 text-sm">
                {error}
              </div>
            )}
          </div>
          
          {/* Available Rounds */}
          <div className="bg-slate-900 border border-slate-700 rounded-xl p-4">
            <div className="flex justify-between items-center mb-3">
              <h3 className="text-sm font-semibold text-slate-300">Available Rounds</h3>
              <span className="text-xs text-slate-500">{roundsTotal} total</span>
            </div>
            
            {roundsLoading ? (
              <div className="flex items-center justify-center h-32">
                <div className="w-6 h-6 border-2 border-blue-500 border-t-transparent rounded-full animate-spin" />
              </div>
            ) : (
              <>
                <div className="space-y-1 max-h-48 overflow-y-auto">
                  {availableRounds.map(r => (
                    <button
                      key={r.round_id}
                      onClick={() => fetchRoundData(r.round_id, 0)}
                      className={`w-full flex justify-between items-center px-3 py-2 rounded-lg text-sm transition-colors ${
                        data?.round_id === r.round_id
                          ? "bg-blue-500/20 text-blue-400 border border-blue-500/30"
                          : "bg-slate-800/50 text-slate-300 hover:bg-slate-700/50"
                      }`}
                    >
                      <span className="font-mono">#{r.round_id}</span>
                      <span className="text-xs text-slate-500">{r.transaction_count} txns</span>
                    </button>
                  ))}
                </div>
                
                {roundsTotal > 20 && (
                  <div className="flex justify-between mt-3 pt-3 border-t border-slate-700">
                    <button
                      onClick={() => setRoundsPage(p => Math.max(1, p - 1))}
                      disabled={roundsPage === 1}
                      className="text-xs text-slate-400 hover:text-white disabled:opacity-50"
                    >
                      ← Prev
                    </button>
                    <span className="text-xs text-slate-500">Page {roundsPage}</span>
                    <button
                      onClick={() => setRoundsPage(p => p + 1)}
                      disabled={roundsPage * 20 >= roundsTotal}
                      className="text-xs text-slate-400 hover:text-white disabled:opacity-50"
                    >
                      Next →
                    </button>
                  </div>
                )}
              </>
            )}
          </div>
        </div>
        
        {/* Single Transaction View */}
        {singleTx && (
          <TransactionDetailModal tx={singleTx} onClose={() => setSingleTx(null)} />
        )}
        
        {/* Round Summary */}
        {data && (
          <>
            <div className="bg-slate-900 border border-slate-700 rounded-xl p-4">
              <h2 className="text-lg font-semibold text-white mb-4">Round {data.round_id} Summary</h2>
              
              <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-7 gap-4 mb-6">
                <div className="bg-slate-800/50 rounded-lg p-3">
                  <div className="text-2xl font-bold text-white">{data.total_transactions}</div>
                  <div className="text-xs text-slate-500">Total Raw</div>
                </div>
                <div className="bg-slate-800/50 rounded-lg p-3">
                  <div className="text-2xl font-bold text-white">{data.analyzed_count}</div>
                  <div className="text-xs text-slate-500">Analyzed</div>
                </div>
                {data.failed_transactions.length > 0 && (
                  <div className="bg-red-500/10 border border-red-500/30 rounded-lg p-3">
                    <div className="text-2xl font-bold text-red-400">{data.failed_transactions.length}</div>
                    <div className="text-xs text-slate-500">Parse Errors</div>
                  </div>
                )}
                <div className="bg-green-500/10 border border-green-500/30 rounded-lg p-3">
                  <div className="text-2xl font-bold text-green-400">{data.round_summary.successful_transactions}</div>
                  <div className="text-xs text-slate-500">Successful</div>
                </div>
                <div className="bg-red-500/10 border border-red-500/30 rounded-lg p-3">
                  <div className="text-2xl font-bold text-red-400">{data.round_summary.failed_transactions}</div>
                  <div className="text-xs text-slate-500">Tx Failed</div>
                </div>
                <div className="bg-slate-800/50 rounded-lg p-3">
                  <div className="text-2xl font-bold text-white">{data.round_summary.total_fee_sol.toFixed(4)}</div>
                  <div className="text-xs text-slate-500">Fees (SOL)</div>
                </div>
                <div className="bg-slate-800/50 rounded-lg p-3">
                  <div className="text-2xl font-bold text-white">{data.round_summary.unique_signers}</div>
                  <div className="text-xs text-slate-500">Signers</div>
                </div>
                <div className="bg-slate-800/50 rounded-lg p-3">
                  <div className="text-2xl font-bold text-white">{(data.round_summary.total_compute_units / 1e6).toFixed(2)}M</div>
                  <div className="text-xs text-slate-500">Compute</div>
                </div>
              </div>
              
              {data.round_summary.ore_summary && (
                <div className="bg-amber-500/10 border border-amber-500/30 rounded-lg p-4 mb-4">
                  <h3 className="text-sm font-semibold text-amber-400 mb-3">ORE Deployment Summary</h3>
                  <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-4">
                    <div>
                      <div className="text-2xl font-bold text-amber-400">{data.round_summary.ore_summary.total_deployments}</div>
                      <div className="text-xs text-slate-500">Deploys (Parsed)</div>
                    </div>
                    <div>
                      <div className="text-2xl font-bold text-cyan-400">{data.round_summary.ore_summary.logged_deploy_count}</div>
                      <div className="text-xs text-slate-500">Deploys (Logged)</div>
                    </div>
                    <div>
                      <div className="text-2xl font-bold text-green-400">{data.round_summary.ore_summary.deployments_matching_round}</div>
                      <div className="text-xs text-slate-500">Matching Round</div>
                    </div>
                    <div>
                      <div className="text-2xl font-bold text-red-400">{data.round_summary.ore_summary.deployments_wrong_round}</div>
                      <div className="text-xs text-slate-500">Wrong Round</div>
                    </div>
                  </div>
                  <div className="grid grid-cols-1 md:grid-cols-3 gap-4 mb-2">
                    <div className="bg-slate-800/50 rounded-lg p-3">
                      <div className="text-xs text-slate-500 mb-1">Parsed Total</div>
                      <div className="text-lg font-bold text-amber-400">{data.round_summary.ore_summary.total_deployed_sol.toFixed(6)} SOL</div>
                    </div>
                    <div className="bg-slate-800/50 rounded-lg p-3">
                      <div className="text-xs text-slate-500 mb-1">Logged Total</div>
                      <div className="text-lg font-bold text-cyan-400">{data.round_summary.ore_summary.logged_deployed_sol.toFixed(6)} SOL</div>
                    </div>
                    <div className="bg-slate-800/50 rounded-lg p-3">
                      <div className="text-xs text-slate-500 mb-1">Difference (Logged - Parsed)</div>
                      <div className={`text-lg font-bold ${data.round_summary.ore_summary.logged_vs_parsed_diff_lamports === 0 ? 'text-green-400' : data.round_summary.ore_summary.logged_vs_parsed_diff_lamports > 0 ? 'text-yellow-400' : 'text-red-400'}`}>
                        {data.round_summary.ore_summary.logged_vs_parsed_diff_sol >= 0 ? '+' : ''}{data.round_summary.ore_summary.logged_vs_parsed_diff_sol.toFixed(6)} SOL
                      </div>
                    </div>
                  </div>
                  <div className="text-xs text-slate-500 space-y-1">
                    <div>Unique Miners (Parsed): {data.round_summary.ore_summary.unique_miners}</div>
                    <div>Unique Miners (Logged): {data.round_summary.ore_summary.logged_unique_miners}</div>
                    {data.round_summary.ore_summary.logged_unmatched_count > 0 && (
                      <div className="text-orange-400">
                        ⚠ Unmatched Logged Deploys: {data.round_summary.ore_summary.logged_unmatched_count} (parsing issue)
                      </div>
                    )}
                  </div>
                  
                  {/* External API Comparison */}
                  <div className="mt-4 pt-4 border-t border-slate-700">
                    <div className="flex items-center justify-between mb-3">
                      <h4 className="text-sm font-semibold text-purple-400">External API Comparison</h4>
                      <button
                        onClick={() => fetchExternalComparison(data.round_id)}
                        disabled={externalLoading}
                        className="px-3 py-1 text-xs bg-purple-600 hover:bg-purple-500 disabled:bg-purple-800 disabled:opacity-50 rounded-lg transition-colors"
                      >
                        {externalLoading ? "Loading..." : externalData ? "Refresh" : "Compare"}
                      </button>
                    </div>
                    
                    {externalError && (
                      <div className="p-2 bg-red-500/10 border border-red-500/30 rounded-lg text-xs text-red-400 mb-3">
                        {externalError}
                      </div>
                    )}
                    
                    {externalData && (
                      <div className="space-y-3">
                        <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
                          <div className="bg-slate-800/50 rounded-lg p-3">
                            <div className="text-xs text-slate-500 mb-1">External Total</div>
                            <div className="text-lg font-bold text-purple-400">{externalData.total_sol.toFixed(6)} SOL</div>
                          </div>
                          <div className="bg-slate-800/50 rounded-lg p-3">
                            <div className="text-xs text-slate-500 mb-1">External Unique Miners</div>
                            <div className="text-lg font-bold text-purple-400">{externalData.unique_miners}</div>
                          </div>
                          <div className="bg-slate-800/50 rounded-lg p-3">
                            <div className="text-xs text-slate-500 mb-1">External Deployments</div>
                            <div className="text-lg font-bold text-purple-400">{externalData.deployments.length}</div>
                          </div>
                        </div>
                        
                        {/* Comparison Table */}
                        <div className="bg-slate-800/30 rounded-lg p-3">
                          <table className="w-full text-xs">
                            <thead>
                              <tr className="text-slate-400">
                                <th className="text-left py-1">Metric</th>
                                <th className="text-right py-1">Parsed</th>
                                <th className="text-right py-1">Logged</th>
                                <th className="text-right py-1">External</th>
                                <th className="text-right py-1">Diff (Ext - Parsed)</th>
                              </tr>
                            </thead>
                            <tbody className="text-slate-300">
                              <tr>
                                <td className="py-1">Unique Miners</td>
                                <td className="text-right text-amber-400">{data.round_summary.ore_summary.unique_miners}</td>
                                <td className="text-right text-cyan-400">{data.round_summary.ore_summary.logged_unique_miners}</td>
                                <td className="text-right text-purple-400">{externalData.unique_miners}</td>
                                <td className={`text-right ${externalData.unique_miners - data.round_summary.ore_summary.unique_miners === 0 ? 'text-green-400' : 'text-yellow-400'}`}>
                                  {externalData.unique_miners - data.round_summary.ore_summary.unique_miners >= 0 ? '+' : ''}{externalData.unique_miners - data.round_summary.ore_summary.unique_miners}
                                </td>
                              </tr>
                              <tr>
                                <td className="py-1">Total SOL</td>
                                <td className="text-right text-amber-400">{data.round_summary.ore_summary.total_deployed_sol.toFixed(6)}</td>
                                <td className="text-right text-cyan-400">{data.round_summary.ore_summary.logged_deployed_sol.toFixed(6)}</td>
                                <td className="text-right text-purple-400">{externalData.total_sol.toFixed(6)}</td>
                                <td className={`text-right ${Math.abs(externalData.total_sol - data.round_summary.ore_summary.total_deployed_sol) < 0.000001 ? 'text-green-400' : 'text-yellow-400'}`}>
                                  {externalData.total_sol - data.round_summary.ore_summary.total_deployed_sol >= 0 ? '+' : ''}{(externalData.total_sol - data.round_summary.ore_summary.total_deployed_sol).toFixed(6)}
                                </td>
                              </tr>
                            </tbody>
                          </table>
                        </div>
                        
                        <button
                          onClick={() => setShowMinerComparison(true)}
                          className="mt-3 px-3 py-1.5 text-xs bg-purple-600 hover:bg-purple-500 rounded-lg transition-colors w-full"
                        >
                          View All Miners Comparison
                        </button>
                      </div>
                    )}
                  </div>
                </div>
              )}
              
              {/* Failed Transactions */}
              {data.failed_transactions && data.failed_transactions.length > 0 && (
                <div className="p-3 bg-red-500/10 rounded-lg border border-red-500/30">
                  <h3 className="text-sm font-semibold text-red-400 mb-2">
                    ⚠ Failed to Parse ({data.failed_transactions.length})
                  </h3>
                  <div className="text-xs text-slate-400 mb-2">
                    These transactions failed to parse and may be missing from analysis:
                  </div>
                  <div className="space-y-2 max-h-48 overflow-y-auto">
                    {data.failed_transactions.map((ft, i) => (
                      <div key={i} className="p-2 bg-slate-900/50 rounded">
                        <div className="flex items-center gap-2 text-xs">
                          <a
                            href={`https://solscan.io/tx/${ft.signature}`}
                            target="_blank"
                            rel="noopener noreferrer"
                            className="font-mono text-blue-400 hover:text-blue-300"
                          >
                            {ft.signature.slice(0, 8)}...{ft.signature.slice(-8)}
                          </a>
                          <span className="text-slate-500">Slot: {ft.slot.toLocaleString()}</span>
                        </div>
                        <div className="text-xs text-red-400 mt-1 font-mono">
                          {ft.error}
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )}
              
              {/* Missing Automation States */}
              {data.missing_automation_states && data.missing_automation_states.length > 0 && (
                <div className="p-3 bg-orange-500/10 rounded-lg border border-orange-500/30">
                  <h3 className="text-sm font-semibold text-orange-400 mb-2">
                    ⚠ Missing Automation States ({data.missing_automation_states.length})
                  </h3>
                  <div className="text-xs text-slate-400 mb-2">
                    These deployments need automation state data for accurate reconstruction:
                  </div>
                  <div className="space-y-1 max-h-32 overflow-y-auto">
                    {data.missing_automation_states.slice(0, 10).map((m, i) => (
                      <div key={i} className="flex items-center gap-2 text-xs">
                        <Pubkey address={m.signature} short />
                        <span className="text-slate-500">ix #{m.ix_index}</span>
                        <span className="text-slate-500">-</span>
                        <Pubkey address={m.miner} short />
                      </div>
                    ))}
                    {data.missing_automation_states.length > 10 && (
                      <div className="text-xs text-slate-500">
                        ... and {data.missing_automation_states.length - 10} more
                      </div>
                    )}
                  </div>
                </div>
              )}
              
              <div>
                <h3 className="text-sm font-semibold text-slate-300 mb-2">Programs</h3>
                <div className="flex flex-wrap gap-2">
                  {data.round_summary.programs_used.map((p, i) => (
                    <div key={i} className="flex items-center gap-2 bg-slate-800/30 rounded-lg px-3 py-1">
                      <ProgramBadge name={p.name} />
                      <span className="text-xs text-slate-500">×{p.invocation_count}</span>
                    </div>
                  ))}
                </div>
              </div>
            </div>
            
            {/* Transactions Table */}
            <div className="bg-slate-900 border border-slate-700 rounded-xl overflow-hidden">
              <div className="p-4 border-b border-slate-700 flex justify-between items-center">
                <h2 className="text-lg font-semibold text-white">
                  Transactions ({offset + 1}-{Math.min(offset + data.transactions.length, data.total_transactions)} of {data.total_transactions})
                </h2>
                <div className="flex gap-2">
                  <button
                    onClick={() => fetchRoundData(data.round_id, Math.max(0, offset - limit))}
                    disabled={offset === 0 || loading}
                    className="px-3 py-1 bg-slate-700 text-sm rounded-lg hover:bg-slate-600 disabled:opacity-50 text-white"
                  >
                    Previous
                  </button>
                  <button
                    onClick={() => fetchRoundData(data.round_id, offset + limit)}
                    disabled={offset + data.transactions.length >= data.total_transactions || loading}
                    className="px-3 py-1 bg-slate-700 text-sm rounded-lg hover:bg-slate-600 disabled:opacity-50 text-white"
                  >
                    Next
                  </button>
                </div>
              </div>
              
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead className="bg-slate-800/50 text-slate-400 text-xs uppercase">
                    <tr>
                      <th className="px-4 py-3 text-left">Signature</th>
                      <th className="px-4 py-3 text-left">Signer</th>
                      <th className="px-4 py-3 text-left">Authority</th>
                      <th className="px-4 py-3 text-center">Status</th>
                      <th className="px-4 py-3 text-right">Slot</th>
                      <th className="px-4 py-3 text-left">Action</th>
                      <th className="px-4 py-3 text-center">IXs</th>
                      <th className="px-4 py-3 text-right">Fee</th>
                      <th className="px-4 py-3 text-center">Details</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-slate-700/50">
                    {data.transactions.map((tx) => {
                      const signer = tx.signers[0] || "";
                      const deployments = tx.ore_analysis?.deployments || [];
                      const uniqueAuthorities = Array.from(new Set(deployments.map(d => d.authority).filter(Boolean)));
                      const authorityCount = uniqueAuthorities.length;
                      const firstAuthority = uniqueAuthorities[0] || "";
                      
                      return (
                        <tr key={tx.signature} className="hover:bg-slate-800/30 transition-colors">
                          <td className="px-4 py-3">
                            <a
                              href={`https://solscan.io/tx/${tx.signature}`}
                              target="_blank"
                              rel="noopener noreferrer"
                              className="font-mono text-xs text-blue-400 hover:text-blue-300"
                            >
                              {tx.signature.slice(0, 4)}...{tx.signature.slice(-4)}
                            </a>
                          </td>
                          <td className="px-4 py-3">
                            {signer && (
                              <a
                                href={`https://solscan.io/account/${signer}`}
                                target="_blank"
                                rel="noopener noreferrer"
                                className="font-mono text-xs text-slate-400 hover:text-slate-300"
                                title={signer}
                              >
                                {signer.slice(0, 4)}...{signer.slice(-4)}
                              </a>
                            )}
                          </td>
                          <td className="px-4 py-3">
                            {authorityCount === 0 ? null : authorityCount === 1 ? (
                              <a
                                href={`https://solscan.io/account/${firstAuthority}`}
                                target="_blank"
                                rel="noopener noreferrer"
                                className="font-mono text-xs text-amber-400 hover:text-amber-300"
                                title={firstAuthority}
                              >
                                {firstAuthority.slice(0, 4)}...{firstAuthority.slice(-4)}
                              </a>
                            ) : (
                              <span 
                                className="text-xs text-amber-400 cursor-help"
                                title={uniqueAuthorities.join('\n')}
                              >
                                {authorityCount} miners
                              </span>
                            )}
                          </td>
                          <td className="px-4 py-3 text-center">
                            <StatusBadge success={tx.success} />
                          </td>
                          <td className="px-4 py-3 text-right font-mono text-xs text-white">
                            {tx.slot.toLocaleString()}
                          </td>
                          <td className="px-4 py-3">
                            <span className={`text-xs ${tx.ore_analysis ? "text-amber-400" : "text-slate-400"}`}>
                              {tx.summary.primary_action}
                            </span>
                          </td>
                          <td className="px-4 py-3 text-center text-xs text-slate-400">
                            {tx.summary.total_instructions}+{tx.summary.total_inner_instructions}
                          </td>
                          <td className="px-4 py-3 text-right font-mono text-xs text-slate-400">
                            {(tx.fee / 1e9).toFixed(6)}
                          </td>
                          <td className="px-4 py-3 text-center">
                            <button
                              onClick={() => setSelectedTx(tx)}
                              className="px-2 py-1 bg-slate-700 text-xs rounded hover:bg-slate-600 text-white"
                            >
                              View
                            </button>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            </div>
          </>
        )}
        
        {/* Empty state */}
        {!data && !singleTx && !loading && !error && (
          <div className="bg-slate-900 border border-slate-700 rounded-xl p-12 text-center">
            <div className="text-4xl mb-4">🔍</div>
            <div className="text-slate-400 mb-2">Select a round or enter a signature</div>
            <div className="text-sm text-slate-500 max-w-md mx-auto">
              Analyze transactions with detailed instruction parsing, balance changes, and ORE-specific deployment data.
            </div>
          </div>
        )}
      </div>
      
      {selectedTx && (
        <TransactionDetailModal tx={selectedTx} onClose={() => setSelectedTx(null)} />
      )}
      
      {showMinerComparison && data && externalData && (
        <MinerComparisonModal
          data={data}
          externalData={externalData}
          onClose={() => setShowMinerComparison(false)}
        />
      )}
    </AdminShell>
  );
}

// ============================================================================
// Main Page (with Suspense wrapper)
// ============================================================================

export default function TransactionsPage() {
  return (
    <Suspense fallback={
      <AdminShell title="Transaction Analyzer" subtitle="Loading...">
        <div className="flex items-center justify-center h-64">
          <div className="w-8 h-8 border-2 border-blue-500 border-t-transparent rounded-full animate-spin" />
        </div>
      </AdminShell>
    }>
      <TransactionsPageContent />
    </Suspense>
  );
}
