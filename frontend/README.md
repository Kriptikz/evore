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
   # RPC URL (defaults to devnet)
   NEXT_PUBLIC_RPC_URL=https://api.devnet.solana.com
   
   # Stats server URL (optional, for future use)
   NEXT_PUBLIC_STATS_SERVER_URL=http://localhost:3001
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

## Stats Server Integration

The frontend is designed to work with a stats-server for optimized data fetching. When `NEXT_PUBLIC_STATS_SERVER_URL` is set, certain queries will use the stats API instead of direct RPC calls.

Currently, the frontend uses direct RPC for all queries. Stats server integration will be added in a future update.
