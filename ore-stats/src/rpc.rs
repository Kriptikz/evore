use std::{collections::{HashMap, HashSet}, time::Duration, u64};

use anyhow::{anyhow, Result};
use chrono::Utc;
use const_crypto::ed25519;
use ore_api::{consts::{BOARD, ROUND, TREASURY_ADDRESS}, state::{round_pda, AutomationStrategy, Board, Miner, Round, Treasury}};
use solana_account_decoder_client_types::UiAccountEncoding;
use solana_client::{nonblocking::rpc_client::RpcClient, rpc_filter::RpcFilterType};
use solana_sdk::{commitment_config::{CommitmentConfig, CommitmentLevel}, keccak::hashv};
use steel::{AccountDeserialize, Pubkey};
use tokio::time::Instant;

use crate::{app_state::{AppDeployedSquare, AppDeployment, AppRound, AutomationCache, ReconstructedAutomation, ReconstructedRound}, helius_api::{HeliusApi, ParsedDeployment, ResetEvent}};

/// Program id for const pda derivations
const PROGRAM_ID: [u8; 32] = unsafe { *(&ore_api::id() as *const Pubkey as *const [u8; 32]) };

/// The address of the board account.
pub const BOARD_ADDRESS: Pubkey =
    Pubkey::new_from_array(ed25519::derive_program_address(&[BOARD], &PROGRAM_ID).0);

/// The address of the square account.
pub const ROUND_ADDRESS: Pubkey =
    Pubkey::new_from_array(ed25519::derive_program_address(&[ROUND], &PROGRAM_ID).0);

pub const RATE_LIMIT: u128 = 200;

pub struct AppRPC {
    helius: HeliusApi,
    connection: RpcClient,
    last_request_at: Instant,
    automation_cache: HashMap<Pubkey, AutomationCache>
}

impl AppRPC {
    pub fn new(rpc_url: String) -> Self {
        let prefix = "https://".to_string();
        let rpc_url = prefix + &rpc_url;
        let helius = HeliusApi::new(rpc_url.clone());
        let connection = RpcClient::new_with_commitment(rpc_url, CommitmentConfig { commitment: CommitmentLevel::Confirmed });
        Self {
            helius,
            connection,
            last_request_at: Instant::now(),
            automation_cache: HashMap::new(),
        }
    }

    pub async fn get_board(&mut self) -> Result<Board> {
        let elapsed_since = self.last_request_at.elapsed().as_millis();
        if  elapsed_since < RATE_LIMIT {
            let time_left = RATE_LIMIT - elapsed_since;
            tokio::time::sleep(Duration::from_millis(time_left as u64)).await;
        }
        self.last_request_at = Instant::now();

        let d = self.connection.get_account_data(&BOARD_ADDRESS).await?;
        match Board::try_from_bytes(&d) {
            Ok(b) => {return Ok(*b)},
            Err(e) => {return Err(e.into())}
        }
    }

    pub async fn get_round(&mut self, round_id: u64) -> Result<Round> {
        let elapsed_since = self.last_request_at.elapsed().as_millis();
        if  elapsed_since < RATE_LIMIT {
            let time_left = RATE_LIMIT - elapsed_since;
            tokio::time::sleep(Duration::from_millis(time_left as u64)).await;
        }
        self.last_request_at = Instant::now();

        let ra = round_pda(round_id).0;
        let d = self.connection.get_account_data(&ra).await?;
        match Round::try_from_bytes(&d) {
            Ok(r) => {return Ok(*r)},
            Err(e) => {return Err(e.into())}
        }
    }

    pub async fn get_treasury(&mut self) -> Result<Treasury> {
        let elapsed_since = self.last_request_at.elapsed().as_millis();
        if  elapsed_since < RATE_LIMIT {
            let time_left = RATE_LIMIT - elapsed_since;
            tokio::time::sleep(Duration::from_millis(time_left as u64)).await;
        }
        self.last_request_at = Instant::now();
        let d = self.connection.get_account_data(&TREASURY_ADDRESS).await?;
        match Treasury::try_from_bytes(&d) {
            Ok(b) => {return Ok(*b)},
            Err(e) => {return Err(e.into())}
        }
    }

    pub async fn get_miners(&mut self) -> Result<Vec<Miner>> {
        let elapsed_since = self.last_request_at.elapsed().as_millis();
        if  elapsed_since < RATE_LIMIT {
            let time_left = RATE_LIMIT - elapsed_since;
            tokio::time::sleep(Duration::from_millis(time_left as u64)).await;
        }
        self.last_request_at = Instant::now();
        let mut miners: Vec<Miner> = vec![];
        match self.connection.get_program_accounts_with_config(
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
                Ok(d) => {
                    for miner_data in d {
                        match Miner::try_from_bytes(&miner_data.1.data) {
                            Ok(m) => {miners.push(m.clone())},
                            Err(e) => {return Err(e.into())}
                        }
                    }
                },
                Err(e) => {
                    return Err(e.into());
                }
        }

        Ok(miners)
    }

    pub async fn reconstruct_round_by_id(&mut self, round_id: u64) -> Result<ReconstructedRound> {
        // Derive the on-chain round PDA
        let (round_pda, _bump) = ore_api::state::round_pda(round_id);
        println!("Round address: {}", round_pda.to_string());

        let mut round = AppRound {
            round_id: round_id as i64,
            winning_square: 100,
            motherlode: 0,
            top_miner: Pubkey::default().to_string(),
            total_deployed: 0,
            total_vaulted: 0,
            total_winnings: 0,
            created_at: Utc::now().timestamp(),
        };

        // ─────────────────────────────────────────────────────────────
        // Stage 1: Collect all Deploys for this round + authority info
        // ─────────────────────────────────────────────────────────────

        let mut all_deploys: Vec<ParsedDeployment> = Vec::new();
        let mut authorities: HashSet<Pubkey> = HashSet::new();
        let mut authority_last_slot: HashMap<Pubkey, u64> = HashMap::new();

        // We’ll approximate the round’s start slot as the minimum slot of all deploys for this round
        let mut round_start_slot_opt: Option<u64> = None;
        let mut reset_event_opt: Option<ResetEvent> = None;
        let mut reset_event_slot: u64 = 0;

        let mut pagination_token: Option<String> = None;

        let mut automation_users: HashSet<Pubkey> = HashSet::new();

        loop {
            let page = self
                .helius
                .get_transactions_for_round(round_id, pagination_token.clone())
                .await?;

            if page.transactions.is_empty() {
                break;
            }

            // This already filters to Deploys that actually target `round_pda`
            let parsed_page = self
                .helius
                .parse_deployments_from_round_page(&round_pda, &page.transactions)?;

            for pd in parsed_page {
                // Track per-authority last deploy slot (in this round)
                authorities.insert(pd.authority);
                authority_last_slot
                    .entry(pd.authority)
                    .and_modify(|s| {
                        if pd.slot > *s {
                            *s = pd.slot;
                        }
                    })
                    .or_insert(pd.slot);

                // Track global minimum slot for this round
                round_start_slot_opt = Some(match round_start_slot_opt {
                    None => pd.slot,
                    Some(current_min) => current_min.min(pd.slot),
                });

                if pd.signer != pd.authority {
                    // maybe used an automation 
                    automation_users.insert(pd.authority);
                }

                all_deploys.push(pd);
            }
                        // ResetEvent (if present) in this page
            if let Some((ev, ev_slot)) = self
                .helius
                .parse_reset_event_from_round_page(round_id, &page.transactions)?
            {
                if ev_slot > reset_event_slot {
                    reset_event_slot = ev_slot;
                    reset_event_opt = Some(ev);
                }
            }

            pagination_token = page.pagination_token;
            if pagination_token.is_none() {
                break;
            }
        }

        // Sort deploys chronologically by slot (oldest -> newest)
        all_deploys.sort_by_key(|pd| pd.slot);

        // If we somehow have no deploys, just return empty round
        let round_start_slot = match round_start_slot_opt {
            Some(s) => s,
            None => {
                if let Some(ev) = reset_event_opt {
                    if ev.winning_square > 25 {
                        round.winning_square = ev.winning_square as i64;
                    } else {
                        round.winning_square = 100;
                    }
                    round.motherlode = ev.motherlode as i64;
                    round.total_deployed = ev.total_deployed as i64;
                    round.total_vaulted = ev.total_vaulted as i64;
                    round.total_winnings = ev.total_winnings as i64;
                    if ev.top_miner != Pubkey::default() {
                        round.top_miner = ev.top_miner.to_string();
                    }
                }
                return Ok(ReconstructedRound {
                    round,
                    deployments: Vec::new(),
                });
            }
        };

        // ─────────────────────────────────────────────────────────────
        // Stage 2:
        // For each authority, find automation state *before this round started*,
        // not during this round.
        //
        // Rule:
        //   - If automation was created/updated in this round, it doesn't apply yet.
        //   - Only Automate txns strictly before `round_start_slot` can affect this round.
        // ─────────────────────────────────────────────────────────────
        println!("Found {} unique miner authorities", authorities.len());

        let mut automation_map: HashMap<Pubkey, Option<ReconstructedAutomation>> =
            HashMap::new();

        for authority in &authorities {
            if let Some(&last_deploy_slot) = authority_last_slot.get(authority) {
                // We only want Automate state with slot < round_start_slot.
                // Also we can't look past the authority's last deploy in this round.
                let mut effective_cutoff_slot = last_deploy_slot;

                if effective_cutoff_slot >= round_start_slot {
                    // We want strictly before the round start
                    effective_cutoff_slot = round_start_slot.saturating_sub(1);
                }

                // If effective_cutoff_slot is now 0, there's no prior history to consider.
                if effective_cutoff_slot == 0 {
                    automation_map.insert(*authority, None);
                    continue;
                }

                // This helper already uses sortOrder="desc" and slot.lte=effective_cutoff_slot,
                // and filters to successful txns only.
                let mut auto = None;
                if let Some(_) = automation_users.get(authority) {
                    let prev_cache = self.automation_cache.get(authority).cloned();
                    let (auto_opt, new_cache) = self
                        .helius
                        .get_latest_automate_for_authority_up_to_slot(authority, effective_cutoff_slot, prev_cache)
                        .await?;

                    auto = auto_opt;

                    self.automation_cache.insert(*authority, new_cache);
                    match &auto {
                        Some(_) => {println!("Found automation for miner: {}", authority.to_string());},
                        None => {}
                    }
                }


                // Convention:
                //   - None  -> last Automate before cutoff was close OR none exists → manual
                //   - Some  -> automation OPEN/configured before round start → automated
                automation_map.insert(*authority, auto);
            }
        }

        // ─────────────────────────────────────────────────────────────
        // Stage 3: Replay deploys; override amount + squares for automated authorities
        // ─────────────────────────────────────────────────────────────

        let mut deployments_map: HashMap<String, AppDeployment> = HashMap::new();

        fn generate_random_mask(num_squares: u64, r: &[u8]) -> [bool; 25] {
            let mut new_mask = [false; 25];
            let mut selected = 0;
            for i in 0..25 {
                let rand_byte = r[i];
                let remaining_needed = num_squares as u64 - selected as u64;
                let remaining_positions = 25 - i;
                if remaining_needed > 0
                    && (rand_byte as u64) * (remaining_positions as u64)
                        < (remaining_needed * 256)
                {
                    new_mask[i] = true;
                    selected += 1;
                }
            }
            new_mask
        }

        for mut pd in all_deploys {
            // Decide if this authority is automated for this round
            let auto_opt = automation_map
                .get(&pd.authority)
                .cloned()
                .flatten(); // Option<ReconstructedAutomation>

            if let Some(auto) = auto_opt {
                // Automation is "open" for this authority for this round:
                //  - override amount_per_square with automation.amount
                //  - derive squares from automation.mask/strategy
                pd.amount_per_square = auto.amount;

                match auto.strategy {
                    AutomationStrategy::Preferred => {
                        // Use automation.mask bits directly
                        let mut squares = [false; 25];
                        for i in 0..25 {
                            squares[i] = (auto.mask & (1 << i)) != 0;
                        }
                        pd.squares = squares;
                    }
                    AutomationStrategy::Random => {
                        // num_squares = lower 8 bits of mask, capped at 25
                        let num_squares = ((auto.mask & 0xFF) as u64).min(25);
                        // r = hash(authority, round.id)
                        let r = hashv(&[
                            &auto.authority.to_bytes(),
                            &round_id.to_le_bytes(),
                        ])
                        .0;
                        pd.squares = generate_random_mask(num_squares, &r);
                    }
                }
            } else {
                // No automation (never existed or last Automate before cutoff was close
                // or was only created this round): treat deploy as manual.
                // Use Deploy's original amount_per_square and squares.
            }

            let squares_mask_true = pd.squares.iter().filter(|&&b| b).count();
            println!(
                "DEPLOY (round_id={}): sig={} authority={} miner={} amount={} squares_mask_true={} \
                 signer_delta={} automation_delta={} round_delta={}",
                round_id,
                pd.signature,
                pd.authority,
                pd.miner,
                pd.amount_per_square,
                squares_mask_true,
                pd.signer_lamports_delta,
                pd.automation_lamports_delta,
                pd.round_lamports_delta,
            );

            // This handles:
            // - ignoring amount=0
            // - skipping deploys that activate 0 new squares
            // - tracking per-miner state & round.total_deployed
            Self::apply_parsed_deployment_to_app(&round_pda, &mut round, &mut deployments_map, &pd);
        }

        // Finalize: only keep miners that actually deployed > 0 in this round
        let deployments: Vec<AppDeployment> = deployments_map
            .into_values()
            .filter(|d| d.total_deployed > 0)
            .collect();

        // ─────────────────────────────
        // Stage 4: apply ResetEvent data to round
        // ─────────────────────────────
        //
        println!("EVENT: \n {:?}", reset_event_opt);
        if let Some(ev) = reset_event_opt {
            // These are authoritative from the on-chain ResetEvent
            if ev.winning_square > 25 {
                round.winning_square = ev.winning_square as i64;
            } else {
                round.winning_square = 100;
            }
            round.motherlode = ev.motherlode as i64;
            round.total_deployed = ev.total_deployed as i64;
            round.total_vaulted = ev.total_vaulted as i64;
            round.total_winnings = ev.total_winnings as i64;

            if ev.top_miner != Pubkey::default() {
                round.top_miner = ev.top_miner.to_string();
            }
        }

        Ok(ReconstructedRound { round, deployments })
    }




    fn get_or_create_deployment<'a>(
        round_id: i64,
        miner_str: &str,
        deployments_map: &'a mut HashMap<String, AppDeployment>,
    ) -> &'a mut AppDeployment {
        deployments_map
            .entry(miner_str.to_string())
            .or_insert_with(|| AppDeployment::new(miner_str.to_string(), round_id))
    }

    pub fn apply_parsed_deployment_to_app(
        expected_round_pda: &Pubkey,
        round: &mut AppRound,
        deployments_map: &mut HashMap<String, AppDeployment>,
        parsed: &ParsedDeployment,
    ) {
        // Only apply deploys that actually target this round PDA
        if parsed.round != *expected_round_pda {
            return;
        }

        // If the decoded amount per square is zero, nothing actually got deployed.
        if parsed.amount_per_square == 0 {
            println!(
                "DEPLOY SKIP (amount=0): sig={} authority={} miner={}",
                parsed.signature,
                parsed.authority,
                parsed.miner,
            );
            return;
        }

        let authority_str = parsed.authority.to_string();
        let app_dep = Self::get_or_create_deployment(round.round_id, &authority_str, deployments_map);

        let mut newly_active_squares = 0u64;

        for (i, &should_deploy) in parsed.squares.iter().enumerate() {
            // Safety: never touch out-of-range indices
            if i >= 25 {
                continue;
            }

            if !should_deploy {
                continue;
            }

            let sq = &mut app_dep.deployments[i];

            // Skip if this miner already deployed to this square earlier in this round
            if sq.amount > 0 {
                continue;
            }

            // First time this miner deploys to this square in this round
            sq.square_id = i as i64;
            sq.slot = parsed.slot as i64;
            sq.amount = parsed.amount_per_square as i64;

            newly_active_squares += 1;
        }

        // If no new squares became active, treat this as a no-op deploy.
        if newly_active_squares == 0 {
            println!(
                "DEPLOY NO-OP (no new squares): sig={} authority={} amount={}",
                parsed.signature,
                parsed.authority,
                parsed.amount_per_square,
            );
            return;
        }

        let added_total = parsed
            .amount_per_square
            .saturating_mul(newly_active_squares);

        // If SOL isn't lining up properly (no lamport movement at all for signer/automation/round),
        // skip counting this deploy.
        if !Self::deployment_has_lamport_movement(parsed) {
            println!(
                "DEPLOY SKIP (no lamport movement): sig={} miner={} added_total={} signerΔ={} autoΔ={} roundΔ={}",
                parsed.signature,
                authority_str,
                added_total,
                parsed.signer_lamports_delta,
                parsed.automation_lamports_delta,
                parsed.round_lamports_delta,
            );
            // rollback square fields we just set? We *could*, but since the on-chain effect
            // was 0 lamports, this is almost certainly a phantom deploy and we prefer to
            // treat it as if it never happened. For safety, we *don't* roll back; we just
            // avoid double-counting in totals, and squares won't be re-used anyway.
            return;
        }

        // Now we are confident this deploy actually moved SOL somewhere; count it.
        app_dep.total_deployed =
            app_dep.total_deployed.saturating_add(added_total as i64);
        round.total_deployed =
            round.total_deployed.saturating_add(added_total as i64);

        println!(
            "APPLY DEPLOY: sig={} miner={} amount_per_square={} new_squares={} added_total={} signerΔ={} autoΔ={} roundΔ={}",
            parsed.signature,
            authority_str,
            parsed.amount_per_square,
            newly_active_squares,
            added_total,
            parsed.signer_lamports_delta,
            parsed.automation_lamports_delta,
            parsed.round_lamports_delta,
        );
    }

    fn deployment_has_lamport_movement(pd: &ParsedDeployment) -> bool {
        pd.signer_lamports_delta != 0
            || pd.automation_lamports_delta != 0
            || pd.round_lamports_delta != 0
    }


    fn deployment_passes_lamport_checks(
        pd: &ParsedDeployment,
        added_total: u64,
    ) -> bool {
        let added = added_total as i64;

        // If nothing was added, skip anyway
        if added == 0 {
            return false;
        }

        // Round must receive lamports
        if pd.round_lamports_delta <= 0 {
            return false;
        }

        // Case 1: Manual deployment (signer pays)
        if pd.signer_lamports_delta < 0 {
            // Signer must lose at least added_total
            if pd.signer_lamports_delta.abs() >= added {
                return true;
            }
        }

        // Case 2: Automation deployment (automation pays)
        if pd.automation_lamports_delta < 0 {
            if pd.automation_lamports_delta.abs() >= added {
                return true;
            }
        }

        // Otherwise: invalid deployment → SKIP
        false
    }




}

