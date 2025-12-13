"use client";

import { useWallet } from "@solana/wallet-adapter-react";
import { Keypair, PublicKey } from "@solana/web3.js";
import { Header } from "@/components/Header";
import { ManagerCard } from "@/components/ManagerCard";
import { CreateManagerForm } from "@/components/CreateManagerForm";
import { useEvore } from "@/hooks/useEvore";

export default function ManagePage() {
  const { publicKey, connected } = useWallet();
  const {
    managers,
    deployers,
    miners,
    board,
    loading,
    createManager,
    createDeployer,
    updateDeployer,
    depositAutodeployBalance,
    withdrawAutodeployBalance,
    checkpoint,
    claimSol,
    claimOre,
  } = useEvore();

  // Find deployer for a manager
  const getDeployerForManager = (managerAddress: PublicKey) => {
    return deployers.find(
      (d) => d.data.managerKey.toBase58() === managerAddress.toBase58()
    );
  };

  // Find miner for a manager
  const getMinerForManager = (managerAddress: PublicKey) => {
    return miners.get(managerAddress.toBase58());
  };

  const handleCreateManager = async (keypair: Keypair): Promise<string> => {
    // Pass the full keypair so it can sign the transaction
    return await createManager(keypair);
  };

  return (
    <div className="min-h-screen bg-zinc-950">
      <Header />

      <main className="max-w-6xl mx-auto px-4 py-8">
        <div className="mb-6">
          <h1 className="text-2xl font-bold">Advanced Management</h1>
          <p className="text-zinc-400 text-sm">Full control over managers, deployers, and miners</p>
        </div>

        {!connected ? (
          <div className="text-center py-20">
            <h2 className="text-2xl font-bold mb-4">Welcome to Evore</h2>
            <p className="text-zinc-400 mb-8">
              Connect your wallet to manage autodeploys
            </p>
            <div className="inline-block">
              <p className="text-sm text-zinc-500">
                Click the button in the header to connect
              </p>
            </div>
          </div>
        ) : (
          <div className="space-y-8">
            {/* Stats Overview */}
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-4">
                <p className="text-sm text-zinc-400">Manager Accounts</p>
                <p className="text-2xl font-bold">{managers.length}</p>
              </div>
              <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-4">
                <p className="text-sm text-zinc-400">Active Deployers</p>
                <p className="text-2xl font-bold">{deployers.length}</p>
              </div>
              <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-4">
                <p className="text-sm text-zinc-400">Total Autodeploy Balance</p>
                <p className="text-2xl font-bold text-yellow-400">
                  {(
                    Number(
                      deployers.reduce((sum, d) => sum + d.autodeployBalance, BigInt(0))
                    ) / 1_000_000_000
                  ).toFixed(4)}{" "}
                  SOL
                </p>
              </div>
            </div>

            {/* Loading State */}
            {loading && (
              <div className="text-center py-8">
                <p className="text-zinc-400">Loading...</p>
              </div>
            )}

            {/* Manager Cards */}
            <div className="space-y-4">
              <h2 className="text-xl font-semibold">Your Managers</h2>
              
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
                {managers.map((manager) => {
                  const deployer = getDeployerForManager(manager.address);
                  const miner = getMinerForManager(manager.address);
                  
                  return (
                    <ManagerCard
                      key={manager.address.toBase58()}
                      managerAddress={manager.address}
                      deployer={
                        deployer
                          ? {
                              address: deployer.address,
                              deployAuthority: deployer.data.deployAuthority,
                              bpsFee: deployer.data.bpsFee,
                              flatFee: deployer.data.flatFee,
                              autodeployBalance: deployer.autodeployBalance,
                            }
                          : undefined
                      }
                      miner={miner}
                      currentBoardRoundId={board?.roundId}
                      onCreateDeployer={(deployAuthority, bpsFee, flatFee) =>
                        createDeployer(manager.address, deployAuthority, bpsFee, flatFee)
                      }
                      onUpdateDeployer={(newDeployAuthority, newBpsFee, newFlatFee) =>
                        updateDeployer(manager.address, newDeployAuthority, newBpsFee, newFlatFee)
                      }
                      onDeposit={(amount) =>
                        depositAutodeployBalance(manager.address, amount)
                      }
                      onWithdraw={(amount) =>
                        withdrawAutodeployBalance(manager.address, amount)
                      }
                      onCheckpoint={(roundId) =>
                        checkpoint(manager.address, roundId)
                      }
                      onClaimSol={() =>
                        claimSol(manager.address)
                      }
                      onClaimOre={() =>
                        claimOre(manager.address)
                      }
                    />
                  );
                })}

                {/* Create New Manager */}
                <CreateManagerForm onCreateManager={handleCreateManager} />
              </div>
            </div>

            {/* Info Section */}
            <div className="bg-zinc-900/50 border border-zinc-800 rounded-lg p-6">
              <h3 className="text-lg font-semibold mb-4">How Autodeploy Works</h3>
              <div className="grid grid-cols-1 md:grid-cols-3 gap-6 text-sm">
                <div>
                  <h4 className="font-medium text-purple-400 mb-2">1. Create Manager</h4>
                  <p className="text-zinc-400">
                    A manager account holds your authority over managed miners. 
                    Create one to get started.
                  </p>
                </div>
                <div>
                  <h4 className="font-medium text-purple-400 mb-2">2. Set Up Deployer</h4>
                  <p className="text-zinc-400">
                    Configure a deployer with a deploy authority (can be a crank service) 
                    and fee percentage.
                  </p>
                </div>
                <div>
                  <h4 className="font-medium text-purple-400 mb-2">3. Fund & Deploy</h4>
                  <p className="text-zinc-400">
                    Deposit SOL to your autodeploy balance. The crank will automatically 
                    deploy when configured.
                  </p>
                </div>
              </div>
            </div>
          </div>
        )}
      </main>
    </div>
  );
}

