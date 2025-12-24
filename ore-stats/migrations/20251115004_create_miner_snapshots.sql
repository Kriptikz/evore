CREATE TABLE IF NOT EXISTS miner_snapshots (
    miner_id      INTEGER NOT NULL PRIMARY KEY,
    unclaimed_ore INTEGER NOT NULL,
    refined_ore   INTEGER NOT NULL,
    lifetime_sol  INTEGER NOT NULL,
    lifetime_ore  INTEGER NOT NULL,
    created_at    INTEGER NOT NULL,
    FOREIGN KEY(miner_id) REFERENCES miners(id)
);
