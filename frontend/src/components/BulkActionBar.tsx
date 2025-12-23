"use client";

import { useState } from "react";
import { PublicKey } from "@solana/web3.js";
import { formatSol } from "@/lib/accounts";
import { parseSolToLamports } from "@/lib/accounts";

interface BulkActionBarProps {
  selectedCount: number;
  totalCount: number;
  lowBalanceCount?: number;
  onSelectAll: () => void;
  onDeselectAll: () => void;
  onSelectLowBalance?: () => void;
  onBulkDeposit?: (authId: bigint, amount: bigint) => Promise<void>;
  onBulkWithdraw?: () => Promise<void>;
  onBulkCheckpoint?: () => Promise<void>;
  onBulkClaimOre?: () => Promise<void>;
  onBulkClaimSol?: () => Promise<void>;
  onBulkUpdate?: (deployAuthority: PublicKey, bpsFee: bigint, flatFee: bigint, maxPerRound: bigint) => Promise<void>;
  totalAutodeployBalance?: bigint;
}

export function BulkActionBar({
  selectedCount,
  totalCount,
  lowBalanceCount,
  onSelectAll,
  onDeselectAll,
  onSelectLowBalance,
  onBulkDeposit,
  onBulkWithdraw,
  onBulkCheckpoint,
  onBulkClaimOre,
  onBulkClaimSol,
  onBulkUpdate,
  totalAutodeployBalance,
}: BulkActionBarProps) {
  const [showDepositModal, setShowDepositModal] = useState(false);
  const [showUpdateModal, setShowUpdateModal] = useState(false);
  const [depositAmount, setDepositAmount] = useState("");
  const [updateDeployAuthority, setUpdateDeployAuthority] = useState("");
  const [updateBpsFee, setUpdateBpsFee] = useState("");
  const [updateFlatFee, setUpdateFlatFee] = useState("");
  const [updateMaxPerRound, setUpdateMaxPerRound] = useState("1000000000");
  const [loading, setLoading] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleBulkDeposit = async () => {
    if (!depositAmount || !onBulkDeposit) return;
    try {
      setLoading("deposit");
      setError(null);
      const lamports = parseSolToLamports(depositAmount);
      await onBulkDeposit(BigInt(0), lamports);
      setShowDepositModal(false);
      setDepositAmount("");
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(null);
    }
  };

  const handleBulkWithdraw = async () => {
    if (!onBulkWithdraw) return;
    try {
      setLoading("withdraw");
      setError(null);
      await onBulkWithdraw();
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(null);
    }
  };

  const handleBulkCheckpoint = async () => {
    if (!onBulkCheckpoint) return;
    try {
      setLoading("checkpoint");
      setError(null);
      await onBulkCheckpoint();
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(null);
    }
  };

  const handleBulkClaimOre = async () => {
    if (!onBulkClaimOre) return;
    try {
      setLoading("claimOre");
      setError(null);
      await onBulkClaimOre();
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(null);
    }
  };

  const handleBulkClaimSol = async () => {
    if (!onBulkClaimSol) return;
    try {
      setLoading("claimSol");
      setError(null);
      await onBulkClaimSol();
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(null);
    }
  };

  const handleBulkUpdate = async () => {
    if (!onBulkUpdate || !updateDeployAuthority || !updateBpsFee) return;
    try {
      setLoading("update");
      setError(null);
      const deployAuthority = new PublicKey(updateDeployAuthority);
      const bpsFee = BigInt(Math.floor(parseFloat(updateBpsFee) * 100)); // Convert % to basis points
      const flatFee = updateFlatFee ? BigInt(Math.floor(parseFloat(updateFlatFee))) : BigInt(0); // Already in lamports
      const maxPerRound = BigInt(Math.floor(parseFloat(updateMaxPerRound) || 0)); // Already in lamports
      await onBulkUpdate(deployAuthority, bpsFee, flatFee, maxPerRound);
      setShowUpdateModal(false);
      setUpdateDeployAuthority("");
      setUpdateBpsFee("");
      setUpdateFlatFee("");
      setUpdateMaxPerRound("1000000000");
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(null);
    }
  };

  if (selectedCount === 0) {
    return (
      <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-3 flex items-center justify-between">
        <div className="flex items-center gap-4">
          <button
            onClick={onSelectAll}
            className="text-sm text-purple-400 hover:text-purple-300"
          >
            Select All ({totalCount})
          </button>
          {onSelectLowBalance && lowBalanceCount !== undefined && lowBalanceCount > 0 && (
            <button
              onClick={onSelectLowBalance}
              className="text-sm text-orange-400 hover:text-orange-300"
            >
              Select Low Balance ({lowBalanceCount})
            </button>
          )}
        </div>
        <p className="text-sm text-zinc-500">Select miners to perform bulk actions</p>
      </div>
    );
  }

  return (
    <>
      <div className="bg-purple-900/30 border border-purple-700 rounded-lg p-3 flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-4">
          <span className="text-sm font-medium text-purple-300">
            {selectedCount} of {totalCount} selected
          </span>
          <button
            onClick={selectedCount === totalCount ? onDeselectAll : onSelectAll}
            className="text-sm text-purple-400 hover:text-purple-300"
          >
            {selectedCount === totalCount ? "Deselect All" : "Select All"}
          </button>
          {onSelectLowBalance && lowBalanceCount !== undefined && lowBalanceCount > 0 && (
            <button
              onClick={onSelectLowBalance}
              className="text-sm text-orange-400 hover:text-orange-300"
            >
              Select Low Balance ({lowBalanceCount})
            </button>
          )}
          <button
            onClick={onDeselectAll}
            className="text-sm text-zinc-400 hover:text-zinc-300"
          >
            Clear
          </button>
        </div>

        {error && (
          <p className="text-sm text-red-400 w-full">{error}</p>
        )}

        <div className="flex flex-wrap gap-2">
          {onBulkDeposit && (
            <button
              onClick={() => setShowDepositModal(true)}
              disabled={loading !== null}
              className="px-3 py-1.5 text-sm bg-green-600 hover:bg-green-500 rounded disabled:opacity-50"
            >
              {loading === "deposit" ? "..." : "Deposit All"}
            </button>
          )}
          {onBulkWithdraw && (
            <button
              onClick={handleBulkWithdraw}
              disabled={loading !== null}
              className="px-3 py-1.5 text-sm bg-yellow-600 hover:bg-yellow-500 rounded disabled:opacity-50"
            >
              {loading === "withdraw" ? "..." : "Claim SOL All"}
            </button>
          )}
          {onBulkCheckpoint && (
            <button
              onClick={handleBulkCheckpoint}
              disabled={loading !== null}
              className="px-3 py-1.5 text-sm bg-blue-600 hover:bg-blue-500 rounded disabled:opacity-50"
            >
              {loading === "checkpoint" ? "..." : "Checkpoint All"}
            </button>
          )}
          {onBulkClaimSol && (
            <button
              onClick={handleBulkClaimSol}
              disabled={loading !== null}
              className="px-3 py-1.5 text-sm bg-yellow-600 hover:bg-yellow-500 rounded disabled:opacity-50"
            >
              {loading === "claimSol" ? "..." : "Claim SOL All"}
            </button>
          )}
          {onBulkClaimOre && (
            <button
              onClick={handleBulkClaimOre}
              disabled={loading !== null}
              className="px-3 py-1.5 text-sm bg-purple-600 hover:bg-purple-500 rounded disabled:opacity-50"
            >
              {loading === "claimOre" ? "..." : "Claim ORE All"}
            </button>
          )}
          {onBulkUpdate && (
            <button
              onClick={() => setShowUpdateModal(true)}
              disabled={loading !== null}
              className="px-3 py-1.5 text-sm bg-pink-600 hover:bg-pink-500 rounded disabled:opacity-50"
            >
              {loading === "update" ? "..." : "Update All"}
            </button>
          )}
        </div>
      </div>

      {/* Deposit Modal */}
      {showDepositModal && (
        <div className="fixed inset-0 bg-black/70 flex items-center justify-center z-50">
          <div className="bg-zinc-900 border border-zinc-700 rounded-lg p-6 max-w-md w-full mx-4">
            <h3 className="text-lg font-semibold mb-4">Bulk Deposit to {selectedCount} Miners</h3>
            <div className="space-y-4">
              <div>
                <label className="block text-sm text-zinc-400 mb-1">Amount per miner (SOL)</label>
                <input
                  type="number"
                  value={depositAmount}
                  onChange={(e) => setDepositAmount(e.target.value)}
                  placeholder="0.1"
                  min="0"
                  step="0.01"
                  className="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded text-sm"
                />
                <p className="text-xs text-zinc-500 mt-1">
                  Total: {depositAmount ? (parseFloat(depositAmount) * selectedCount).toFixed(4) : "0"} SOL
                </p>
              </div>
              {error && <p className="text-sm text-red-400">{error}</p>}
              <div className="flex gap-2">
                <button
                  onClick={() => {
                    setShowDepositModal(false);
                    setError(null);
                  }}
                  className="flex-1 px-4 py-2 bg-zinc-700 hover:bg-zinc-600 rounded"
                  disabled={loading !== null}
                >
                  Cancel
                </button>
                <button
                  onClick={handleBulkDeposit}
                  className="flex-1 px-4 py-2 bg-green-600 hover:bg-green-500 rounded"
                  disabled={loading !== null || !depositAmount}
                >
                  {loading === "deposit" ? "Depositing..." : "Deposit"}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Update Modal */}
      {showUpdateModal && (
        <div className="fixed inset-0 bg-black/70 flex items-center justify-center z-50">
          <div className="bg-zinc-900 border border-zinc-700 rounded-lg p-6 max-w-md w-full mx-4">
            <h3 className="text-lg font-semibold mb-4">Bulk Update {selectedCount} Deployers</h3>
            <div className="space-y-4">
              <div>
                <label className="block text-sm text-zinc-400 mb-1">Deploy Authority (Pubkey)</label>
                <input
                  type="text"
                  value={updateDeployAuthority}
                  onChange={(e) => setUpdateDeployAuthority(e.target.value)}
                  placeholder="Enter public key..."
                  className="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded text-sm font-mono"
                />
              </div>
              <div>
                <label className="block text-sm text-zinc-400 mb-1">BPS Fee (%)</label>
                <input
                  type="number"
                  value={updateBpsFee}
                  onChange={(e) => setUpdateBpsFee(e.target.value)}
                  placeholder="10"
                  min="0"
                  max="100"
                  step="0.01"
                  className="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded text-sm"
                />
                <p className="text-xs text-zinc-500 mt-1">
                  e.g., 10 = 10% = 1000 basis points
                </p>
              </div>
              <div>
                <label className="block text-sm text-zinc-400 mb-1">Flat Fee (lamports, optional)</label>
                <input
                  type="number"
                  value={updateFlatFee}
                  onChange={(e) => setUpdateFlatFee(e.target.value)}
                  placeholder="0"
                  min="0"
                  step="1"
                  className="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded text-sm"
                />
                <p className="text-xs text-zinc-500 mt-1">
                  e.g., 1000 lamports = 0.000001 SOL
                </p>
              </div>
              <div>
                <label className="block text-sm text-zinc-400 mb-1">Max Per Round (lamports)</label>
                <input
                  type="number"
                  value={updateMaxPerRound}
                  onChange={(e) => setUpdateMaxPerRound(e.target.value)}
                  placeholder="1000000000"
                  min="0"
                  step="1"
                  className="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded text-sm"
                />
                <p className="text-xs text-zinc-500 mt-1">
                  Maximum lamports to deploy per round. Set to 0 for unlimited. (1 SOL = 1,000,000,000 lamports)
                </p>
              </div>
              {error && <p className="text-sm text-red-400">{error}</p>}
              <div className="flex gap-2">
                <button
                  onClick={() => {
                    setShowUpdateModal(false);
                    setError(null);
                  }}
                  className="flex-1 px-4 py-2 bg-zinc-700 hover:bg-zinc-600 rounded"
                  disabled={loading !== null}
                >
                  Cancel
                </button>
                <button
                  onClick={handleBulkUpdate}
                  className="flex-1 px-4 py-2 bg-pink-600 hover:bg-pink-500 rounded"
                  disabled={loading !== null || !updateDeployAuthority || !updateBpsFee}
                >
                  {loading === "update" ? "Updating..." : "Update All"}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
