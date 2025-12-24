CREATE TABLE IF NOT EXISTS deployments (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    round_id          INTEGER NOT NULL,
    miner_id          INTEGER NOT NULL,
    total_deployed    INTEGER NOT NULL,
    total_sol_earned  INTEGER NOT NULL,
    total_ore_earned  INTEGER NOT NULL,
    winner            INTEGER NOT NULL,
    FOREIGN KEY(round_id) REFERENCES rounds(id),
    FOREIGN KEY(miner_id) REFERENCES miners(id)
);
