use std::time::Duration;

use crate::{app_state::{AppMiner, AppRound}, database::Database, external_api::get_ore_supply_rounds, rpc::AppRPC};

pub struct MinerSnapshot {
    round_id: u64,
    miners: Vec<AppMiner>,
    taken: bool,
    finalized: bool,
}

pub struct AccountTracker {
    app_rpc: AppRPC,
    app_db: Database
}

impl AccountTracker {
    pub fn new(app_rpc: AppRPC, app_db: Database) -> Self {
        AccountTracker {
            app_rpc,
            app_db
        }
    }

    pub async fn start_tracking(&mut self) {
        tracing::info!("AccountTracker::start_tracking running.");
        let mut tracking_round_id = loop {
            if let Ok(b) = self.app_rpc.get_board().await {
                break b.round_id;
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        };
        loop {
            // Get Board
            if let Ok(b) = self.app_rpc.get_board().await {
                let current_round_id = b.round_id;
                if current_round_id != tracking_round_id {
                    // Save the round data to the database
                    let round = loop {
                        if let Ok(r) = self.app_rpc.get_round(tracking_round_id).await {
                            break r;
                        }
                        tokio::time::sleep(Duration::from_secs(3)).await;
                    };
                    let app_round: AppRound = round.into();

                    if let Err(e) = self.app_db.insert_round(&app_round.into()).await {
                        println!("Failed to insert finalized round data.\nError: {:?}", e);
                    }

                    // Parse the deployments from transaction data
                    if let Ok(rr) = self.app_rpc.reconstruct_round_by_id(tracking_round_id).await {
                        if let Err(e) = self.app_db.insert_reconstructed_round(&rr).await {
                            println!("Failed to insert reconstructed round.\nError: {:?}", e);
                        }
                    }

                    // Set the tracking_round_id to the current_round_id
                    tracking_round_id = current_round_id;
                } 
            }
            // Waiting for board to go to next round, so the 
            // tracked round is finalized
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    }


    pub async fn backfill_rounds(&self) {
        let mut current_page = 0;
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            tracing::info!("Processing ore supply events reset page: {}", current_page);
            let fetched_rounds = get_ore_supply_rounds(current_page).await;

            if fetched_rounds.len() > 0 {
                for r in fetched_rounds {
                    if let Err(e) = self.app_db.insert_round(&r.into()).await {
                        tracing::error!("Failed to insert rounds.\nError: {:?}", e);
                    }
                }

                current_page += 1;
            } else {
                tracing::info!("No more rounds fetched from ore supply api.");
                break;
            }
        }

        tracing::info!("Rounds backfill complete!");
    }
}
