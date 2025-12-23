"use client";

import { useState } from "react";
import { PublicKey } from "@solana/web3.js";
import { useWallet } from "@solana/wallet-adapter-react";
import { shortenPubkey, formatSol, formatOre, formatFee, parseSolToLamports, parsePercentToBps } from "@/lib/accounts";
import { getManagedMinerAuthPda } from "@/lib/pda";

interface MinerData {
  address: PublicKey;
  authority: PublicKey;
  roundId: bigint;
  checkpointId: bigint;
  deployed: bigint[];
  rewardsSol: bigint;
  rewardsOre: bigint;
}

interface ManagerCardProps {
  managerAddress: PublicKey;
  deployer?: {
    address: PublicKey;
    deployAuthority: PublicKey;
    bpsFee: bigint;  // Percentage fee in basis points (1000 = 10%)
    flatFee: bigint; // Flat fee in lamports (added on top of bpsFee)
    maxPerRound: bigint; // Maximum lamports to deploy per round (0 = unlimited)
    autodeployBalance: bigint;
    authPdaAddress: PublicKey;  // The managed_miner_auth PDA where funds are held
  };
  miner?: MinerData;
  currentBoardRoundId?: bigint;
  isSelected?: boolean;
  onToggleSelect?: () => void;
  onCreateDeployer: (deployAuthority: PublicKey, bpsFee: bigint, flatFee: bigint, maxPerRound: bigint) => Promise<string>;
  onUpdateDeployer: (newDeployAuthority: PublicKey, newBpsFee: bigint, newFlatFee: bigint, newMaxPerRound: bigint) => Promise<string>;
  onDeposit: (authId: bigint, amount: bigint) => Promise<string>;
  onWithdraw: (authId: bigint, amount: bigint) => Promise<string>;
  onCheckpoint: (roundId: bigint) => Promise<string>;
  onClaimSol: () => Promise<string>;
  onClaimOre: () => Promise<string>;
  onTransfer: (newAuthority: PublicKey) => Promise<string>;
}

// Copy text to clipboard and show feedback
async function copyToClipboard(text: string, setTooltip: (msg: string | null) => void) {
  try {
    await navigator.clipboard.writeText(text);
    setTooltip("Copied!");
    setTimeout(() => setTooltip(null), 1500);
  } catch {
    setTooltip("Failed to copy");
    setTimeout(() => setTooltip(null), 1500);
  }
}

// Clickable pubkey component that copies on click
function CopyablePubkey({ pubkey, label }: { pubkey: PublicKey; label?: string }) {
  const [tooltip, setTooltip] = useState<string | null>(null);
  
  return (
    <div className="flex justify-between items-center">
      {label && <span className="text-zinc-400">{label}</span>}
      <button
        onClick={() => copyToClipboard(pubkey.toBase58(), setTooltip)}
        className="font-mono text-purple-400 hover:text-purple-300 hover:bg-zinc-800 px-1 rounded transition-colors relative"
        title="Click to copy"
      >
        {shortenPubkey(pubkey)}
        {tooltip && (
          <span className="absolute -top-6 left-1/2 -translate-x-1/2 text-xs bg-zinc-700 px-2 py-1 rounded whitespace-nowrap">
            {tooltip}
          </span>
        )}
      </button>
    </div>
  );
}

export function ManagerCard({
  managerAddress,
  deployer,
  miner,
  currentBoardRoundId,
  isSelected = false,
  onToggleSelect,
  onCreateDeployer,
  onUpdateDeployer,
  onDeposit,
  onWithdraw,
  onCheckpoint,
  onClaimSol,
  onClaimOre,
  onTransfer,
}: ManagerCardProps) {
  const { publicKey } = useWallet();
  const [showCreateDeployer, setShowCreateDeployer] = useState(false);
  const [showUpdateDeployer, setShowUpdateDeployer] = useState(false);
  const [showDeposit, setShowDeposit] = useState(false);
  const [showWithdraw, setShowWithdraw] = useState(false);
  const [showTransfer, setShowTransfer] = useState(false);
  const [loading, setLoading] = useState(false);
  const [claimLoading, setClaimLoading] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Form states
  const [deployAuthority, setDeployAuthority] = useState("");
  const [bpsFeeAmount, setBpsFeeAmount] = useState("5"); // Default 5%
  const [flatFeeAmount, setFlatFeeAmount] = useState("0"); // Default 0 lamports
  const [maxPerRoundAmount, setMaxPerRoundAmount] = useState("1000000000"); // Default 1 SOL in lamports
  const [depositAmount, setDepositAmount] = useState("");
  const [withdrawAmount, setWithdrawAmount] = useState("");
  const [transferAddress, setTransferAddress] = useState("");

  // Derive managed miner auth PDA for auth_id 0
  const [managedMinerAuthPda] = getManagedMinerAuthPda(managerAddress, BigInt(0));

  // Check if miner needs checkpoint:
  // - miner.checkpointId < miner.roundId (hasn't checkpointed the last played round)
  // - AND the board has moved to a new round (miner.roundId < currentBoardRoundId)
  // This means we can only checkpoint once the round we played in has ended
  const needsCheckpoint = miner && currentBoardRoundId && 
    miner.checkpointId < miner.roundId && 
    miner.roundId < currentBoardRoundId;

  const handleCreateDeployer = async () => {
    if (!deployAuthority) {
      setError("Deploy authority is required");
      return;
    }

    try {
      setLoading(true);
      setError(null);
      const authority = new PublicKey(deployAuthority);
      const bpsFee = parsePercentToBps(bpsFeeAmount);
      const flatFee = BigInt(Math.floor(parseFloat(flatFeeAmount) || 0));
      const maxPerRound = BigInt(Math.floor(parseFloat(maxPerRoundAmount) || 0));
      await onCreateDeployer(authority, bpsFee, flatFee, maxPerRound);
      setShowCreateDeployer(false);
      setDeployAuthority("");
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  };

  const handleUpdateDeployer = async () => {
    if (!deployAuthority) {
      setError("Deploy authority is required");
      return;
    }

    try {
      setLoading(true);
      setError(null);
      const authority = new PublicKey(deployAuthority);
      const bpsFee = parsePercentToBps(bpsFeeAmount);
      const flatFee = BigInt(Math.floor(parseFloat(flatFeeAmount) || 0));
      const maxPerRound = BigInt(Math.floor(parseFloat(maxPerRoundAmount) || 0));
      await onUpdateDeployer(authority, bpsFee, flatFee, maxPerRound);
      setShowUpdateDeployer(false);
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  };

  const handleDeposit = async () => {
    if (!depositAmount) {
      setError("Amount is required");
      return;
    }

    try {
      setLoading(true);
      setError(null);
      const lamports = parseSolToLamports(depositAmount);
      // Use auth_id 0 for legacy deployers
      await onDeposit(BigInt(0), lamports);
      setShowDeposit(false);
      setDepositAmount("");
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  };

  const handleWithdraw = async () => {
    if (!withdrawAmount) {
      setError("Amount is required");
      return;
    }

    try {
      setLoading(true);
      setError(null);
      const lamports = parseSolToLamports(withdrawAmount);
      // Use auth_id 0 for legacy deployers
      await onWithdraw(BigInt(0), lamports);
      setShowWithdraw(false);
      setWithdrawAmount("");
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  };

  const handleCheckpoint = async () => {
    if (!miner) return;
    
    try {
      setClaimLoading("checkpoint");
      setError(null);
      // Checkpoint the last played round
      await onCheckpoint(miner.roundId);
    } catch (err: any) {
      setError(err.message);
    } finally {
      setClaimLoading(null);
    }
  };

  const handleClaimSol = async () => {
    try {
      setClaimLoading("sol");
      setError(null);
      await onClaimSol();
    } catch (err: any) {
      setError(err.message);
    } finally {
      setClaimLoading(null);
    }
  };

  const handleClaimOre = async () => {
    try {
      setClaimLoading("ore");
      setError(null);
      await onClaimOre();
    } catch (err: any) {
      setError(err.message);
    } finally {
      setClaimLoading(null);
    }
  };

  const handleTransfer = async () => {
    if (!transferAddress) {
      setError("New authority address is required");
      return;
    }

    try {
      setLoading(true);
      setError(null);
      const newAuthority = new PublicKey(transferAddress);
      await onTransfer(newAuthority);
      setShowTransfer(false);
      setTransferAddress("");
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  };

  // Calculate total deployed amount
  const totalDeployed = miner?.deployed.reduce((sum, val) => sum + val, BigInt(0)) || BigInt(0);

  return (
    <div className={`bg-zinc-900 border rounded-lg p-4 transition-colors ${
      isSelected ? 'border-purple-500 bg-purple-900/10' : 'border-zinc-800'
    }`}>
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-3">
          {onToggleSelect && (
            <input
              type="checkbox"
              checked={isSelected}
              onChange={onToggleSelect}
              className="w-4 h-4 rounded border-zinc-600 bg-zinc-800 text-purple-500 focus:ring-purple-500 focus:ring-offset-zinc-900 cursor-pointer"
            />
          )}
          <h3 className="text-lg font-semibold">Manager Account</h3>
        </div>
        <div className="flex items-center gap-2">
          <CopyablePubkey pubkey={managerAddress} />
          <button
            onClick={() => setShowTransfer(true)}
            className="px-2 py-1 text-xs bg-zinc-700 hover:bg-zinc-600 rounded"
            title="Transfer manager authority"
          >
            Transfer
          </button>
        </div>
      </div>

      <div className="space-y-2 text-sm mb-4">
        <CopyablePubkey pubkey={managedMinerAuthPda} label="Miner Authority:" />
      </div>

      {/* Miner Stats */}
      {miner && (
        <div className="bg-zinc-800/50 rounded-lg p-3 mb-4">
          <h4 className="font-medium mb-2 text-blue-400">ORE Miner Stats</h4>
          <div className="space-y-1 text-sm">
            <CopyablePubkey pubkey={miner.address} label="Miner Address:" />
            <div className="flex justify-between">
              <span className="text-zinc-400">Last Played Round:</span>
              <span className="font-mono">{miner.roundId.toString()}</span>
            </div>
            {currentBoardRoundId && (
              <div className="flex justify-between">
                <span className="text-zinc-400">Current Board Round:</span>
                <span className="font-mono">{currentBoardRoundId.toString()}</span>
              </div>
            )}
            <div className="flex justify-between">
              <span className="text-zinc-400">Checkpoint ID:</span>
              <span className={`font-mono ${needsCheckpoint ? 'text-orange-400' : ''}`}>
                {miner.checkpointId.toString()}
                {needsCheckpoint && <span className="ml-1 text-xs">(needs checkpoint)</span>}
              </span>
            </div>
            <div className="flex justify-between">
              <span className="text-zinc-400">Claimable SOL:</span>
              <span className={miner.rewardsSol > BigInt(0) ? "text-yellow-400" : "text-zinc-500"}>
                {formatSol(miner.rewardsSol)} SOL
              </span>
            </div>
            <div className="flex justify-between">
              <span className="text-zinc-400">Claimable ORE:</span>
              <span className={miner.rewardsOre > BigInt(0) ? "text-orange-400" : "text-zinc-500"}>
                {formatOre(miner.rewardsOre)} ORE
              </span>
            </div>
            <div className="flex justify-between">
              <span className="text-zinc-400">Total Deployed:</span>
              <span className="text-green-400">{formatSol(totalDeployed)} SOL</span>
            </div>
            
            {/* Action buttons */}
            <div className="flex gap-2 mt-3 pt-2 border-t border-zinc-700">
              <button
                onClick={handleCheckpoint}
                disabled={!needsCheckpoint || claimLoading !== null}
                className={`flex-1 px-2 py-1.5 text-xs rounded ${
                  needsCheckpoint 
                    ? 'bg-orange-600 hover:bg-orange-500 text-white' 
                    : 'bg-zinc-700 text-zinc-500 cursor-not-allowed'
                }`}
              >
                {claimLoading === "checkpoint" ? "..." : "Checkpoint"}
              </button>
              <button
                onClick={handleClaimSol}
                disabled={!miner.rewardsSol || miner.rewardsSol <= BigInt(0) || claimLoading !== null}
                className={`flex-1 px-2 py-1.5 text-xs rounded ${
                  miner.rewardsSol && miner.rewardsSol > BigInt(0)
                    ? 'bg-yellow-600 hover:bg-yellow-500 text-white' 
                    : 'bg-zinc-700 text-zinc-500 cursor-not-allowed'
                }`}
              >
                {claimLoading === "sol" ? "..." : "Claim SOL"}
              </button>
              <button
                onClick={handleClaimOre}
                disabled={!miner.rewardsOre || miner.rewardsOre <= BigInt(0) || claimLoading !== null}
                className={`flex-1 px-2 py-1.5 text-xs rounded ${
                  miner.rewardsOre && miner.rewardsOre > BigInt(0)
                    ? 'bg-purple-600 hover:bg-purple-500 text-white' 
                    : 'bg-zinc-700 text-zinc-500 cursor-not-allowed'
                }`}
              >
                {claimLoading === "ore" ? "..." : "Claim ORE"}
              </button>
            </div>
            
            {/* Deployment grid */}
            <div className="mt-2 pt-2 border-t border-zinc-700">
              <div className="text-xs text-zinc-500 mb-1">
                Deployments in Round {miner.roundId.toString()} (25 squares):
              </div>
              <div className="grid grid-cols-5 gap-1">
                {miner.deployed.map((amount, i) => (
                  <div
                    key={i}
                    className={`text-xs text-center py-0.5 rounded ${
                      amount > BigInt(0) ? 'bg-green-900/50 text-green-400' : 'bg-zinc-800 text-zinc-600'
                    }`}
                    title={`Square ${i}: ${formatSol(amount)} SOL`}
                  >
                    {amount > BigInt(0) ? formatSol(amount, 4) : '-'}
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>
      )}

      {deployer ? (
        <div className="bg-zinc-800/50 rounded-lg p-3 mb-4">
          <h4 className="font-medium mb-2 text-green-400">✓ Deployer Active</h4>
          <div className="space-y-1 text-sm">
            <CopyablePubkey pubkey={deployer.deployAuthority} label="Deploy Authority:" />
            <CopyablePubkey pubkey={deployer.authPdaAddress} label="Auth PDA (deposit here):" />
            <div className="flex justify-between">
              <span className="text-zinc-400">Fee:</span>
              <span>{formatFee(deployer.bpsFee, deployer.flatFee)}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-zinc-400">Max Per Round:</span>
              <span className="text-blue-400">
                {!deployer.maxPerRound || deployer.maxPerRound <= BigInt(0) ? 'Unlimited' : `${formatSol(deployer.maxPerRound)} SOL`}
              </span>
            </div>
            <div className="flex justify-between">
              <span className="text-zinc-400">Autodeploy Balance:</span>
              <span className="text-yellow-400">{formatSol(deployer.autodeployBalance)} SOL</span>
            </div>
          </div>

          <div className="flex gap-2 mt-3">
            <button
              onClick={() => {
                setDeployAuthority(deployer.deployAuthority.toBase58());
                setBpsFeeAmount((Number(deployer.bpsFee) / 100).toString());
                setFlatFeeAmount(deployer.flatFee.toString());
                setMaxPerRoundAmount(deployer.maxPerRound.toString());
                setShowUpdateDeployer(true);
              }}
              className="flex-1 px-3 py-1.5 text-sm bg-zinc-700 hover:bg-zinc-600 rounded"
            >
              Update
            </button>
            <button
              onClick={() => setShowDeposit(true)}
              className="flex-1 px-3 py-1.5 text-sm bg-green-600 hover:bg-green-500 rounded"
            >
              Deposit
            </button>
            <button
              onClick={() => setShowWithdraw(true)}
              className="flex-1 px-3 py-1.5 text-sm bg-orange-600 hover:bg-orange-500 rounded"
              disabled={!deployer.autodeployBalance || deployer.autodeployBalance <= BigInt(0)}
            >
              Withdraw
            </button>
          </div>
        </div>
      ) : (
        <div className="bg-zinc-800/50 rounded-lg p-3 mb-4">
          <h4 className="font-medium mb-2 text-zinc-400">No Deployer</h4>
          <p className="text-sm text-zinc-500 mb-3">
            Create a deployer to enable autodeploys for this manager.
          </p>
          <button
            onClick={() => {
              setDeployAuthority(publicKey?.toBase58() || "");
              setShowCreateDeployer(true);
            }}
            className="w-full px-3 py-2 bg-purple-600 hover:bg-purple-500 rounded font-medium"
          >
            Create Deployer
          </button>
        </div>
      )}

      {/* Error display */}
      {error && (
        <div className="bg-red-900/50 border border-red-700 rounded p-2 mb-4 text-sm text-red-300">
          {error}
        </div>
      )}

      {/* Create Deployer Modal */}
      {showCreateDeployer && (
        <div className="fixed inset-0 bg-black/70 flex items-center justify-center z-50">
          <div className="bg-zinc-900 border border-zinc-700 rounded-lg p-6 max-w-md w-full mx-4">
            <h3 className="text-lg font-semibold mb-4">Create Deployer</h3>
            <div className="space-y-4">
              <div>
                <label className="block text-sm text-zinc-400 mb-1">Deploy Authority</label>
                <input
                  type="text"
                  value={deployAuthority}
                  onChange={(e) => setDeployAuthority(e.target.value)}
                  placeholder="Public key..."
                  className="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded text-sm"
                />
              </div>
              <div>
                <label className="block text-sm text-zinc-400 mb-1">Percentage Fee (%)</label>
                <input
                  type="number"
                  value={bpsFeeAmount}
                  onChange={(e) => setBpsFeeAmount(e.target.value)}
                  placeholder="5"
                  min="0"
                  max="100"
                  step="0.01"
                  className="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded text-sm"
                />
                <p className="text-xs text-zinc-500 mt-1">
                  Enter percentage (e.g., 5 for 5%). Set to 0 to disable.
                </p>
              </div>
              <div>
                <label className="block text-sm text-zinc-400 mb-1">Flat Fee (lamports)</label>
                <input
                  type="number"
                  value={flatFeeAmount}
                  onChange={(e) => setFlatFeeAmount(e.target.value)}
                  placeholder="0"
                  min="0"
                  step="1"
                  className="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded text-sm"
                />
                <p className="text-xs text-zinc-500 mt-1">
                  Additional flat fee in lamports. Set to 0 to disable.
                </p>
              </div>
              <div>
                <label className="block text-sm text-zinc-400 mb-1">Max Per Round (lamports)</label>
                <input
                  type="number"
                  value={maxPerRoundAmount}
                  onChange={(e) => setMaxPerRoundAmount(e.target.value)}
                  placeholder="1000000000"
                  min="0"
                  step="1"
                  className="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded text-sm"
                />
                <p className="text-xs text-zinc-500 mt-1">
                  Maximum lamports to deploy per round. Set to 0 for unlimited. (1 SOL = 1,000,000,000 lamports)
                </p>
              </div>
              <div className="flex gap-2">
                <button
                  onClick={() => setShowCreateDeployer(false)}
                  className="flex-1 px-4 py-2 bg-zinc-700 hover:bg-zinc-600 rounded"
                  disabled={loading}
                >
                  Cancel
                </button>
                <button
                  onClick={handleCreateDeployer}
                  className="flex-1 px-4 py-2 bg-purple-600 hover:bg-purple-500 rounded"
                  disabled={loading}
                >
                  {loading ? "Creating..." : "Create"}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Update Deployer Modal */}
      {showUpdateDeployer && (
        <div className="fixed inset-0 bg-black/70 flex items-center justify-center z-50">
          <div className="bg-zinc-900 border border-zinc-700 rounded-lg p-6 max-w-md w-full mx-4">
            <h3 className="text-lg font-semibold mb-4">Update Deployer</h3>
            <div className="space-y-4">
              <div>
                <label className="block text-sm text-zinc-400 mb-1">New Deploy Authority</label>
                <input
                  type="text"
                  value={deployAuthority}
                  onChange={(e) => setDeployAuthority(e.target.value)}
                  placeholder="Public key..."
                  className="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded text-sm"
                />
              </div>
              <div>
                <label className="block text-sm text-zinc-400 mb-1">Percentage Fee (%)</label>
                <input
                  type="number"
                  value={bpsFeeAmount}
                  onChange={(e) => setBpsFeeAmount(e.target.value)}
                  placeholder="5"
                  min="0"
                  max="100"
                  step="0.01"
                  className="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded text-sm"
                />
                <p className="text-xs text-zinc-500 mt-1">
                  Enter percentage (e.g., 5 for 5%). Set to 0 to disable.
                </p>
              </div>
              <div>
                <label className="block text-sm text-zinc-400 mb-1">Flat Fee (lamports)</label>
                <input
                  type="number"
                  value={flatFeeAmount}
                  onChange={(e) => setFlatFeeAmount(e.target.value)}
                  placeholder="0"
                  min="0"
                  step="1"
                  className="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded text-sm"
                />
                <p className="text-xs text-zinc-500 mt-1">
                  Additional flat fee in lamports. Set to 0 to disable.
                </p>
              </div>
              <div>
                <label className="block text-sm text-zinc-400 mb-1">Max Per Round (lamports)</label>
                <input
                  type="number"
                  value={maxPerRoundAmount}
                  onChange={(e) => setMaxPerRoundAmount(e.target.value)}
                  placeholder="1000000000"
                  min="0"
                  step="1"
                  className="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded text-sm"
                />
                <p className="text-xs text-zinc-500 mt-1">
                  Maximum lamports to deploy per round. Set to 0 for unlimited. (1 SOL = 1,000,000,000 lamports)
                </p>
              </div>
              <div className="flex gap-2">
                <button
                  onClick={() => setShowUpdateDeployer(false)}
                  className="flex-1 px-4 py-2 bg-zinc-700 hover:bg-zinc-600 rounded"
                  disabled={loading}
                >
                  Cancel
                </button>
                <button
                  onClick={handleUpdateDeployer}
                  className="flex-1 px-4 py-2 bg-purple-600 hover:bg-purple-500 rounded"
                  disabled={loading}
                >
                  {loading ? "Updating..." : "Update"}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Deposit Modal */}
      {showDeposit && (
        <div className="fixed inset-0 bg-black/70 flex items-center justify-center z-50">
          <div className="bg-zinc-900 border border-zinc-700 rounded-lg p-6 max-w-md w-full mx-4">
            <h3 className="text-lg font-semibold mb-4">Deposit to Autodeploy Balance</h3>
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
                  disabled={loading}
                >
                  Cancel
                </button>
                <button
                  onClick={handleDeposit}
                  className="flex-1 px-4 py-2 bg-green-600 hover:bg-green-500 rounded"
                  disabled={loading}
                >
                  {loading ? "Depositing..." : "Deposit"}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Withdraw Modal */}
      {showWithdraw && (
        <div className="fixed inset-0 bg-black/70 flex items-center justify-center z-50">
          <div className="bg-zinc-900 border border-zinc-700 rounded-lg p-6 max-w-md w-full mx-4">
            <h3 className="text-lg font-semibold mb-4">Withdraw from Autodeploy Balance</h3>
            <div className="space-y-4">
              <div>
                <label className="block text-sm text-zinc-400 mb-1">
                  Amount (SOL) - Max: {deployer ? formatSol(deployer.autodeployBalance) : "0"}
                </label>
                <input
                  type="number"
                  value={withdrawAmount}
                  onChange={(e) => setWithdrawAmount(e.target.value)}
                  placeholder="0.1"
                  min="0"
                  step="0.01"
                  className="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded text-sm"
                />
                <button
                  onClick={() => setWithdrawAmount(formatSol(deployer?.autodeployBalance || BigInt(0), 9))}
                  className="mt-1 text-xs text-purple-400 hover:text-purple-300"
                >
                  Max
                </button>
              </div>
              <div className="flex gap-2">
                <button
                  onClick={() => setShowWithdraw(false)}
                  className="flex-1 px-4 py-2 bg-zinc-700 hover:bg-zinc-600 rounded"
                  disabled={loading}
                >
                  Cancel
                </button>
                <button
                  onClick={handleWithdraw}
                  className="flex-1 px-4 py-2 bg-orange-600 hover:bg-orange-500 rounded"
                  disabled={loading}
                >
                  {loading ? "Withdrawing..." : "Withdraw"}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Transfer Modal */}
      {showTransfer && (
        <div className="fixed inset-0 bg-black/70 flex items-center justify-center z-50">
          <div className="bg-zinc-900 border border-zinc-700 rounded-lg p-6 max-w-md w-full mx-4">
            <h3 className="text-lg font-semibold mb-2">Transfer Manager</h3>
            <p className="text-sm text-zinc-400 mb-4">
              ⚠️ This will transfer full control of this manager and all associated accounts (miner, deployer, etc.) to a new wallet. This action is irreversible.
            </p>
            <div className="space-y-4">
              <div>
                <label className="block text-sm text-zinc-400 mb-1">New Authority Address</label>
                <input
                  type="text"
                  value={transferAddress}
                  onChange={(e) => setTransferAddress(e.target.value)}
                  placeholder="Enter wallet address..."
                  className="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded text-sm font-mono"
                />
              </div>
              <div className="flex gap-2">
                <button
                  onClick={() => {
                    setShowTransfer(false);
                    setTransferAddress("");
                    setError(null);
                  }}
                  className="flex-1 px-4 py-2 bg-zinc-700 hover:bg-zinc-600 rounded"
                  disabled={loading}
                >
                  Cancel
                </button>
                <button
                  onClick={handleTransfer}
                  className="flex-1 px-4 py-2 bg-red-600 hover:bg-red-500 rounded"
                  disabled={loading || !transferAddress}
                >
                  {loading ? "Transferring..." : "Transfer"}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
