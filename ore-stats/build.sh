#!/bin/bash
#
# Build ore-stats for release and deploy to ~/programs/ore-stats/
#
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_DIR="$(dirname "$SCRIPT_DIR")"
DEPLOY_DIR="$HOME/programs/ore-stats"

echo "Building ore-stats in release mode..."
cd "$WORKSPACE_DIR"
cargo build --release -p ore-stats

echo "Creating deploy directory..."
mkdir -p "$DEPLOY_DIR"
mkdir -p "$DEPLOY_DIR/backups"

# Backup existing binary if it exists
if [ -f "$DEPLOY_DIR/ore-stats" ]; then
    TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
    BACKUP_NAME="ore-stats_$TIMESTAMP"
    echo "Backing up existing binary to backups/$BACKUP_NAME..."
    mv "$DEPLOY_DIR/ore-stats" "$DEPLOY_DIR/backups/$BACKUP_NAME"
    echo "✓ Backup created: $DEPLOY_DIR/backups/$BACKUP_NAME"
fi

echo "Copying new binary..."
cp "$WORKSPACE_DIR/target/release/ore-stats" "$DEPLOY_DIR/"

# Copy example env if it doesn't exist
if [ ! -f "$DEPLOY_DIR/.env" ]; then
    echo "Creating example .env file..."
    cat > "$DEPLOY_DIR/.env.example" << 'EOF'
# ore-stats Environment Configuration
# Copy this to .env and fill in your values

# Server
PORT=3000

# Solana RPC (Helius recommended)
RPC_URL=mainnet.helius-rpc.com/?api-key=YOUR_API_KEY

# ClickHouse
CLICKHOUSE_URL=http://127.0.0.1:8123
CLICKHOUSE_USER=default
CLICKHOUSE_PASSWORD=
CLICKHOUSE_DATABASE=ore_stats

# PostgreSQL
DATABASE_URL=postgres://ore_app:YOUR_PASSWORD@127.0.0.1:5432/ore_operations

# Logging
RUST_LOG=info,ore_stats=debug
EOF
    echo "Created $DEPLOY_DIR/.env.example"
    echo "⚠️  Copy .env.example to .env and configure it!"
else
    echo ".env already exists, skipping..."
fi

echo ""
echo "✅ Build complete!"
echo ""
echo "Deploy location: $DEPLOY_DIR"
echo "Binary: $DEPLOY_DIR/ore-stats"
echo ""
echo "To run:"
echo "  cd $DEPLOY_DIR"
echo "  ./ore-stats"
echo ""
echo "Or with systemd (recommended for production):"
echo "  sudo systemctl start ore-stats"
echo ""
echo "To rollback to a previous version:"
echo "  ls $DEPLOY_DIR/backups/  # list available backups"
echo "  cp $DEPLOY_DIR/backups/<backup_name> $DEPLOY_DIR/ore-stats"

