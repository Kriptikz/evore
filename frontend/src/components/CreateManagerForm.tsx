"use client";

import { useState } from "react";
import { Keypair } from "@solana/web3.js";

interface CreateManagerFormProps {
  onCreateManager: (keypair: Keypair) => Promise<string>;
}

export function CreateManagerForm({ onCreateManager }: CreateManagerFormProps) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showConfirm, setShowConfirm] = useState(false);
  const [keypair, setKeypair] = useState<Keypair | null>(null);

  const handleCreate = async () => {
    // Generate a new keypair for the manager account
    const newKeypair = Keypair.generate();
    setKeypair(newKeypair);
    setShowConfirm(true);
  };

  const handleConfirm = async () => {
    if (!keypair) return;

    try {
      setLoading(true);
      setError(null);
      await onCreateManager(keypair);
      setShowConfirm(false);
      setKeypair(null);
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="bg-zinc-900 border border-zinc-800 border-dashed rounded-lg p-6">
      <h3 className="text-lg font-semibold mb-2">Create New Manager</h3>
      <p className="text-sm text-zinc-400 mb-4">
        Create a new manager account to start using autodeploys.
      </p>

      {error && (
        <div className="bg-red-900/50 border border-red-700 rounded p-2 mb-4 text-sm text-red-300">
          {error}
        </div>
      )}

      <button
        onClick={handleCreate}
        className="w-full px-4 py-3 bg-purple-600 hover:bg-purple-500 rounded-lg font-medium"
        disabled={loading}
      >
        + Create Manager Account
      </button>

      {/* Confirmation Modal */}
      {showConfirm && keypair && (
        <div className="fixed inset-0 bg-black/70 flex items-center justify-center z-50">
          <div className="bg-zinc-900 border border-zinc-700 rounded-lg p-6 max-w-md w-full mx-4">
            <h3 className="text-lg font-semibold mb-4">Confirm Manager Creation</h3>
            
            <div className="bg-zinc-800 rounded p-3 mb-4">
              <p className="text-sm text-zinc-400 mb-1">New Manager Address:</p>
              <p className="font-mono text-sm break-all">{keypair.publicKey.toBase58()}</p>
            </div>

            <p className="text-sm text-zinc-400 mb-4">
              This will create a new manager account owned by your wallet. 
              The transaction will require SOL for rent and transaction fees.
            </p>

            <div className="flex gap-2">
              <button
                onClick={() => {
                  setShowConfirm(false);
                  setKeypair(null);
                }}
                className="flex-1 px-4 py-2 bg-zinc-700 hover:bg-zinc-600 rounded"
                disabled={loading}
              >
                Cancel
              </button>
              <button
                onClick={handleConfirm}
                className="flex-1 px-4 py-2 bg-purple-600 hover:bg-purple-500 rounded"
                disabled={loading}
              >
                {loading ? "Creating..." : "Create Manager"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
