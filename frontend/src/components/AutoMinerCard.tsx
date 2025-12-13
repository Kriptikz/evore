"use client";

import { useState } from "react";
import { PublicKey } from "@solana/web3.js";
import { shortenPubkey, formatSol, formatOre, formatFee, parseSolToLamports } from "@/lib/accounts";
import { getManagedMinerAuthPda } from "@/lib/pda";
import { MIN_AUTODEPLOY_BALANCE, MIN_AUTODEPLOY_BALANCE_FIRST } from "@/lib/constants";

interface MinerData {
  address: PublicKey;
  authority: PublicKey;
  roundId: bigint;
  checkpointId: bigint;
  deployed: bigint[];
  rewardsSol: bigint;
  rewardsOre: bigint;
  refinedOre: bigint;
}

interface DeployerData {
  deployAuthority: PublicKey;
  bpsFee: bigint;  // Percentage fee in basis points (1000 = 10%)
  flatFee: bigint; // Flat fee in lamports (added on top of bpsFee)
  autodeployBalance: bigint;
}

interface AutoMinerCardProps {
  managerAddress: PublicKey;
  deployer?: DeployerData;
  miner?: MinerData;
  currentBoardRoundId?: bigint;
  onDeposit: (amount: bigint) => Promise<string>;
  onWithdraw: (rewardsSol: bigint, autodeployBalance: bigint) => Promise<string>;
  onCheckpoint: (roundId: bigint) => Promise<string>;
  onClaimOre: () => Promise<string>;
}

// Copy text to clipboard
async function copyToClipboard(text: string, setTooltip: (msg: string | null) => void) {
  try {
    await navigator.clipboard.writeText(text);
    setTooltip("Copied!");
    setTimeout(() => setTooltip(null), 1500);
  } catch {
    setTooltip("Failed");
    setTimeout(() => setTooltip(null), 1500);
  }
}

// Clickable pubkey that copies on click
function CopyablePubkey({ pubkey, short = true }: { pubkey: PublicKey; short?: boolean }) {
  const [tooltip, setTooltip] = useState<string | null>(null);
  
  return (
    <button
      onClick={() => copyToClipboard(pubkey.toBase58(), setTooltip)}
      className="font-mono text-purple-400 hover:text-purple-300 hover:bg-zinc-800 px-1 rounded transition-colors relative"
      title={pubkey.toBase58()}
    >
      {short ? shortenPubkey(pubkey) : pubkey.toBase58()}
      {tooltip && (
        <span className="absolute -top-6 left-1/2 -translate-x-1/2 text-xs bg-zinc-700 px-2 py-1 rounded whitespace-nowrap z-10">
          {tooltip}
        </span>
      )}
    </button>
  );
}

export function AutoMinerCard({
  managerAddress,
  deployer,
  miner,
  currentBoardRoundId,
  onDeposit,
  onWithdraw,
  onCheckpoint,
  onClaimOre,
}: AutoMinerCardProps) {
  const [showDeposit, setShowDeposit] = useState(false);
  const [depositAmount, setDepositAmount] = useState("");
  const [loading, setLoading] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showGrid, setShowGrid] = useState(false);

  // Derive managed miner auth PDA (the "miner pubkey")
  const [minerPubkey] = getManagedMinerAuthPda(managerAddress, BigInt(0));

  // Calculate totals
  const totalDeployed = miner?.deployed.reduce((sum, val) => sum + val, BigInt(0)) || BigInt(0);
  const totalClaimableOre = (miner?.rewardsOre || BigInt(0)) + (miner?.refinedOre || BigInt(0));
  
  // Check if checkpoint is needed (behind by 3+ rounds)
  const roundsBehind = currentBoardRoundId && miner 
    ? Number(currentBoardRoundId - miner.checkpointId)
    : 0;
  const needsCheckpoint = miner && currentBoardRoundId && 
    miner.checkpointId < miner.roundId && 
    roundsBehind >= 3;

  // Total withdrawable (SOL rewards + autodeploy balance)
  const totalWithdrawable = (miner?.rewardsSol || BigInt(0)) + (deployer?.autodeployBalance || BigInt(0));

  // Check if balance is too low for deployments
  // First deploy needs more (miner account creation), subsequent deploys need less
  const minRequired = miner ? BigInt(MIN_AUTODEPLOY_BALANCE) : BigInt(MIN_AUTODEPLOY_BALANCE_FIRST);
  const balanceTooLow = deployer && deployer.autodeployBalance < minRequired;

  const handleDeposit = async () => {
    if (!depositAmount) return;
    try {
      setLoading("deposit");
      setError(null);
      await onDeposit(parseSolToLamports(depositAmount));
      setShowDeposit(false);
      setDepositAmount("");
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(null);
    }
  };

  const handleWithdraw = async () => {
    if (!deployer) return;
    try {
      setLoading("withdraw");
      setError(null);
      await onWithdraw(miner?.rewardsSol || BigInt(0), deployer.autodeployBalance);
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(null);
    }
  };

  const handleCheckpoint = async () => {
    if (!miner) return;
    try {
      setLoading("checkpoint");
      setError(null);
      await onCheckpoint(miner.roundId);
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(null);
    }
  };

  const handleClaimOre = async () => {
    try {
      setLoading("claimOre");
      setError(null);
      await onClaimOre();
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(null);
    }
  };

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-4">
      {/* Header */}
      <div className="flex items-center justify-between mb-4">
        <div>
          <p className="text-xs text-zinc-500">AutoMiner</p>
          <CopyablePubkey pubkey={minerPubkey} />
        </div>
        {deployer && (
          <div className="text-right">
            <p className="text-xs text-zinc-500">Balance</p>
            <p className={`text-lg font-bold ${balanceTooLow ? 'text-red-400' : 'text-yellow-400'}`}>
              {formatSol(deployer.autodeployBalance)} SOL
            </p>
          </div>
        )}
      </div>

      {/* Low Balance Warning */}
      {balanceTooLow && (
        <div className="bg-red-900/30 border border-red-800 rounded p-2 mb-4 text-xs text-red-300">
          ⚠️ Balance too low for deployments. Minimum required: {formatSol(minRequired)} SOL
        </div>
      )}

      {/* Miner Stats */}
      {miner && (
        <div className="space-y-2 text-sm mb-4">
          <div>
            <p className="text-zinc-500 text-xs">Last Deployed</p>
            <p className="font-mono">Round {miner.roundId.toString()}</p>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <p className="text-zinc-500 text-xs">Claimable SOL</p>
              <p className={miner.rewardsSol > BigInt(0) ? "text-yellow-400" : "text-zinc-500"}>
                {formatSol(miner.rewardsSol)} SOL
              </p>
            </div>
            <div>
              <p className="text-zinc-500 text-xs">Claimable ORE</p>
              <p className={totalClaimableOre > BigInt(0) ? "text-orange-400" : "text-zinc-500"}>
                {formatOre(totalClaimableOre)} ORE
              </p>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <p className="text-zinc-500 text-xs">Total Deployed This Round</p>
              <p className="text-green-400">{formatSol(totalDeployed)} SOL</p>
            </div>
            <div>
              <p className="text-zinc-500 text-xs">Refined ORE</p>
              <p className={miner.refinedOre > BigInt(0) ? "text-purple-400" : "text-zinc-500"}>
                {formatOre(miner.refinedOre)}
              </p>
            </div>
          </div>

          {/* Deployment Grid Toggle */}
          <div className="pt-2 border-t border-zinc-800">
            <button
              onClick={() => setShowGrid(!showGrid)}
              className="text-xs text-zinc-400 hover:text-zinc-300"
            >
              {showGrid ? "▼ Hide" : "▶ Show"} Deployment Grid
            </button>
            {showGrid && (
              <div className="mt-2 grid grid-cols-5 gap-1">
                {miner.deployed.map((amount, i) => (
                  <div
                    key={i}
                    className={`text-xs text-center py-1 rounded ${
                      amount > BigInt(0) ? 'bg-green-900/50 text-green-400' : 'bg-zinc-800 text-zinc-600'
                    }`}
                    title={`Square ${i}: ${formatSol(amount)} SOL`}
                  >
                    {amount > BigInt(0) ? formatSol(amount, 4) : '-'}
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      )}

      {/* Deployer Info */}
      {deployer && (
        <div className="text-xs text-zinc-500 mb-4 p-2 bg-zinc-800/50 rounded">
          <div className="flex justify-between">
            <span>Deploy Authority:</span>
            <CopyablePubkey pubkey={deployer.deployAuthority} />
          </div>
          <div className="flex justify-between mt-1">
            <span>Fee:</span>
            <span>{formatFee(deployer.bpsFee, deployer.flatFee)}</span>
          </div>
        </div>
      )}

      {/* Error */}
      {error && (
        <div className="bg-red-900/50 border border-red-700 rounded p-2 mb-4 text-sm text-red-300">
          {error}
        </div>
      )}

      {/* Action Buttons */}
      <div className="flex flex-wrap gap-2">
        <button
          onClick={() => setShowDeposit(true)}
          className="flex-1 px-3 py-2 bg-green-600 hover:bg-green-500 rounded text-sm font-medium"
          disabled={loading !== null}
        >
          Deposit
        </button>
        <button
          onClick={handleWithdraw}
          disabled={totalWithdrawable === BigInt(0) || loading !== null}
          className={`flex-1 px-3 py-2 rounded text-sm font-medium ${
            totalWithdrawable > BigInt(0)
              ? 'bg-orange-600 hover:bg-orange-500'
              : 'bg-zinc-700 text-zinc-500 cursor-not-allowed'
          }`}
        >
          {loading === "withdraw" ? "..." : "Withdraw All"}
        </button>
        {needsCheckpoint && (
          <button
            onClick={handleCheckpoint}
            disabled={loading !== null}
            className="flex-1 px-3 py-2 bg-yellow-600 hover:bg-yellow-500 rounded text-sm font-medium"
          >
            {loading === "checkpoint" ? "..." : `Checkpoint (${roundsBehind} behind)`}
          </button>
        )}
        {totalClaimableOre > BigInt(0) && (
          <button
            onClick={handleClaimOre}
            disabled={loading !== null}
            className="flex-1 px-3 py-2 bg-purple-600 hover:bg-purple-500 rounded text-sm font-medium"
          >
            {loading === "claimOre" ? "..." : "Claim ORE"}
          </button>
        )}
      </div>

      {/* Deposit Modal */}
      {showDeposit && (
        <div className="fixed inset-0 bg-black/70 flex items-center justify-center z-50">
          <div className="bg-zinc-900 border border-zinc-700 rounded-lg p-6 max-w-sm w-full mx-4">
            <h3 className="text-lg font-semibold mb-4">Deposit SOL</h3>
            <div className="space-y-4">
              <div>
                <label className="block text-sm text-zinc-400 mb-1">Amount (SOL)</label>
                <input
                  type="number"
                  value={depositAmount}
                  onChange={(e) => setDepositAmount(e.target.value)}
                  placeholder="0.1"
                  min="0"
                  step="0.01"
                  className="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded text-sm"
                />
              </div>
              <div className="flex gap-2">
                <button
                  onClick={() => setShowDeposit(false)}
                  className="flex-1 px-4 py-2 bg-zinc-700 hover:bg-zinc-600 rounded"
                  disabled={loading !== null}
                >
                  Cancel
                </button>
                <button
                  onClick={handleDeposit}
                  className="flex-1 px-4 py-2 bg-green-600 hover:bg-green-500 rounded"
                  disabled={loading !== null}
                >
                  {loading === "deposit" ? "..." : "Deposit"}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

