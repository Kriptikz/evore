CREATE TABLE IF NOT EXISTS rounds (
    id                INTEGER PRIMARY KEY,
    winning_square    INTEGER NOT NULL,
    motherlode        INTEGER NOT NULL,
    top_miner         TEXT    NOT NULL,
    total_deployed    INTEGER NOT NULL,
    total_vaulted     INTEGER NOT NULL,
    total_winnings    INTEGER NOT NULL,
    created_at        INTEGER NOT NULL
);
