use ore_api::state::{Round, Treasury};
use serde::{Deserialize, Serialize};
use sqlx::{prelude::FromRow, Pool, QueryBuilder, Sqlite};
use tokio::time::Instant;

use crate::{app_state::AppMiner};

#[derive(Serialize, Deserialize, Debug, Clone, FromRow)]
pub struct CreateMinerSnapshot {
    pub pubkey: String,
    pub unclaimed_ore: i64,
    pub refined_ore: i64,
    pub lifetime_sol: i64,
    pub lifetime_ore: i64,
    pub created_at: i64,
}

impl From<AppMiner> for CreateMinerSnapshot {
    fn from(r: AppMiner) -> Self {
        CreateMinerSnapshot {
            pubkey: r.authority,
            unclaimed_ore: r.rewards_ore as i64,
            refined_ore: r.refined_ore as i64,
            lifetime_sol: r.lifetime_rewards_sol as i64,
            lifetime_ore: r.lifetime_rewards_ore as i64,
            created_at: chrono::Utc::now().timestamp(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, FromRow)]
pub struct DbMinerSnapshot {
    pub id: i64,
    pub pubkey: String,
    pub unclaimed_ore: i64,
    pub refined_ore: i64,
    pub lifetime_sol: i64,
    pub lifetime_ore: i64,
    pub created_at: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, FromRow)]
pub struct CreateTreasury {
    pub balance: i64,
    pub motherlode: i64,
    pub total_staked: i64,
    pub total_unclaimed: i64,
    pub total_refined: i64,
    pub created_at: String, // RFC3339
}

impl From<Treasury> for CreateTreasury {
    fn from(r: Treasury) -> Self {
        CreateTreasury {
            balance: r.balance as i64,
            motherlode: r.motherlode as i64,
            total_staked: r.total_staked as i64,
            total_unclaimed: r.total_unclaimed as i64,
            total_refined: r.total_refined as i64,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, FromRow)]
pub struct DbTreasury {
    pub id: i64,
    pub balance: i64,
    pub motherlode: i64,
    pub total_staked: i64,
    pub total_unclaimed: i64,
    pub total_refined: i64,
    pub created_at: String, // RFC3339
}

#[derive(Serialize, Deserialize, Debug, Clone, FromRow)]
pub struct CreateDeployment {
    pub round_id: i64,
    pub pubkey: String,
    pub square_id: i64,
    pub amount: i64,
    pub sol_earned: i64,
    pub ore_earned: i64,
    pub unclaimed_ore: i64,
    pub created_at: String, // RFC3339
}

#[derive(Serialize, Deserialize, Debug, Clone, FromRow)]
pub struct RoundRow {
    pub id: i64,
    pub slot_hash: Vec<u8>,
    pub winning_square: i64,
    pub expires_at: i64,
    pub motherlode: i64,
    pub rent_payer: String,
    pub top_miner: String,
    pub top_miner_reward: i64,
    pub total_deployed: i64,
    pub total_vaulted: i64,
    pub total_winnings: i64,
    pub created_at: String, // RFC3339
}

impl From<Round> for RoundRow {
    fn from(r: Round) -> Self {
        if let Some(rand) = r.rng() {
            RoundRow {
                id: r.id as i64,
                slot_hash: r.slot_hash.to_vec(),
                winning_square: r.winning_square(rand) as i64,
                expires_at: r.expires_at as i64,
                motherlode: r.motherlode as i64,
                rent_payer: r.rent_payer.to_string(),
                top_miner: r.top_miner.to_string(),
                top_miner_reward: r.top_miner_reward as i64,
                total_deployed: r.total_deployed as i64,
                total_vaulted: r.total_vaulted as i64,
                total_winnings: r.total_winnings as i64,
                created_at: chrono::Utc::now().to_rfc3339(),
            }
        } else {
            RoundRow {
                id: r.id as i64,
                slot_hash: r.slot_hash.to_vec(),
                winning_square: 100,
                expires_at: r.expires_at as i64,
                motherlode: r.motherlode as i64,
                rent_payer: r.rent_payer.to_string(),
                top_miner: r.top_miner.to_string(),
                top_miner_reward: r.top_miner_reward as i64,
                total_deployed: r.total_deployed as i64,
                total_vaulted: r.total_vaulted as i64,
                total_winnings: r.total_winnings as i64,
                created_at: chrono::Utc::now().to_rfc3339(),
            }
        }
    }
}

pub async fn insert_treasury(pool: &Pool<Sqlite>, r: &CreateTreasury) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO treasury (
            balance, motherlode, total_staked, total_unclaimed, total_refined, created_at
        ) VALUES (?, ?, ?, ?, ?, ?)
        "#
    )
    .bind(r.balance)
    .bind(r.motherlode)
    .bind(r.total_staked)
    .bind(r.total_unclaimed)
    .bind(r.total_refined)
    .bind(&r.created_at)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_treasuries(pool: &Pool<Sqlite>, limit: i64, offset: i64) -> Result<Vec<DbTreasury>, sqlx::Error> {
    let treasuries = sqlx::query_as::<_, DbTreasury>(
        r#"
        SELECT * FROM treasury
        ORDER BY id DESC
        LIMIT ? OFFSET ?
        "#
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok(treasuries)
}

pub async fn insert_round(pool: &Pool<Sqlite>, r: &RoundRow) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO rounds (
            id, slot_hash, winning_square, expires_at, motherlode, rent_payer, top_miner,
            top_miner_reward, total_deployed, total_vaulted, total_winnings, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            slot_hash        = excluded.slot_hash,
            winning_square   = excluded.winning_square,
            expires_at       = excluded.expires_at,
            motherlode       = excluded.motherlode,
            rent_payer       = excluded.rent_payer,
            top_miner        = excluded.top_miner,
            top_miner_reward = excluded.top_miner_reward,
            total_deployed   = excluded.total_deployed,
            total_vaulted    = excluded.total_vaulted,
            total_winnings   = excluded.total_winnings,
            created_at       = excluded.created_at
        "#
    )
    .bind(r.id)
    .bind(r.slot_hash.as_slice())
    .bind(r.winning_square)
    .bind(r.expires_at)
    .bind(r.motherlode)
    .bind(r.rent_payer.clone())
    .bind(r.top_miner.clone())
    .bind(r.top_miner_reward)
    .bind(r.total_deployed)
    .bind(r.total_vaulted)
    .bind(r.total_winnings)
    .bind(&r.created_at)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_round_by_id(pool: &Pool<Sqlite>, round_id: i64) -> Result<Vec<RoundRow>, sqlx::Error> {
    let rounds = sqlx::query_as::<_, RoundRow>(
        r#"
        SELECT * FROM rounds
        WHERE id = ?
        "#
    )
    .bind(round_id)
    .fetch_all(pool)
    .await?;

    return Ok(rounds)
}

pub async fn get_rounds(pool: &Pool<Sqlite>, limit: i64, offset: i64, ml: Option<bool>) -> Result<Vec<RoundRow>, sqlx::Error> {
    if let Some(ml) = ml {
        if ml {
            let rounds = sqlx::query_as::<_, RoundRow>(
                r#"
                SELECT * FROM rounds
                WHERE motherlode > 0
                ORDER BY id DESC
                LIMIT ? OFFSET ?
                "#
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await?;

            return Ok(rounds)
        }
    }

    let rounds = sqlx::query_as::<_, RoundRow>(
        r#"
        SELECT * FROM rounds
        ORDER BY id DESC
        LIMIT ? OFFSET ?
        "#
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok(rounds)
}

pub async fn get_rounds_via_cursor(pool: &Pool<Sqlite>, limit: i64, cursor: i64, ml: Option<bool>) -> Result<Vec<RoundRow>, sqlx::Error> {
    if let Some(ml) = ml {
        if ml {
            let rounds = sqlx::query_as::<_, RoundRow>(
                r#"
                SELECT * FROM rounds
                WHERE motherlode > 0
                AND id < ?
                ORDER BY id DESC
                LIMIT ?
                "#
            )
            .bind(cursor)
            .bind(limit)
            .fetch_all(pool)
            .await?;

            return Ok(rounds)
        }
    }

    let rounds = sqlx::query_as::<_, RoundRow>(
        r#"
        SELECT * FROM rounds
        WHERE id < ?
        ORDER BY id DESC
        LIMIT ?
        "#
    )
    .bind(cursor)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rounds)
}

pub async fn get_miner_rounds(pool: &Pool<Sqlite>, pubkey: String, limit: i64, offset: i64) -> Result<Vec<RoundRow>, sqlx::Error> {
    let rounds = sqlx::query_as::<_, RoundRow>(
        r#"
        SELECT * FROM rounds r
        WHERE EXISTS (
          SELECT 1 FROM deployments d
          WHERE d.round_id = r.id
            AND d.pubkey   = ?
        )
        ORDER BY r.created_at DESC
        LIMIT ? OFFSET ?
        "#
    )
    .bind(pubkey)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok(rounds)
}

pub async fn get_miner_rounds_via_cursor(pool: &Pool<Sqlite>, pubkey: String, limit: i64, cursor: i64) -> Result<Vec<RoundRow>, sqlx::Error> {
    let rounds = sqlx::query_as::<_, RoundRow>(
        r#"
        SELECT * FROM rounds r
        WHERE EXISTS (
          SELECT 1 FROM deployments d
          WHERE d.round_id = r.id
            AND d.pubkey   = ?
        )
        AND r.id < ?
        ORDER BY r.id DESC
        LIMIT ?
        "#
    )
    .bind(pubkey)
    .bind(cursor)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rounds)
}

pub async fn insert_deployment(pool: &Pool<Sqlite>, d: &CreateDeployment) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO deployments (
            round_id, pubkey, square_id, amount, sol_earned, ore_earned, unclaimed_ore, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(round_id, pubkey, square_id) DO UPDATE SET
            amount        = excluded.amount,
            sol_earned    = excluded.sol_earned,
            ore_earned    = excluded.ore_earned,
            unclaimed_ore = excluded.unclaimed_ore,
            created_at    = excluded.created_at
        "#
    )
    .bind(d.round_id)
    .bind(&d.pubkey)
    .bind(d.square_id)
    .bind(d.amount)
    .bind(d.sol_earned)
    .bind(d.ore_earned)
    .bind(d.unclaimed_ore)
    .bind(&d.created_at)
    .execute(pool)
    .await?;

    Ok(())
}


pub async fn insert_deployments(
    pool: &Pool<Sqlite>,
    rows: &[CreateDeployment],
) -> Result<(), sqlx::Error> {
    const CHUNK_SIZE: usize = 100;

    let mut tx = pool.begin().await?;

    for chunk in rows.chunks(CHUNK_SIZE) {
        let mut qb = QueryBuilder::<Sqlite>::new(
            "INSERT INTO deployments (
                round_id, pubkey, square_id, amount, sol_earned, ore_earned, unclaimed_ore, created_at
            ) ",
        );

        qb.push_values(chunk, |mut b, d| {
            b.push_bind(d.round_id)
                .push_bind(&d.pubkey)
                .push_bind(d.square_id)
                .push_bind(d.amount)
                .push_bind(d.sol_earned)
                .push_bind(d.ore_earned)
                .push_bind(d.unclaimed_ore)
                .push_bind(&d.created_at);
        });

        qb.build().execute(&mut *tx).await?;
    }

    tx.commit().await?;
    Ok(())
}


#[derive(Serialize, Deserialize, Debug, Clone, FromRow)]
pub struct GetDeployment {
    pub round_id: i64,
    pub pubkey: String,
    pub square_id: i64,
    pub amount: i64,
    pub sol_earned: i64,
    pub ore_earned: i64,
}

pub async fn get_deployments_by_round(
    pool: &Pool<Sqlite>,
    round_id: i64,
) -> Result<Vec<GetDeployment>, sqlx::Error> {
    let deployments = sqlx::query_as::<_, GetDeployment>(
        r#"
        SELECT
            round_id, pubkey, square_id, amount,
            sol_earned, ore_earned
        FROM deployments
        WHERE round_id = ?
        ORDER BY ore_earned DESC
        "#
    )
    .bind(round_id)
    .fetch_all(pool)
    .await?;

    Ok(deployments)
}

pub async fn insert_miner_snapshots(
    pool: &Pool<Sqlite>,
    rows: &[CreateMinerSnapshot],
) -> Result<(), sqlx::Error> {
    let n = Instant::now();
    tracing::info!("Inserting snapshots ({} rows)", rows.len());
    const CHUNK_SIZE: usize = 150;

    let mut tx = pool.begin().await?;

    for chunk in rows.chunks(CHUNK_SIZE) {
        let mut qb = QueryBuilder::<Sqlite>::new(
            r#"
            INSERT INTO miner_snapshots (
                pubkey, unclaimed_ore, refined_ore, lifetime_sol, lifetime_ore, created_at
            )
            "#,
        );

        qb.push_values(chunk, |mut b, d| {
            b.push_bind(&d.pubkey)
                .push_bind(d.unclaimed_ore)
                .push_bind(d.refined_ore)
                .push_bind(d.lifetime_sol)
                .push_bind(d.lifetime_ore)
                .push_bind(&d.created_at);
        });

        qb.build().execute(&mut *tx).await?;
    }

    tx.commit().await?;
    tracing::info!("Inserted snapshots in: {} ms", n.elapsed().as_millis());
    Ok(())
}


pub async fn get_miner_snapshots(
    pool: &Pool<Sqlite>,
    pubkey: String,
    limit: i64,
    offset: i64,
) -> Result<Vec<DbMinerSnapshot>, sqlx::Error> {
    let miner_data = sqlx::query_as::<_, DbMinerSnapshot>(
        r#"
        SELECT
            id, pubkey, unclaimed_ore, refined_ore, lifetime_sol, lifetime_ore, created_at
        FROM miner_snapshots
        WHERE pubkey = ?
        ORDER BY id DESC
        LIMIT ? OFFSET ?
        "#
    )
    .bind(pubkey)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok(miner_data)
}


pub async fn get_miner_stats(
    pool: &sqlx::SqlitePool,
    pubkey: String,
) -> anyhow::Result<Option<MinerTotalsRow>> {
    let row = sqlx::query_as::<_, MinerTotalsRow>(r#"
        SELECT
          pubkey,
          rounds_played,
          rounds_won,
          total_sol_deployed,
          total_sol_earned,
          total_ore_earned,
          net_sol_change,
          CASE
            WHEN net_sol_change > 0 THEN 'up'
            WHEN net_sol_change < 0 THEN 'down'
            ELSE 'flat'
          END AS sol_balance_direction
        FROM miner_totals
        WHERE pubkey = ?
    "#)
    .bind(pubkey)
    .fetch_optional(pool) // returns Option<T>
    .await?;

    Ok(row)
}

pub async fn finalize_round_idempotent(pool: &sqlx::SqlitePool, round_id: i64) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;

    // a) Read prior contribution for this round
    let prior: Vec<(String, i64, i64, i64, i64, i64, i64)> = sqlx::query_as(r#"
        SELECT pubkey, 1 as rounds_played, won_round, total_sol_deployed, total_sol_earned, total_ore_earned, net_sol_round
        FROM miner_round_stats
        WHERE round_id = ?
    "#).bind(round_id).fetch_all(&mut *tx).await?;

    // b) Subtract prior from miner_totals (if any)
    for (pubkey, rp, won, dep, earned, ore, net) in prior {
        sqlx::query(r#"
            UPDATE miner_totals SET
              rounds_played      = rounds_played      - ?,
              rounds_won         = rounds_won         - ?,
              total_sol_deployed = total_sol_deployed - ?,
              total_sol_earned   = total_sol_earned   - ?,
              total_ore_earned   = total_ore_earned   - ?,
              net_sol_change     = net_sol_change     - ?
            WHERE pubkey = ?
        "#)
        .bind(rp).bind(won).bind(dep).bind(earned).bind(ore).bind(net)
        .bind(&pubkey)
        .execute(&mut *tx).await?;
    }

    // c) Recompute & upsert this round's rows (same SELECT as above)
    sqlx::query(r#"
        INSERT INTO miner_round_stats (
            round_id, pubkey, total_sol_deployed, total_sol_earned, total_ore_earned, won_round, net_sol_round
        )
        SELECT
            d.round_id,
            d.pubkey,
            SUM(d.amount),
            SUM(d.sol_earned),
            SUM(d.ore_earned),
            MAX(CASE WHEN d.square_id = r.winning_square THEN 1 ELSE 0 END),
            (SUM(d.sol_earned) - SUM(d.amount))
        FROM deployments d
        JOIN rounds r ON r.id = d.round_id
        WHERE d.round_id = ?
        GROUP BY d.round_id, d.pubkey
        ON CONFLICT(round_id, pubkey) DO UPDATE SET
          total_sol_deployed = excluded.total_sol_deployed,
          total_sol_earned   = excluded.total_sol_earned,
          total_ore_earned   = excluded.total_ore_earned,
          won_round          = excluded.won_round,
          net_sol_round      = excluded.net_sol_round
    "#).bind(round_id).execute(&mut *tx).await?;

    // d) Add fresh contribution to totals
    sqlx::query(r#"
        INSERT INTO miner_totals (
          pubkey, rounds_played, rounds_won, total_sol_deployed, total_sol_earned, total_ore_earned, net_sol_change
        )
        SELECT
          s.pubkey,
          COUNT(*),
          SUM(s.won_round),
          SUM(s.total_sol_deployed),
          SUM(s.total_sol_earned),
          SUM(s.total_ore_earned),
          SUM(s.net_sol_round)
        FROM miner_round_stats s
        WHERE s.round_id = ?
        GROUP BY s.pubkey
        ON CONFLICT(pubkey) DO UPDATE SET
          rounds_played      = miner_totals.rounds_played      + excluded.rounds_played,
          rounds_won         = miner_totals.rounds_won         + excluded.rounds_won,
          total_sol_deployed = miner_totals.total_sol_deployed + excluded.total_sol_deployed,
          total_sol_earned   = miner_totals.total_sol_earned   + excluded.total_sol_earned,
          total_ore_earned   = miner_totals.total_ore_earned   + excluded.total_ore_earned,
          net_sol_change     = miner_totals.net_sol_change     + excluded.net_sol_change
    "#).bind(round_id).execute(&mut *tx).await?;

    tx.commit().await?;
    Ok(())
}


pub async fn get_available_pubkeys(pool: &Pool<Sqlite>, limit: String) -> Result<Vec<String>, sqlx::Error> {
    Ok(vec![])
}

pub async fn get_snapshot_24h_ago(
    pool: &Pool<Sqlite>,
    pubkey: String,
) -> Result<Option<DbMinerSnapshot>, sqlx::Error> {
    // target = now - 24h (seconds)
    // narrow window: ±15 minutes is plenty for 60–90s cadence
    let rows: Vec<DbMinerSnapshot> = sqlx::query_as::<_, DbMinerSnapshot>(
        r#"
        WITH target(ts) AS (SELECT strftime('%s','now','-24 hours'))
        SELECT id, pubkey, unclaimed_ore, refined_ore, lifetime_sol, lifetime_ore, created_at
        FROM miner_snapshots, target
        WHERE pubkey = ?
          AND created_at BETWEEN (ts - 900) AND (ts + 900)
        ORDER BY ABS(created_at - ts)
        LIMIT 1
        "#
    )
    .bind(&pubkey)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().next())
}

#[derive(Serialize, Deserialize, Debug, Clone, sqlx::FromRow)]
pub struct MinerTotalsRow {
    pub pubkey: String,
    pub rounds_played: i64,
    pub rounds_won: i64,                 // NEW
    pub total_sol_deployed: i64,
    pub total_sol_earned: i64,
    pub total_ore_earned: i64,
    pub net_sol_change: i64,
    pub sol_balance_direction: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, sqlx::FromRow)]
pub struct MinerLeaderboardRow {
    pub rank: i64,
    pub pubkey: String,
    pub rounds_played: i64,
    pub rounds_won: i64,
    pub total_sol_deployed: i64,
    pub total_sol_earned: i64,
    pub total_ore_earned: i64,
    pub net_sol_change: i64,
    pub sol_balance_direction: String,
}

pub async fn get_miner_totals_all_time(
    pool: &sqlx::SqlitePool,
    limit: i64,
    offset: i64,
) -> anyhow::Result<Vec<MinerTotalsRow>> {
    let rows = sqlx::query_as::<_, MinerTotalsRow>(r#"
        WITH per_miner_round AS (
          SELECT
            d.pubkey,
            d.round_id,
            SUM(d.amount)      AS total_deployed,
            SUM(d.sol_earned)  AS total_sol_earned,
            SUM(d.ore_earned)  AS total_ore_earned,
            MAX(CASE WHEN d.square_id = r.winning_square THEN 1 ELSE 0 END) AS won_round,
            (SUM(d.sol_earned) - SUM(d.amount)) AS net_sol_round
          FROM deployments d
          JOIN rounds r ON r.id = d.round_id
          GROUP BY d.pubkey, d.round_id
        )
        SELECT
          pubkey,
          COUNT(*)                                  AS rounds_played,
          SUM(won_round)                            AS rounds_won,
          SUM(total_deployed)                       AS total_sol_deployed,
          SUM(total_sol_earned)                     AS total_sol_earned,
          SUM(total_ore_earned)                     AS total_ore_earned,
          SUM(net_sol_round)                        AS net_sol_change,
          CASE
            WHEN SUM(net_sol_round) > 0 THEN 'up'
            WHEN SUM(net_sol_round) < 0 THEN 'down'
            ELSE 'flat'
          END AS sol_balance_direction
        FROM per_miner_round
        GROUP BY pubkey
        HAVING COUNT(*) >= 100
        ORDER BY net_sol_change DESC
        LIMIT ? OFFSET ?;
    "#)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

pub async fn get_leaderboard_last_n_rounds(
    pool: &sqlx::SqlitePool,
    n_rounds: i64,
    limit: i64,
    offset: i64,
) -> anyhow::Result<Vec<MinerLeaderboardRow>> {
    let rows = sqlx::query_as::<_, MinerLeaderboardRow>(r#"
        WITH last_60_rounds AS (
          SELECT id
          FROM rounds
          ORDER BY id DESC
          LIMIT ?
        ),
        per_miner_round AS (
          SELECT
            d.pubkey,
            d.round_id,
            SUM(d.amount)      AS total_deployed,
            SUM(d.sol_earned)  AS total_sol_earned,
            SUM(d.ore_earned)  AS total_ore_earned,
            MAX(CASE WHEN d.square_id = r.winning_square THEN 1 ELSE 0 END) AS won_round,
            (SUM(d.sol_earned) - SUM(d.amount)) AS net_sol_round
          FROM deployments d
          JOIN rounds r ON r.id = d.round_id
          WHERE d.round_id IN (SELECT id FROM last_60_rounds)
          GROUP BY d.pubkey, d.round_id
        ),
        miner_aggs AS (
          SELECT
            pubkey,
            COUNT(*)              AS rounds_played,
            SUM(won_round)        AS rounds_won,
            SUM(total_deployed)   AS total_sol_deployed,
            SUM(total_sol_earned) AS total_sol_earned,
            SUM(total_ore_earned) AS total_ore_earned,
            SUM(net_sol_round)    AS net_sol_change
          FROM per_miner_round
          GROUP BY pubkey
        )
        SELECT
          ROW_NUMBER() OVER (ORDER BY net_sol_change DESC) AS rank,
          pubkey,
          rounds_played,
          rounds_won,
          total_sol_deployed,
          total_sol_earned,
          total_ore_earned,
          net_sol_change,
          CASE
            WHEN net_sol_change > 0 THEN 'up'
            WHEN net_sol_change < 0 THEN 'down'
            ELSE 'flat'
          END AS sol_balance_direction
        FROM miner_aggs
        ORDER BY rank
        LIMIT ? OFFSET ?;
    "#)
    .bind(n_rounds)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}


#[derive(Serialize, Deserialize, Debug, Clone, sqlx::FromRow)]
pub struct MinerOreLeaderboardRow {
    pub rank: i64,                // ranked by total_ore_earned DESC
    pub pubkey: String,
    pub rounds_played: i64,
    pub rounds_won: i64,
    pub total_sol_deployed: i64,
    pub total_sol_earned: i64,
    pub total_ore_earned: i64,
    pub net_sol_change: i64,      // still useful context even though we sort by ore
}


pub async fn get_ore_leaderboard_all_time(
    pool: &sqlx::SqlitePool,
    limit: i64,
    offset: i64,
) -> anyhow::Result<Vec<MinerOreLeaderboardRow>> {
    let rows = sqlx::query_as::<_, MinerOreLeaderboardRow>(r#"
        WITH per_miner_round AS (
          SELECT
            d.pubkey,
            d.round_id,
            SUM(d.amount)      AS total_deployed,
            SUM(d.sol_earned)  AS total_sol_earned,
            SUM(d.ore_earned)  AS total_ore_earned,
            MAX(CASE WHEN d.square_id = r.winning_square THEN 1 ELSE 0 END) AS won_round,
            (SUM(d.sol_earned) - SUM(d.amount)) AS net_sol_round
          FROM deployments d
          JOIN rounds r ON r.id = d.round_id
          GROUP BY d.pubkey, d.round_id
        ),
        miner_aggs AS (
          SELECT
            pubkey,
            COUNT(*)                  AS rounds_played,
            SUM(won_round)            AS rounds_won,
            SUM(total_deployed)       AS total_sol_deployed,
            SUM(total_sol_earned)     AS total_sol_earned,
            SUM(total_ore_earned)     AS total_ore_earned,
            SUM(net_sol_round)        AS net_sol_change
          FROM per_miner_round
          GROUP BY pubkey
          HAVING COUNT(*) >= 100
        )
        SELECT
          ROW_NUMBER() OVER (ORDER BY total_ore_earned DESC, total_sol_earned DESC) AS rank,
          pubkey,
          rounds_played,
          rounds_won,
          total_sol_deployed,
          total_sol_earned,
          total_ore_earned,
          net_sol_change
        FROM miner_aggs
        ORDER BY rank
        LIMIT ? OFFSET ?;
    "#)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_ore_leaderboard_last_n_rounds(
    pool: &sqlx::SqlitePool,
    n_rounds: i64,
    limit: i64,
    offset: i64,
) -> anyhow::Result<Vec<MinerOreLeaderboardRow>> {
    let rows = sqlx::query_as::<_, MinerOreLeaderboardRow>(r#"
        WITH last_n_rounds AS (
          SELECT id
          FROM rounds
          ORDER BY id DESC
          LIMIT ?
        ),
        per_miner_round AS (
          SELECT
            d.pubkey,
            d.round_id,
            SUM(d.amount)      AS total_deployed,
            SUM(d.sol_earned)  AS total_sol_earned,
            SUM(d.ore_earned)  AS total_ore_earned,
            MAX(CASE WHEN d.square_id = r.winning_square THEN 1 ELSE 0 END) AS won_round,
            (SUM(d.sol_earned) - SUM(d.amount)) AS net_sol_round
          FROM deployments d
          JOIN rounds r ON r.id = d.round_id
          WHERE d.round_id IN (SELECT id FROM last_n_rounds)
          GROUP BY d.pubkey, d.round_id
        ),
        miner_aggs AS (
          SELECT
            pubkey,
            COUNT(*)                  AS rounds_played,
            SUM(won_round)            AS rounds_won,
            SUM(total_deployed)       AS total_sol_deployed,
            SUM(total_sol_earned)     AS total_sol_earned,
            SUM(total_ore_earned)     AS total_ore_earned,
            SUM(net_sol_round)        AS net_sol_change
          FROM per_miner_round
          GROUP BY pubkey
        )
        SELECT
          ROW_NUMBER() OVER (ORDER BY total_ore_earned DESC, total_sol_earned DESC) AS rank,
          pubkey,
          rounds_played,
          rounds_won,
          total_sol_deployed,
          total_sol_earned,
          total_ore_earned,
          net_sol_change
        FROM miner_aggs
        ORDER BY rank
        LIMIT ? OFFSET ?;
    "#)
    .bind(n_rounds.max(1))
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}


pub async fn get_miner_totals_all_time_v2(
    pool: &sqlx::SqlitePool,
    limit: i64,
    offset: i64,
) -> anyhow::Result<Vec<MinerTotalsRow>> {
    let rows = sqlx::query_as::<_, MinerTotalsRow>(r#"
        SELECT
          pubkey,
          rounds_played,
          rounds_won,
          total_sol_deployed,
          total_sol_earned,
          total_ore_earned,
          net_sol_change,
          CASE
            WHEN net_sol_change > 0 THEN 'up'
            WHEN net_sol_change < 0 THEN 'down'
            ELSE 'flat'
          END AS sol_balance_direction
        FROM miner_totals
        WHERE rounds_played >= 100
        ORDER BY net_sol_change DESC
        LIMIT ? OFFSET ?;
    "#)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_ore_leaderboard_all_time_v2(
    pool: &sqlx::SqlitePool,
    limit: i64,
    offset: i64,
) -> anyhow::Result<Vec<MinerOreLeaderboardRow>> {
    let rows = sqlx::query_as::<_, MinerOreLeaderboardRow>(r#"
        SELECT
          ROW_NUMBER() OVER (ORDER BY total_ore_earned DESC, total_sol_earned DESC) AS rank,
          pubkey,
          rounds_played,
          rounds_won,
          total_sol_deployed,
          total_sol_earned,
          total_ore_earned,
          net_sol_change
        FROM miner_totals
        WHERE rounds_played >= 100
        ORDER BY rank
        LIMIT ? OFFSET ?;
    "#)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}


pub async fn get_leaderboard_last_n_rounds_v2(
    pool: &sqlx::SqlitePool,
    n_rounds: i64,
    limit: i64,
    offset: i64,
) -> anyhow::Result<Vec<MinerLeaderboardRow>> {
    let rows = sqlx::query_as::<_, MinerLeaderboardRow>(r#"
        WITH last_n AS (
          SELECT id FROM rounds ORDER BY id DESC LIMIT ?
        ),
        agg AS (
          SELECT
            s.pubkey,
            COUNT(*)                         AS rounds_played,
            SUM(s.won_round)                 AS rounds_won,
            SUM(s.total_sol_deployed)        AS total_sol_deployed,
            SUM(s.total_sol_earned)          AS total_sol_earned,
            SUM(s.total_ore_earned)          AS total_ore_earned,
            SUM(s.net_sol_round)             AS net_sol_change
          FROM miner_round_stats s
          JOIN last_n r ON r.id = s.round_id
          GROUP BY s.pubkey
        )
        SELECT
          ROW_NUMBER() OVER (ORDER BY net_sol_change DESC) AS rank,
          pubkey,
          rounds_played,
          rounds_won,
          total_sol_deployed,
          total_sol_earned,
          total_ore_earned,
          net_sol_change,
          CASE
            WHEN net_sol_change > 0 THEN 'up'
            WHEN net_sol_change < 0 THEN 'down'
            ELSE 'flat'
          END AS sol_balance_direction
        FROM agg
        ORDER BY rank
        LIMIT ? OFFSET ?;
    "#)
    .bind(n_rounds.max(1))
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_ore_leaderboard_last_n_rounds_v2(
    pool: &sqlx::SqlitePool,
    n_rounds: i64,
    limit: i64,
    offset: i64,
) -> anyhow::Result<Vec<MinerOreLeaderboardRow>> {
    let rows = sqlx::query_as::<_, MinerOreLeaderboardRow>(r#"
        WITH last_n AS (
          SELECT id FROM rounds ORDER BY id DESC LIMIT ?
        ),
        agg AS (
          SELECT
            s.pubkey,
            COUNT(*)                  AS rounds_played,
            SUM(s.won_round)          AS rounds_won,
            SUM(s.total_sol_deployed) AS total_sol_deployed,
            SUM(s.total_sol_earned)   AS total_sol_earned,
            SUM(s.total_ore_earned)   AS total_ore_earned,
            SUM(s.net_sol_round)      AS net_sol_change
          FROM miner_round_stats s
          JOIN last_n r ON r.id = s.round_id
          GROUP BY s.pubkey
        )
        SELECT
          ROW_NUMBER() OVER (ORDER BY total_ore_earned DESC, total_sol_earned DESC) AS rank,
          pubkey,
          rounds_played,
          rounds_won,
          total_sol_deployed,
          total_sol_earned,
          total_ore_earned,
          net_sol_change
        FROM agg
        ORDER BY rank
        LIMIT ? OFFSET ?;
    "#)
    .bind(n_rounds.max(1))
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

