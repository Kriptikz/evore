"use client";

import { FC, ReactNode, useMemo } from "react";
import {
  ConnectionProvider,
  WalletProvider as SolanaWalletProvider,
} from "@solana/wallet-adapter-react";
import { WalletModalProvider } from "@solana/wallet-adapter-react-ui";

import "@solana/wallet-adapter-react-ui/styles.css";

interface Props {
  children: ReactNode;
}

/**
 * Wallet Provider with RPC connection for transactions.
 * 
 * IMPORTANT: This RPC is ONLY used for transaction operations:
 * - getLatestBlockhash (for tx construction)
 * - sendTransaction (for tx submission)
 * - confirmTransaction (for tx confirmation)
 * 
 * All READ operations (accounts, balances, etc.) should go through
 * the ore-stats API to avoid RPC rate limits and costs.
 * 
 * TODO (Phase 1b): Migrate all reads in useEvore.ts to ore-stats API
 */
export const WalletProvider: FC<Props> = ({ children }) => {
  // This RPC is required for wallet adapter transaction functionality
  // Use a basic/free endpoint - heavy reads go through ore-stats API
  const endpoint = process.env.NEXT_PUBLIC_RPC_URL || "https://api.mainnet-beta.solana.com";

  // Empty array - wallet-standard will auto-detect installed wallets (Phantom, Solflare, etc.)
  const wallets = useMemo(() => [], []);

  return (
    <ConnectionProvider endpoint={endpoint}>
      <SolanaWalletProvider wallets={wallets} autoConnect>
        <WalletModalProvider>{children}</WalletModalProvider>
      </SolanaWalletProvider>
    </ConnectionProvider>
  );
};
