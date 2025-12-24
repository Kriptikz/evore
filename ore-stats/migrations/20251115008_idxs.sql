CREATE INDEX IF NOT EXISTS idx_deployments_miner_id_round ON deployments(miner_id, round_id);
CREATE INDEX IF NOT EXISTS idx_deployments_round          ON deployments(round_id);
CREATE INDEX IF NOT EXISTS idx_deployments_miner_id       ON deployments(miner_id);
CREATE INDEX IF NOT EXISTS idx_deployments_prs            ON deployments(miner_id, round_id, total_sol_earned);

CREATE INDEX IF NOT EXISTS idx_rounds_id                  ON rounds(id);

CREATE INDEX IF NOT EXISTS idx_deployment_squares_deployment_id
  ON deployment_squares(deployment_id);

CREATE INDEX IF NOT EXISTS idx_deployment_squares_square
  ON deployment_squares(square);

CREATE INDEX IF NOT EXISTS idx_deployment_squares_deployment_id_square_amount
  ON deployment_squares(deployment_id, square, amount);

CREATE INDEX IF NOT EXISTS idx_rounds_winning_square
  ON rounds(winning_square);

CREATE INDEX IF NOT EXISTS idx_miners_pubkey_id
  ON miners(id, pubkey);

CREATE INDEX IF NOT EXISTS idx_miners_snapshots_miner_id
  ON miner_snapshots(miner_id);

CREATE INDEX IF NOT EXISTS idx_miner_totals
  ON miner_totals(miner_id);

