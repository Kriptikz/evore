use ore_api::state::Treasury;
use serde::{Deserialize, Serialize};
use sqlx::{prelude::FromRow, Pool, Sqlite};
use tokio::time::Instant;

use crate::app_state::{AppDeployedSquare, AppDeployment, AppRound, ReconstructedRound};

pub struct Database {
    writer: Pool<Sqlite>,
    reader: Pool<Sqlite>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CreateDeployment {
    pub round_id: i64,
    pub pubkey: String,
    pub deployed_squares: [AppDeployedSquare; 25],
    pub total_sol_deployed: i64,
    pub total_sol_earned: i64,
    pub total_ore_earned: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, FromRow)]
pub struct CreateTreasury {
    pub balance: i64,
    pub motherlode: i64,
    pub total_staked: i64,
    pub total_unclaimed: i64,
    pub total_refined: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct CreateRound {
    pub id: i64,
    pub winning_square: i64,
    pub motherlode: i64,
    pub top_miner: String,
    pub total_deployed: i64,
    pub total_vaulted: i64,
    pub total_winnings: i64,
    pub created_at: i64,
}

impl From<AppRound> for CreateRound {
    fn from(r: AppRound) -> Self {
        CreateRound {
            id: r.round_id,
            winning_square: r.winning_square,
            motherlode: r.motherlode,
            top_miner: r.top_miner,
            total_deployed: r.total_deployed,
            total_vaulted: r.total_vaulted,
            total_winnings: r.total_winnings,
            created_at: r.created_at,
        }
    }
}

impl From<AppDeployment> for CreateDeployment {
    fn from(r: AppDeployment) -> Self {
        CreateDeployment {
            round_id: r.round_id,
            pubkey: r.pubkey,
            deployed_squares: r.deployments,
            total_sol_deployed: r.total_deployed,
            total_sol_earned: r.total_sol_earned,
            total_ore_earned: r.total_ore_earned,
        }
    }
}

impl From<Treasury> for CreateTreasury {
    fn from(r: Treasury) -> Self {
        CreateTreasury {
            balance: r.balance as i64,
            motherlode: r.motherlode as i64,
            total_staked: r.total_staked as i64,
            total_unclaimed: r.total_unclaimed as i64,
            total_refined: r.total_refined as i64,
            created_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl Database {
    pub fn new(writer: Pool<Sqlite>, reader: Pool<Sqlite>) -> Self {
        Database { writer, reader }
    }

    pub async fn insert_treasury(&self, new_t: &CreateTreasury) -> Result<(), sqlx::Error> {
        let now = Instant::now();
        sqlx::query(
            r#"
            INSERT INTO treasury (
                balance, motherlode, total_staked, total_unclaimed, total_refined, created_at
            ) VALUES (?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(new_t.balance)
        .bind(new_t.motherlode)
        .bind(new_t.total_staked)
        .bind(new_t.total_unclaimed)
        .bind(new_t.total_refined)
        .bind(&new_t.created_at)
        .execute(&self.writer)
        .await?;

        let elapsed = now.elapsed().as_millis();
        if elapsed >= 1000 {
            tracing::warn!("Long Query: Treasury inserted in {}ms", elapsed);
        }
        Ok(())
    }

    pub async fn insert_round(&self, new_r: &CreateRound) -> Result<(), sqlx::Error> {
        let now = Instant::now();
        sqlx::query(
            r#"
            INSERT INTO rounds (
                id, winning_square, motherlode, top_miner, total_deployed, total_vaulted, total_winnings, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO NOTHING
            "#
        )
        .bind(new_r.id)
        .bind(new_r.winning_square)
        .bind(new_r.motherlode)
        .bind(new_r.top_miner.clone())
        .bind(new_r.total_deployed)
        .bind(new_r.total_vaulted)
        .bind(new_r.total_winnings)
        .bind(&new_r.created_at)
        .execute(&self.writer)
        .await?;

        let elapsed = now.elapsed().as_millis();
        if elapsed >= 1000 {
            tracing::warn!("Long Query: Round inserted in {}ms", elapsed);
        }
        Ok(())
    }


    /// Insert a reconstructed round and all of its deployments.
    /// - If the round doesn't exist, it is created.
    /// - If a miner doesn't exist (by pubkey), it is created.
    pub async fn insert_reconstructed_round(
        &self,
        rr: &ReconstructedRound,
    ) -> Result<(), sqlx::Error> {
        use chrono::Utc;

        let mut tx = self.writer.begin().await?;

        let now = Instant::now();
        let now_ts = Utc::now().timestamp();

        // 1) Insert round if it doesn't exist yet
        //    We use ON CONFLICT DO NOTHING to "create if missing"
        let round = &rr.round;
        sqlx::query(
            r#"
            INSERT INTO rounds (
                id,
                winning_square,
                motherlode,
                top_miner,
                total_deployed,
                total_vaulted,
                total_winnings,
                created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO NOTHING
            "#,
        )
        .bind(round.round_id)
        .bind(round.winning_square)
        .bind(round.motherlode)
        .bind(&round.top_miner)
        .bind(round.total_deployed)
        .bind(round.total_vaulted)
        .bind(round.total_winnings)
        .bind(now_ts)
        .execute(&mut *tx)
        .await?;

        // 2) For each deployment:
        //    - ensure miner exists (by pubkey)
        //    - insert deployment
        //    - insert related squares with amount > 0
        for deployment in rr.deployments.iter() {
            if deployment.total_deployed <= 0 {
                continue;
            }

            // 2a) lookup or create miner
            let miner_id: i64 = if let Some(existing_id) = sqlx::query_scalar::<_, i64>(
                r#"SELECT id FROM miners WHERE pubkey = ?"#,
            )
            .bind(&deployment.pubkey)
            .fetch_optional(&mut *tx)
            .await?
            {
                existing_id
            } else {
                // Insert new miner and get its id
                sqlx::query_scalar::<_, i64>(
                    r#"
                    INSERT INTO miners (pubkey)
                    VALUES (?)
                    RETURNING id
                    "#,
                )
                .bind(&deployment.pubkey)
                .bind(now_ts)
                .fetch_one(&mut *tx)
                .await?
            };

            // 2b) insert into deployments table
            let deployment_id: i64 = sqlx::query_scalar(
                r#"
                INSERT INTO deployments (
                    round_id,
                    miner_id,
                    total_deployed,
                    total_sol_earned,
                    total_ore_earned,
                    winner
                ) VALUES (?, ?, ?, ?, ?, ?)
                RETURNING id
                "#,
            )
            .bind(round.round_id)
            .bind(miner_id)
            .bind(deployment.total_deployed)
            .bind(deployment.total_sol_earned)
            .bind(deployment.total_ore_earned)
            .bind(if deployment.winner { 1_i64 } else { 0_i64 })
            .fetch_one(&mut *tx)
            .await?;

            // 2c) insert all squares for this deployment with non-zero amount
            for ds in deployment
                .deployments
                .iter()
                .filter(|ds| ds.amount > 0)
            {
                sqlx::query(
                    r#"
                    INSERT INTO deployment_squares (
                        deployment_id,
                        square,
                        amount,
                        slot
                    ) VALUES (?, ?, ?, ?)
                    "#,
                )
                .bind(deployment_id)
                .bind(ds.square_id)
                .bind(ds.amount)
                .bind(ds.slot)
                .execute(&mut *tx)
                .await?;
            }
        }

        tx.commit().await?;

        let elapsed = now.elapsed().as_millis();
        tracing::info!("Reconstructed round and deployments inserted in {}ms", elapsed);
        Ok(())
    }
}
