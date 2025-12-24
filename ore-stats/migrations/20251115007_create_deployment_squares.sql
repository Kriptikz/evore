CREATE TABLE IF NOT EXISTS deployment_squares (
    deployment_id     INTEGER NOT NULL,
    square            INTEGER NOT NULL,
    amount            INTEGER NOT NULL,
    slot              INTEGER NOT NULL,
    PRIMARY KEY (deployment_id, square),
    FOREIGN KEY(deployment_id) REFERENCES deployments(id)
);

