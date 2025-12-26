#!/bin/bash
#
# ClickHouse Migration Script
# Applies numbered SQL migrations in order, tracking applied versions
#
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Load environment if available
if [ -f /etc/ore-stats.env ]; then
    source /etc/ore-stats.env
fi

# Default connection settings
CH_HOST="${CLICKHOUSE_HOST:-127.0.0.1}"
CH_PORT="${CLICKHOUSE_PORT:-9000}"
CH_USER="${CLICKHOUSE_USER:-default}"
CH_PASS="${CLICKHOUSE_PASSWORD:-}"

# Build password arg
if [ -n "$CH_PASS" ]; then
    PASS_ARG="--password $CH_PASS"
else
    PASS_ARG=""
fi

echo "ClickHouse Migration Runner"
echo "==========================="
echo "Host: $CH_HOST:$CH_PORT"
echo "User: $CH_USER"
echo ""

# Create ore_stats database if needed (for migrations table)
clickhouse-client -h "$CH_HOST" --port "$CH_PORT" -u "$CH_USER" $PASS_ARG \
    -q "CREATE DATABASE IF NOT EXISTS ore_stats"

# Create migrations tracking table
clickhouse-client -h "$CH_HOST" --port "$CH_PORT" -u "$CH_USER" $PASS_ARG <<EOF
CREATE TABLE IF NOT EXISTS ore_stats._migrations (
    version UInt32,
    name String,
    applied_at DateTime DEFAULT now()
) ENGINE = MergeTree()
ORDER BY version;
EOF

echo "Checking migrations..."
echo ""

APPLIED=0
SKIPPED=0

# Apply each migration in order
for file in "$SCRIPT_DIR"/*.sql; do
    [ -f "$file" ] || continue
    
    filename=$(basename "$file")
    
    # Extract version number from filename (e.g., 001_create_databases.sql -> 1)
    version=$(echo "$filename" | grep -oE '^[0-9]+' | sed 's/^0*//')
    
    if [ -z "$version" ]; then
        echo "⚠ Skipping $filename (no version number)"
        continue
    fi
    
    # Check if already applied
    applied=$(clickhouse-client -h "$CH_HOST" --port "$CH_PORT" -u "$CH_USER" $PASS_ARG \
        -q "SELECT count() FROM ore_stats._migrations WHERE version = $version" 2>/dev/null || echo "0")
    
    if [ "$applied" -eq "0" ]; then
        echo "→ Applying $filename..."
        
        # Apply migration
        clickhouse-client -h "$CH_HOST" --port "$CH_PORT" -u "$CH_USER" $PASS_ARG \
            --multiquery < "$file"
        
        # Record migration
        clickhouse-client -h "$CH_HOST" --port "$CH_PORT" -u "$CH_USER" $PASS_ARG \
            -q "INSERT INTO ore_stats._migrations (version, name) VALUES ($version, '$filename')"
        
        echo "✓ Applied $filename"
        APPLIED=$((APPLIED + 1))
    else
        echo "○ Skipping $filename (already applied)"
        SKIPPED=$((SKIPPED + 1))
    fi
done

echo ""
echo "==========================="
echo "Applied: $APPLIED, Skipped: $SKIPPED"
echo "Done!"

