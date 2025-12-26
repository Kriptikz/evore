# Evore Frontend

Web interface for managing Evore autodeploy accounts.

## Features

- **Wallet Integration**: Connect with Phantom, Solflare, or other Solana wallets
- **Manager Accounts**: Create and view manager accounts
- **Deployer Configuration**: Set up deployers with custom deploy authorities and fees
- **Balance Management**: Deposit and withdraw from autodeploy balance

## Setup

1. Install dependencies:
   ```bash
   npm install
   ```

2. Set environment variables (create `.env.local`):
   ```bash
   # ORE Stats API URL (REQUIRED - primary data source for all reads)
   NEXT_PUBLIC_API_URL=https://your-ore-stats-domain.com
   
   # RPC URL (for wallet transactions only - use basic/free endpoint)
   NEXT_PUBLIC_RPC_URL=https://api.mainnet-beta.solana.com
   
   # Deployer settings for AutoMiner creation
   NEXT_PUBLIC_DEPLOYER_PUBKEY=your_deployer_pubkey
   NEXT_PUBLIC_DEPLOYER_BPS_FEE=500
   NEXT_PUBLIC_DEPLOYER_FLAT_FEE=715
   ```

3. Run development server:
   ```bash
   npm run dev
   ```

4. Open [http://localhost:3000](http://localhost:3000)

## Building for Production

```bash
npm run build
npm start
```

## Project Structure

```
src/
├── app/                  # Next.js app router pages
│   ├── layout.tsx        # Root layout with wallet provider
│   └── page.tsx          # Main dashboard page
├── components/           # React components
│   ├── Header.tsx        # Navigation header with wallet button
│   ├── ManagerCard.tsx   # Manager account card with deployer controls
│   ├── CreateManagerForm.tsx
│   └── WalletProvider.tsx
├── hooks/
│   └── useEvore.ts       # Main hook for Evore program interactions
└── lib/
    ├── constants.ts      # Program IDs, seeds, fees
    ├── pda.ts            # PDA derivation functions
    ├── instructions.ts   # Transaction instruction builders
    └── accounts.ts       # Account decoders and utilities
```

## Usage

### Creating a Manager

1. Connect your wallet
2. Click "Create Manager Account"
3. Confirm the transaction

### Setting Up a Deployer

1. On your manager card, click "Create Deployer"
2. Enter the deploy authority public key (can be a crank service)
3. Set the fee percentage (e.g., 5% = 500 bps)
4. Confirm the transaction

### Managing Autodeploy Balance

- **Deposit**: Add SOL to fund autodeploys
- **Withdraw**: Remove SOL from the autodeploy balance

## ORE Stats API Integration

The frontend uses the ore-stats API (`NEXT_PUBLIC_API_URL`) for **all read operations**:
- ORE accounts (Board, Round, Treasury, Miners)
- EVORE accounts (Managers, Deployers, Auth balances) [Phase 1b]
- SOL balances, ORE token balances
- Admin dashboard operations

The wallet RPC (`NEXT_PUBLIC_RPC_URL`) is **only used for transactions**:
- Getting latest blockhash
- Sending transactions
- Confirming transactions

This architecture reduces RPC costs and improves performance through server-side caching.

## Admin Dashboard

Access the admin dashboard at `/admin` to:
- View server metrics and uptime
- Monitor RPC usage and errors
- Manage IP blacklist
- View real-time performance data

Admin authentication uses a password set via `ADMIN_PASSWORD` environment variable on the ore-stats server.
