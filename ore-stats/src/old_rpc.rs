
use std::{env, str::FromStr, time::Duration};

use ore_api::{consts::{SPLIT_ADDRESS, TREASURY_ADDRESS}, state::{round_pda, Board, Miner, Round, Treasury}};
use serde::Deserialize;
use solana_account_decoder_client_types::UiAccountEncoding;
use solana_client::{nonblocking::{pubsub_client::PubsubClient, rpc_client::RpcClient}, rpc_config::RpcAccountInfoConfig, rpc_filter::RpcFilterType};
use solana_sdk::{commitment_config::{CommitmentConfig, CommitmentLevel}, slot_hashes::SlotHashes};
use steel::{AccountDeserialize, Numeric, Pubkey};
use tokio::time::Instant;
use tokio_stream::StreamExt;

use crate::{app_state::{AppLiveDeployment, AppMiner, AppRound, AppState, AppWinningSquare}, database::{self, insert_deployments, insert_miner_snapshots, insert_round, insert_treasury, CreateDeployment, CreateMinerSnapshot, CreateTreasury, RoundRow}, entropy_api::ORE_VAR_ADDRESS, BOARD_ADDRESS};

pub struct MinerSnapshot {
    round_id: u64,
    miners: Vec<AppMiner>,
    completed: bool,
}

#[derive(Deserialize)]
struct EntropyApiSeed {
    address: Vec<u8>,
    end_slot: u64,
    samples: u64,
    commit: Vec<u8>,
    seed: Vec<u8>,
}

pub async fn update_data_system(connection: RpcClient, app_state: AppState) {
    tracing::info!("Starting update_data_system");
    let db_pool = app_state.db_pool.clone();

    let entropy_seed_api = env::var("ENTROPY_SEED_API").expect("ENTROPY_SEED_API must be set");

    tokio::spawn(async move {
        let mut board_snapshot = false;
        let mut miners_snapshot = MinerSnapshot {
            round_id: 0,
            miners: vec![],
            completed: false,
        };
        let mut emitted_winning_square = false;
        loop {
            let treasury = if let Ok(treasury) = connection.get_account_data(&TREASURY_ADDRESS).await {
                if let Ok(treasury) = Treasury::try_from_bytes(&treasury) {
                    treasury.clone()
                } else {
                    tracing::error!("Failed to parse Treasury account");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue
                }
            } else {
                tracing::error!("Failed to load treasury account data");
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue
            };

            // update treasury
            let r = app_state.treasury.clone();
            let mut l = r.write().await;
            *l = treasury.into();
            drop(l);

            tokio::time::sleep(Duration::from_secs(1)).await;

            let board = if let Ok(board) = connection.get_account_data(&BOARD_ADDRESS).await {
                if let Ok(board) = Board::try_from_bytes(&board) {
                    board.clone()
                } else {
                    tracing::error!("Failed to parse Board account");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
            } else {
                tracing::error!("Failed to load board account data");
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            };

            // update board
            let r = app_state.board.clone();
            let mut l = r.write().await;
            *l = board.into();
            drop(l);

            let last_deployable_slot = board.end_slot;
            let current_slot = if let Ok(current_slot) = connection.get_slot().await {
                current_slot
            } else {
                tracing::error!("Failed to get slot from rpc");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            };

            let slots_left_in_round = last_deployable_slot as i64 - current_slot as i64;

            println!("Slots left for round: {}", slots_left_in_round);
            tokio::time::sleep(Duration::from_secs(1)).await;

            if slots_left_in_round <= 0 {
                if !board_snapshot {
                    tracing::info!("Updating data");
                    let round = if let Ok(round) = connection.get_account_data(&round_pda(board.round_id).0).await {
                        if let Ok(round) = Round::try_from_bytes(&round) {
                            round.clone()
                        } else {
                            tracing::error!("Failed to parse Round account");
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            continue;
                        }
                    } else {
                        tracing::error!("Failed to load round account data");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue
                    };

                    let mut miners: Vec<AppMiner> = vec![];
                    if let Ok(miners_data_raw) = connection.get_program_accounts_with_config(
                        &ore_api::id(),
                        solana_client::rpc_config::RpcProgramAccountsConfig { 
                            filters: Some(vec![RpcFilterType::DataSize(size_of::<Miner>() as u64 + 8)]),
                            account_config: solana_client::rpc_config::RpcAccountInfoConfig {
                                encoding: Some(UiAccountEncoding::Base64),
                                data_slice: None,
                                commitment: Some(CommitmentConfig { commitment: CommitmentLevel::Confirmed }),
                                min_context_slot: None,
                            },
                            with_context: None,
                            sort_results: None
                        } 
                    ).await {
                        for miner_data in miners_data_raw {
                            if let Ok(miner) = Miner::try_from_bytes(&miner_data.1.data) {
                                let mut miner = *miner;
                                miner.refined_ore = infer_refined_ore(&miner, &treasury);
                                miners.push(miner.clone().into());
                            }
                        }
                    }

                    if miners.len() > 0 {
                        miners_snapshot.round_id = round.id;
                        miners_snapshot.miners = miners.clone();
                        miners_snapshot.completed = false;
                        miners.sort_by(|a, b| b.rewards_ore.partial_cmp(&a.rewards_ore).unwrap());

                        tracing::info!("Setting miners snapshot completed to false");
                        
                    } else {
                        miners_snapshot.round_id = round.id;
                        miners_snapshot.miners = vec![];
                        miners_snapshot.completed = true;
                        tracing::info!("Setting miners snapshot completed to true");
                    }
                    board_snapshot = true;
                }
                if !emitted_winning_square {
                    println!("Checking and Emitting for winning square");
                    if let Ok(res) = reqwest::get(&entropy_seed_api).await {
                        if let Ok(d) = res.json::<EntropyApiSeed>().await {
                            let  entropy_var = if let Ok(v) = connection.get_account_data(&ORE_VAR_ADDRESS).await {
                                if let Ok(ovar) = crate::entropy_api::Var::try_from_bytes(&v) {
                                    ovar.clone()
                                } else {
                                    tracing::error!("Failed to parse Var account");
                                    tokio::time::sleep(Duration::from_secs(1)).await;
                                    continue
                                }
                            } else {
                                tracing::error!("Failed to load var account data");
                                tokio::time::sleep(Duration::from_secs(1)).await;
                                continue
                            };
                            if d.commit != entropy_var.commit {
                                tracing::info!("Missmatching commits..trying again..");
                                tokio::time::sleep(Duration::from_secs(2)).await;
                                continue
                            }
                            tokio::time::sleep(Duration::from_millis(200)).await;
                            match connection.get_account_data(&Pubkey::from_str("SysvarS1otHashes111111111111111111111111111").unwrap()).await {
                                Ok(data) => {
                                    let slot_hashes =
                                        bincode::deserialize::<SlotHashes>(&data).unwrap();
                                    if let Some(slot_hash) = slot_hashes.get(&entropy_var.end_at) {
                                        let s_hash =
                                            solana_program::keccak::hashv(&[&slot_hash.to_bytes(), &d.seed, &entropy_var.samples.to_le_bytes()])
                                                .to_bytes();
                                        tokio::time::sleep(Duration::from_millis(200)).await;
                                        let mut round = if let Ok(round) = connection.get_account_data(&round_pda(board.round_id).0).await {
                                            if let Ok(round) = Round::try_from_bytes(&round) {
                                                round.clone()
                                            } else {
                                                tracing::error!("Failed to parse Round account");
                                                tokio::time::sleep(Duration::from_secs(1)).await;
                                                continue;
                                            }
                                        } else {
                                            tracing::error!("Failed to load round account data");
                                            tokio::time::sleep(Duration::from_secs(1)).await;
                                            continue
                                        };

                                        round.slot_hash = s_hash;

                                        if let Some(r) = round.rng() {
                                            let wsquare = round.winning_square(r);
                                            tracing::info!("WINNING SQUARE: {}", wsquare);
                                            if let Err(_) = app_state.live_data_broadcaster.send(crate::app_state::LiveBroadcastData::WinningSquare(
                                                AppWinningSquare {
                                                    round_id: round.id,
                                                    winning_square: wsquare,
                                                }
                                            )) {
                                                tracing::error!("Failed to broadcast live round data");
                                            }
                                            emitted_winning_square = true;
                                        }
                                    } else {
                                        println!("\nFailed to get slothash\n");
                                        tokio::time::sleep(Duration::from_secs(1)).await;
                                        continue;
                                    };
                                },
                                Err(_e) => {
                                    println!("Failed to get slothash for slot {}", board.end_slot);
                                    tokio::time::sleep(Duration::from_secs(1)).await;
                                    continue;
                                }
                            }
                        }

                        tokio::time::sleep(Duration::from_millis(400)).await;
                        continue;
                    } else {
                        tracing::error!("Failed to get entropy seed api data");
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        continue;
                    }
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            } else if slots_left_in_round > 0 {
                let now = Instant::now();
                board_snapshot = false;
                emitted_winning_square = false;
                tracing::info!("Checking miner snapshot status: {}", miners_snapshot.completed);
                if !miners_snapshot.completed {
                    let r_now = Instant::now();
                    tracing::info!("Performing snapshot and updating round");
                    // load previous round
                    let round_id = board.round_id - 1;
                    let mut round = if let Ok(round) = connection.get_account_data(&round_pda(round_id).0).await {
                        if let Ok(round) = Round::try_from_bytes(&round) {
                            round.clone()
                        } else {
                            tracing::error!("Failed to parse Round account");
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            continue;
                        }
                    } else {
                        tracing::error!("Failed to load round account data");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue
                    };


                    if round.slot_hash == [0; 32] {
                        tracing::error!("Round slot hash should not be 0's");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    } else if round.slot_hash == [u8::MAX; 32] {
                        tracing::error!("Round reset failed");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        tracing::error!("");
                        // Update miners
                        let r = app_state.miners.clone();
                        let mut l = r.write().await;
                        *l = miners_snapshot.miners.clone();
                        drop(l);
                        miners_snapshot.completed = true;

                        let mut db_snapshot: Vec<CreateMinerSnapshot> = vec![];

                        for m in miners_snapshot.miners.iter() {
                            let m = m.clone();
                            db_snapshot.push(m.into());
                        }

                        // insert miners
                        if let Err(e) = insert_miner_snapshots(&db_pool, &db_snapshot).await {
                            tracing::error!("Failed to insert miners snapshot: {:?}", e);
                        }

                        // update round
                        let r = app_state.rounds.clone();
                        let mut l = r.write().await;
                        l.push(round.into());
                        drop(l);

                        // insert round
                        if let Err(e) = insert_round(&db_pool, &RoundRow::from(round)).await {
                            tracing::error!("Failed to insert round: {:?}", e);
                        }

                        // insert treasury
                        if let Err(e) = insert_treasury(&db_pool, &CreateTreasury::from(treasury)).await {
                            tracing::error!("Failed to insert treasury: {:?}", e);
                        }
                        miners_snapshot.completed = true;
                        continue;
                    } else {
                        // process round data
                        if let Some(_r) = round.rng() {
                            let (winning_square_opt, top_sample_opt, denom_opt) = if let Some(r) = round.rng() {
                                let winning_square = round.winning_square(r) as usize;

                                // Total deployed on winning square (denominator for pro-rata shares)
                                let denom = round.deployed[winning_square];
                                if denom == 0 {
                                    // Degenerate case: nothing deployed on the winning square → no rewards
                                    (Some(winning_square), None, Some(denom))
                                } else {
                                    // If split, every miner on the winning square shares top_miner_reward pro-rata.
                                    // If not split, one miner (whose cumulative range contains top_sample) takes it all.
                                    let top_sample = if round.top_miner == SPLIT_ADDRESS {
                                        None
                                    } else {
                                        Some(round.top_miner_sample(r, winning_square))
                                    };
                                    (Some(winning_square), top_sample, Some(denom))
                                }
                            } else {
                                (None, None, None)
                            };

                            let mut deployments: Vec<CreateDeployment> = Vec::new();

                            // Convenience captures
                            let winning_square = winning_square_opt;
                            let denom = denom_opt.unwrap_or(0);
                            let is_split = round.top_miner == SPLIT_ADDRESS;
                            let motherlode_amt = round.motherlode; // you already set this earlier if did_hit_motherlode
                            let total_winnings = round.total_winnings;
                            let top_sample = top_sample_opt; // same for all miners if not split

                            for miner in miners_snapshot.miners.iter() {
                                if miner.round_id == round.id {
                                     for (square_index, amount) in miner.deployed.iter().enumerate() {
                                         if *amount == 0 {
                                             continue;
                                         }

                                         // Defaults for non-winning squares (or missing RNG)
                                         let mut sol_earned_u64: u64 = 0;
                                         let mut ore_earned_u64: u64 = 0;

                                         // Only compute rewards on the winning square and when we had RNG
                                         if let Some(ws) = winning_square {
                                             if square_index == ws && denom > 0 {
                                                 // ---- SOL rewards ----
                                                 // Base = original_deployment - admin_fee (admin_fee = max(1, original/100))
                                                 let original = *amount as u64;
                                                 let admin_fee = (original / 100).max(1);
                                                 let mut rewards_sol = original.saturating_sub(admin_fee);

                                                 // Pro-rata share of round.total_winnings
                                                 let share = ((total_winnings as u128 * original as u128) / denom as u128) as u64;
                                                 rewards_sol = rewards_sol.saturating_add(share);

                                                 sol_earned_u64 = rewards_sol;

                                                 // ---- ORE rewards ----
                                                 // Top miner reward: split evenly pro-rata if split, else winner-takes-all by sample
                                                 if is_split {
                                                     let split_share = ((round.top_miner_reward as u128 * original as u128)
                                                         / denom as u128) as u64;
                                                     ore_earned_u64 = ore_earned_u64.saturating_add(split_share);
                                                 } else if let Some(sample) = top_sample {
                                                     // Check if this miner's cumulative interval covers the sample
                                                     let start = miner.cumulative[ws];
                                                     let end = start.saturating_add(original);
                                                     if sample >= start && sample < end {
                                                         ore_earned_u64 = ore_earned_u64.saturating_add(round.top_miner_reward);
                                                         round.top_miner = Pubkey::from_str(&miner.authority).unwrap();
                                                     }
                                                 }

                                                 // Motherlode reward (if any)
                                                 if motherlode_amt > 0 {
                                                     let ml_share = ((motherlode_amt as u128 * original as u128)
                                                         / denom as u128) as u64;
                                                     ore_earned_u64 = ore_earned_u64.saturating_add(ml_share);
                                                 }
                                             }
                                         }

                                         let deployment = CreateDeployment {
                                             round_id: miner.round_id as i64,
                                             pubkey: miner.authority.to_string(),
                                             square_id: square_index as i64,
                                             amount: *amount as i64,
                                             sol_earned: sol_earned_u64 as i64,
                                             ore_earned: ore_earned_u64 as i64,
                                             unclaimed_ore: miner.rewards_ore as i64,
                                             created_at: chrono::Utc::now().to_rfc3339(),
                                         };

                                         deployments.push(deployment);
                                     }
                                }

                            }
                            let n = Instant::now();
                            if let Err(e) = insert_deployments(&db_pool, &deployments).await {
                                tracing::error!("Failed to insert deployments: {:?}", e);
                            }
                            tracing::info!("Inserted deployments in {} ms", n.elapsed().as_millis());


                        } else {
                            tracing::error!("Failed to get round rng.");
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            continue
                        }
                        
                        // Update miners
                        let r = app_state.miners.clone();
                        let mut l = r.write().await;
                        *l = miners_snapshot.miners.clone();
                        drop(l);

                        let mut db_snapshot: Vec<CreateMinerSnapshot> = vec![];

                        for m in miners_snapshot.miners.iter() {
                            let m = m.clone();
                            db_snapshot.push(m.into());
                        }

                        // insert miners
                        if let Err(e) = insert_miner_snapshots(&db_pool, &db_snapshot).await {
                            tracing::error!("Failed to insert miners snapshot: {:?}", e);
                        }


                        // update round
                        let n = Instant::now();
                        tracing::info!("\n----------------\nUpdating round.");
                        let r = app_state.rounds.clone();
                        let mut l = r.write().await;
                        l.push(round.into());
                        drop(l);
                        tracing::info!("Updated round in {} ms", n.elapsed().as_millis());

                        // insert round
                        let n = Instant::now();
                        if let Err(e) = insert_round(&db_pool, &RoundRow::from(round)).await {
                            tracing::error!("Failed to insert round: {:?}", e);
                        }
                        tracing::info!("Inserted round in {} ms", n.elapsed().as_millis());

                        // insert treasury
                        let n = Instant::now();
                        if let Err(e) = insert_treasury(&db_pool, &CreateTreasury::from(treasury)).await {
                            tracing::error!("Failed to insert treasury: {:?}", e);
                        }
                        tracing::info!("Inserted treasury in {} ms", n.elapsed().as_millis());


                        let n = Instant::now();
                        if let Err(e) = database::finalize_round_idempotent(&db_pool, round.id as i64).await {
                            tracing::error!("Failed to finalize for round: {:?}", e);
                        }
                        tracing::info!("Finalized data in {} ms", n.elapsed().as_millis());

                        tracing::info!("Successfully snapshot round and updated database in {}ms", r_now.elapsed().as_millis());
                        miners_snapshot.completed = true;
                    }
                }



                let elapsed = now.elapsed().as_millis();
                let sleep_time = ((slots_left_in_round as u64  * 400) as u128 - elapsed) as u64;
                println!("Sleeping until round is over in {} ms", sleep_time);
                tokio::time::sleep(Duration::from_millis(sleep_time)).await;
            } else {
                board_snapshot = false;
                println!("Sleeping for 5 seconds");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }


        }
    });
}

pub fn infer_refined_ore(miner: &Miner, treasury: &Treasury) -> u64 {
    let delta = treasury.miner_rewards_factor - miner.rewards_factor;
    if delta < Numeric::ZERO {
        // Defensive: shouldn't happen, but keep behavior sane.
        return miner.refined_ore;
    }
    let accrued = (delta * Numeric::from_u64(miner.rewards_ore)).to_u64();
    miner.refined_ore.saturating_add(accrued)
}

pub fn refinement_level_percent(refined_ore: f64, unclaimed_ore: f64) -> f64 {
    if unclaimed_ore <= 0.0 {
        if refined_ore <= 0.0 {
            -10.0
        } else {
            f64::INFINITY
        }
    } else {
        -10.0 + 100.0 * (refined_ore / unclaimed_ore)
    }
}

pub async fn watch_live_board(rpc_url: &str, app_state: AppState) {
    let prefix = "ws://".to_string();
    let url = prefix + rpc_url;
    //let http_url = "https://".to_string() + rpc_url;
    tokio::spawn(async move {
        loop {
            if let Ok(ps_client) = PubsubClient::new(&url).await {
                let account_info_config = RpcAccountInfoConfig {
                    encoding: Some(UiAccountEncoding::Base64),
                    data_slice: None,
                    commitment: Some(CommitmentConfig { commitment: CommitmentLevel::Confirmed }),
                    min_context_slot: None,
                };
                let config = solana_client::rpc_config::RpcProgramAccountsConfig {
                    filters: None,
                    account_config: account_info_config,
                    with_context: Some(true),
                    sort_results: None,
                };
                let mut live_round_id = 0;
                if let Ok((mut accounts_stream, accounts_stream_unsub)) = ps_client.program_subscribe(&ore_api::id(), Some(config)).await {
                    while let Some(account_data) = accounts_stream.next().await {
                        let data_ctx = account_data.context;
                        if let Some(data) = account_data.value.account.data.decode() {
                            if let Ok(data) = ore_api::state::Round::try_from_bytes(&data) {
                                if data.id > live_round_id {
                                    live_round_id = data.id;
                                    let mut w = app_state.live_deployments.write().await;
                                    *w = vec![];
                                } else if data.id == live_round_id {
                                    // got new board data for this round
                                    let r = AppRound::from(*data);
                                    let mut w = app_state.live_round.write().await;
                                    *w = r.clone();
                                    drop(w);
                                    if let Err(_) = app_state.live_data_broadcaster.send(crate::app_state::LiveBroadcastData::Round(r)) {
                                        tracing::error!("Failed to broadcast live round data");
                                    }
                                } else {
                                    // got an old board for some reason, maybe a checkpoint or
                                    // something
                                }
                            } else if let Ok(miner) = ore_api::state::Miner::try_from_bytes(&data) {
                                if miner.round_id == live_round_id {
                                    let deployment_data = AppLiveDeployment {
                                        round: miner.round_id,
                                        slot: data_ctx.slot,
                                        authority: miner.authority.to_string(),
                                        deployments: miner.deployed,
                                        total_deployed: miner.deployed.iter().sum(),
                                    };
                                    let mut w = app_state.live_deployments.write().await;
                                    w.push(deployment_data.clone());
                                    drop(w);
                                    if let Err(_) = app_state.live_data_broadcaster.send(crate::app_state::LiveBroadcastData::Deployment(deployment_data)) {
                                        tracing::error!("Failed to broadcast live round data");
                                    }
                                }

                            }
                        }
                    }

                    accounts_stream_unsub().await;
                    println!("Unsubbed Successfully!");
                } else {
                    println!("Failed to subscribe to program");
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            }
        }
    });

    ()
}

