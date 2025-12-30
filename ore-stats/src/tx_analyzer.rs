//! Comprehensive Transaction Analyzer
//!
//! Provides blockchain-explorer-level transaction parsing and analysis.
//! Supports multiple programs including:
//! - System Program
//! - Compute Budget Program
//! - ORE Program
//! - Token Program
//! - Associated Token Program
//! - Memo Program
//! - EVORE Program
//! - Unknown programs (raw data display)

use evore::ore_api::{self, Deploy, OreInstruction};
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use solana_sdk::{bs58, pubkey::Pubkey};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::LazyLock;

// ============================================================================
// Logged Deployment Parsing (from text logs)
// ============================================================================

/// Compiled regex for parsing deploy log messages
/// Format: "Round #101833: deploying 0.000024 SOL to 19 squares"
static DEPLOY_LOG_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Round #(\d+): deploying ([\d.]+) SOL to (\d+) squares")
        .expect("Invalid deploy log regex")
});

/// A deployment parsed from a text log message
#[derive(Debug, Clone, Serialize)]
pub struct LoggedDeployment {
    pub round_id: u64,
    pub amount_per_square_sol: f64,
    pub squares_count: u32,
    pub total_sol: f64,
    pub total_lamports: u64,
    pub round_matches: bool,
    /// Authority (miner identity) from the corresponding parsed deploy instruction
    pub authority: Option<String>,
    /// Whether this logged deployment was matched to a parsed deploy instruction
    /// If false, indicates a parsing failure that needs investigation
    pub matched_parsed: bool,
}

/// Parse "Round #X: deploying Y SOL to Z squares" from logs
/// If expected_round_id is provided, sets round_matches to true only for matching rounds
/// Authority and matched_parsed are initially None/false - must be correlated with parsed deployments afterwards
pub fn parse_deploy_logs(logs: &[String], expected_round_id: Option<u64>) -> Vec<LoggedDeployment> {
    logs.iter()
        .filter_map(|log| {
            DEPLOY_LOG_REGEX.captures(log).and_then(|cap| {
                let round_id: u64 = cap.get(1)?.as_str().parse().ok()?;
                let amount_sol: f64 = cap.get(2)?.as_str().parse().ok()?;
                let squares: u32 = cap.get(3)?.as_str().parse().ok()?;
                let total_sol = amount_sol * squares as f64;
                let total_lamports = (total_sol * 1e9) as u64;
                let round_matches = expected_round_id.map(|e| round_id == e).unwrap_or(true);
                
                Some(LoggedDeployment {
                    round_id,
                    amount_per_square_sol: amount_sol,
                    squares_count: squares,
                    total_sol,
                    total_lamports,
                    round_matches,
                    authority: None,       // Will be correlated with parsed deployments
                    matched_parsed: false, // Will be set true if matched
                })
            })
        })
        .collect()
}

// ============================================================================
// Known Program IDs
// ============================================================================

pub const SYSTEM_PROGRAM_ID: &str = "11111111111111111111111111111111";
pub const COMPUTE_BUDGET_PROGRAM_ID: &str = "ComputeBudget111111111111111111111111111111";
pub const TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const TOKEN_2022_PROGRAM_ID: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
pub const ASSOCIATED_TOKEN_PROGRAM_ID: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";
pub const MEMO_PROGRAM_ID: &str = "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr";
pub const MEMO_PROGRAM_V1_ID: &str = "Memo1UhkJRfHyvLMcVucJwxXeuD728EqVDDwQDxFMNo";
pub const ORE_MINT_PROGRAM_ID: &str = "mintzxW6Kckmeyh1h6Zfdj9QcYgCzhPSGiC8ChZ6fCx";
pub const ENTROPY_PROGRAM_ID: &str = "3jSkUuYBoJzQPMEzTvkDFXCZUBksPamrVhrnHR9igu2X";
pub const EVORE_PROGRAM_ID: &str = "8jaLKWLJAj5jVCZbxpe3zRUvLB3LD48MRtaQ2AjfCfxa";

// ============================================================================
// Response Types
// ============================================================================

#[derive(Debug, Clone, Serialize)]
pub struct FullTransactionAnalysis {
    // Basic info
    pub signature: String,
    pub slot: u64,
    pub block_time: i64,
    pub block_time_formatted: String,
    pub success: bool,
    pub error: Option<String>,
    
    // Fee info
    pub fee: u64,
    pub compute_units_consumed: Option<u64>,
    
    // Accounts
    pub signers: Vec<String>,
    pub writable_accounts: Vec<String>,
    pub readonly_accounts: Vec<String>,
    pub all_accounts: Vec<AccountInfo>,
    
    // Balance changes
    pub balance_changes: Vec<BalanceChange>,
    
    // Programs used
    pub programs_invoked: Vec<ProgramInfo>,
    
    // Instructions
    pub instructions: Vec<InstructionAnalysis>,
    
    // Inner instructions (flattened with parent reference)
    pub inner_instructions: Vec<InnerInstructionGroup>,
    
    // Logs
    pub logs: Vec<String>,
    
    // ORE-specific analysis
    pub ore_analysis: Option<OreTransactionAnalysis>,
    
    // Summary
    pub summary: TransactionSummaryInfo,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccountInfo {
    pub index: usize,
    pub pubkey: String,
    pub is_signer: bool,
    pub is_writable: bool,
    pub is_program: bool,
    pub program_name: Option<String>,
    pub pre_balance: u64,
    pub post_balance: u64,
    pub balance_change: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BalanceChange {
    pub account: String,
    pub pre_balance: u64,
    pub post_balance: u64,
    pub change: i64,
    pub change_sol: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProgramInfo {
    pub pubkey: String,
    pub name: String,
    pub invocation_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstructionAnalysis {
    pub index: usize,
    pub program_id: String,
    pub program_name: String,
    pub instruction_type: String,
    pub accounts: Vec<InstructionAccount>,
    pub data_base58: String,
    pub data_hex: String,
    pub data_length: usize,
    pub parsed: Option<ParsedInstruction>,
    pub parse_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstructionAccount {
    pub index: usize,
    pub pubkey: String,
    pub is_signer: bool,
    pub is_writable: bool,
    pub role: Option<String>, // e.g., "source", "destination", "authority"
}

#[derive(Debug, Clone, Serialize)]
pub struct InnerInstructionGroup {
    pub parent_index: usize,
    pub instructions: Vec<InstructionAnalysis>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ParsedInstruction {
    // System Program
    SystemTransfer {
        from: String,
        to: String,
        lamports: u64,
        sol: f64,
    },
    SystemCreateAccount {
        from: String,
        new_account: String,
        lamports: u64,
        space: u64,
        owner: String,
    },
    SystemAssign {
        account: String,
        owner: String,
    },
    SystemAllocate {
        account: String,
        space: u64,
    },
    SystemAdvanceNonceAccount {
        nonce_account: String,
        nonce_authority: String,
    },
    
    // Compute Budget
    ComputeSetLimit {
        units: u32,
    },
    ComputeSetPrice {
        micro_lamports: u64,
    },
    ComputeRequestHeapFrame {
        bytes: u32,
    },
    
    // ORE Program
    // Deploy accounts: [signer, authority, automation, board, config, miner, round, system_program, ore_program, entropy_var, entropy_program]
    OreDeploy {
        signer: String,
        authority: String,
        automation_pda: String,
        board: String,
        config: String,
        miner: String,
        round: String,
        round_id: Option<u64>, // Derived round ID from PDA if matches expected
        amount_per_square: u64,
        amount_sol: f64,
        squares_mask: u32,
        squares: Vec<u8>,
        total_lamports: u64,
        total_sol: f64,
    },
    // Checkpoint accounts: [signer, board, miner, round, treasury, system_program]
    OreCheckpoint {
        signer: String,
        board: String,
        miner: String,
        round: String,
        round_id: Option<u64>, // Previous round being checkpointed
        treasury: String,
    },
    // ClaimSOL accounts: [signer, miner, system_program]
    OreClaimSOL {
        signer: String,
        miner: String,
    },
    // ClaimORE accounts: [signer, miner, mint, recipient_tokens, treasury, treasury_tokens, system_program, token_program, ata_program]
    OreClaimORE {
        signer: String,
        miner: String,
        mint: String,
        recipient_tokens: String,
        treasury: String,
    },
    // Reset accounts: [signer, board, config, fee_collector, mint, round, round_next, top_miner, treasury, treasury_tokens, ...]
    OreReset {
        signer: String,
        board: String,
        config: String,
        fee_collector: String,
        round: String,
        round_next: String,
        top_miner: String,
        treasury: String,
    },
    // Automate accounts: [signer, automation, executor, miner, system_program]
    OreAutomate {
        signer: String,
        automation_pda: String,
        executor: String,
        miner: String,
    },
    OreLog {
        // Log instruction - event data
        event_type: String,
        data_hex: String,
    },
    OreOther {
        instruction_tag: u8,
        instruction_name: String,
        accounts_count: usize,
    },
    
    // Token Program
    TokenTransfer {
        source: String,
        destination: String,
        authority: String,
        amount: u64,
    },
    TokenTransferChecked {
        source: String,
        destination: String,
        mint: String,
        authority: String,
        amount: u64,
        decimals: u8,
    },
    TokenInitializeAccount {
        account: String,
        mint: String,
        owner: String,
    },
    TokenApprove {
        source: String,
        delegate: String,
        owner: String,
        amount: u64,
    },
    TokenMintTo {
        mint: String,
        destination: String,
        authority: String,
        amount: u64,
    },
    TokenBurn {
        account: String,
        mint: String,
        authority: String,
        amount: u64,
    },
    TokenCloseAccount {
        account: String,
        destination: String,
        authority: String,
    },
    
    // ATA
    AtaCreate {
        payer: String,
        associated_token: String,
        wallet: String,
        mint: String,
    },
    
    // Memo
    Memo {
        message: String,
    },
    
    // Unknown
    Unknown {
        program: String,
        data_preview: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct OreTransactionAnalysis {
    pub has_ore_instructions: bool,
    pub deploy_count: usize,
    pub reset_count: usize,
    pub log_count: usize,
    pub other_count: usize,
    pub deployments: Vec<OreDeploymentInfo>,
    pub total_deployed_lamports: u64,
    pub total_deployed_sol: f64,
    
    // Logged totals from text logs (parsed from "Round #X: deploying Y SOL to Z squares")
    pub logged_deployments: Vec<LoggedDeployment>,
    pub logged_deploy_count: usize,
    pub logged_deployed_lamports: u64,
    pub logged_deployed_sol: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OreDeploymentInfo {
    pub instruction_index: usize,
    pub is_inner: bool,
    pub signer: String,
    pub authority: String,
    pub miner: String,
    pub round: String,
    pub round_id: Option<u64>,
    pub expected_round_id: Option<u64>,
    pub round_matches: bool,
    pub amount_per_square: u64,
    pub squares: Vec<u8>,
    pub total_lamports: u64,
    pub total_sol: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransactionSummaryInfo {
    pub total_instructions: usize,
    pub total_inner_instructions: usize,
    pub programs_count: usize,
    pub has_failed: bool,
    pub is_ore_transaction: bool,
    pub is_deploy_transaction: bool,
    pub primary_action: String,
}

// ============================================================================
// Analyzer Implementation
// ============================================================================


pub struct TransactionAnalyzer {
    expected_round_pda: Option<Pubkey>,
    expected_round_id: Option<u64>,
}

impl TransactionAnalyzer {
    pub fn new() -> Self {
        Self {
            expected_round_pda: None,
            expected_round_id: None,
        }
    }
    
    pub fn with_expected_round(mut self, round_id: u64) -> Self {
        let (pda, _) = ore_api::round_pda(round_id);
        self.expected_round_pda = Some(pda);
        self.expected_round_id = Some(round_id);
        self
    }
    
    /// Check if a round PDA matches our expected round, returning the round_id if it matches
    fn check_round_pda(&self, round_pda: &Pubkey) -> Option<u64> {
        match (&self.expected_round_pda, self.expected_round_id) {
            (Some(expected_pda), Some(expected_id)) if expected_pda == round_pda => {
                Some(expected_id)
            }
            _ => None,
        }
    }
    
    pub fn analyze(&self, raw_json: &str) -> Result<FullTransactionAnalysis, String> {
        let tx: Value = serde_json::from_str(raw_json)
            .map_err(|e| format!("JSON parse error: {}", e))?;
        
        self.analyze_value(&tx)
    }
    
    pub fn analyze_value(&self, tx: &Value) -> Result<FullTransactionAnalysis, String> {
        let analyze_start = std::time::Instant::now();
        
        // Basic info
        let signature = tx.get("transaction")
            .and_then(|t| t.get("signatures"))
            .and_then(|s| s.as_array())
            .and_then(|a| a.first())
            .and_then(|s| s.as_str())
            .unwrap_or("unknown")
            .to_string();
        
        tracing::debug!(
            signature = %signature,
            "analyze_value: START"
        );
        
        let slot = tx.get("slot").and_then(|s| s.as_u64()).unwrap_or(0);
        let block_time = tx.get("blockTime").and_then(|b| b.as_i64()).unwrap_or(0);
        let block_time_formatted = if block_time > 0 {
            chrono::DateTime::from_timestamp(block_time, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                .unwrap_or_else(|| "Invalid".to_string())
        } else {
            "Unknown".to_string()
        };
        
        tracing::trace!(
            signature = %signature,
            elapsed_us = analyze_start.elapsed().as_micros(),
            "analyze_value: parsed basic info (slot, block_time)"
        );
        
        // Meta
        let meta = tx.get("meta").ok_or("Missing meta")?;
        
        // Success/error
        let err = meta.get("err");
        let success = err.map_or(true, |e| e.is_null());
        let error = if !success {
            Some(format!("{:?}", err))
        } else {
            None
        };
        
        // Fee
        let fee = meta.get("fee").and_then(|f| f.as_u64()).unwrap_or(0);
        let compute_units_consumed = meta.get("computeUnitsConsumed").and_then(|c| c.as_u64());
        
        // Logs
        let logs: Vec<String> = meta.get("logMessages")
            .and_then(|l| l.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        
        // Account keys
        let message = tx.get("transaction")
            .and_then(|t| t.get("message"))
            .ok_or("Missing message")?;
        
        let account_keys_json = message.get("accountKeys")
            .and_then(|k| k.as_array())
            .ok_or("Missing accountKeys")?;
        
        let mut account_keys: Vec<Pubkey> = Vec::new();
        for key_val in account_keys_json {
            let key_str = key_val.as_str().ok_or("Account key not a string")?;
            let pk = Pubkey::from_str(key_str)
                .map_err(|e| format!("Invalid pubkey {}: {}", key_str, e))?;
            account_keys.push(pk);
        }
        
        // Pre/post balances
        let pre_balances: Vec<u64> = meta.get("preBalances")
            .and_then(|p| p.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect())
            .unwrap_or_default();
        
        let post_balances: Vec<u64> = meta.get("postBalances")
            .and_then(|p| p.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect())
            .unwrap_or_default();
        
        // Header info for signer/writable detection
        let header = message.get("header");
        let num_required_signatures = header
            .and_then(|h| h.get("numRequiredSignatures"))
            .and_then(|n| n.as_u64())
            .unwrap_or(1) as usize;
        let num_readonly_signed = header
            .and_then(|h| h.get("numReadonlySignedAccounts"))
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as usize;
        let num_readonly_unsigned = header
            .and_then(|h| h.get("numReadonlyUnsignedAccounts"))
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as usize;
        
        // Build account info list
        let mut all_accounts: Vec<AccountInfo> = Vec::new();
        let mut signers: Vec<String> = Vec::new();
        let mut writable_accounts: Vec<String> = Vec::new();
        let mut readonly_accounts: Vec<String> = Vec::new();
        let mut programs_map: HashMap<String, usize> = HashMap::new();
        
        for (i, pk) in account_keys.iter().enumerate() {
            let is_signer = i < num_required_signatures;
            let is_readonly = if is_signer {
                i >= num_required_signatures - num_readonly_signed
            } else {
                i >= account_keys.len() - num_readonly_unsigned
            };
            let is_writable = !is_readonly;
            
            let pre = pre_balances.get(i).copied().unwrap_or(0);
            let post = post_balances.get(i).copied().unwrap_or(0);
            let change = post as i64 - pre as i64;
            
            let pk_str = pk.to_string();
            let program_name = self.identify_program(&pk_str);
            let is_program = program_name.is_some();
            
            all_accounts.push(AccountInfo {
                index: i,
                pubkey: pk_str.clone(),
                is_signer,
                is_writable,
                is_program,
                program_name: program_name.clone(),
                pre_balance: pre,
                post_balance: post,
                balance_change: change,
            });
            
            if is_signer {
                signers.push(pk_str.clone());
            }
            if is_writable {
                writable_accounts.push(pk_str.clone());
            } else {
                readonly_accounts.push(pk_str.clone());
            }
        }
        
        // Balance changes (only non-zero)
        let balance_changes: Vec<BalanceChange> = all_accounts.iter()
            .filter(|a| a.balance_change != 0)
            .map(|a| BalanceChange {
                account: a.pubkey.clone(),
                pre_balance: a.pre_balance,
                post_balance: a.post_balance,
                change: a.balance_change,
                change_sol: a.balance_change as f64 / 1e9,
            })
            .collect();
        
        tracing::trace!(
            signature = %signature,
            accounts_count = all_accounts.len(),
            elapsed_us = analyze_start.elapsed().as_micros(),
            "analyze_value: parsed accounts and balances"
        );
        
        // Parse instructions
        let instruction_parse_start = std::time::Instant::now();
        let mut instructions: Vec<InstructionAnalysis> = Vec::new();
        let mut ore_deployments: Vec<OreDeploymentInfo> = Vec::new();
        let mut ore_reset_count = 0;
        let mut ore_log_count = 0;
        let mut ore_other_count = 0;
        
        if let Some(ixs) = message.get("instructions").and_then(|i| i.as_array()) {
            for (ix_idx, ix) in ixs.iter().enumerate() {
                let analysis = self.analyze_instruction(ix, &account_keys, ix_idx, false)?;
                
                // Track programs
                *programs_map.entry(analysis.program_id.clone()).or_insert(0) += 1;
                
                // Track ORE specifics
                if let Some(parsed) = &analysis.parsed {
                    match parsed {
                        ParsedInstruction::OreDeploy { signer, authority, miner, round, round_id, amount_per_square, squares, .. } => {
                            // Compare using round_id if available, otherwise compare PDAs
                            let matches = match (*round_id, self.expected_round_id) {
                                (Some(deploy_round), Some(expected)) => deploy_round == expected,
                                _ => self.expected_round_pda
                                    .map(|expected| round == &expected.to_string())
                                    .unwrap_or(true),
                            };
                            let total = *amount_per_square * squares.len() as u64;
                            ore_deployments.push(OreDeploymentInfo {
                                instruction_index: ix_idx,
                                is_inner: false,
                                signer: signer.clone(),
                                authority: authority.clone(),
                                miner: miner.clone(),
                                round: round.clone(),
                                round_id: *round_id,
                                expected_round_id: self.expected_round_id,
                                round_matches: matches,
                                amount_per_square: *amount_per_square,
                                squares: squares.clone(),
                                total_lamports: total,
                                total_sol: total as f64 / 1e9,
                            });
                        }
                        ParsedInstruction::OreReset { .. } => ore_reset_count += 1,
                        ParsedInstruction::OreLog { .. } => ore_log_count += 1,
                        ParsedInstruction::OreOther { .. } => ore_other_count += 1,
                        _ => {}
                    }
                }
                
                instructions.push(analysis);
            }
        }
        
        tracing::trace!(
            signature = %signature,
            instruction_count = instructions.len(),
            elapsed_us = instruction_parse_start.elapsed().as_micros(),
            total_elapsed_us = analyze_start.elapsed().as_micros(),
            "analyze_value: parsed top-level instructions"
        );
        
        // Inner instructions
        let inner_parse_start = std::time::Instant::now();
        let mut inner_instructions: Vec<InnerInstructionGroup> = Vec::new();
        if let Some(inner_arr) = meta.get("innerInstructions").and_then(|i| i.as_array()) {
            for inner in inner_arr {
                let parent_idx = inner.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                let mut group_instructions: Vec<InstructionAnalysis> = Vec::new();
                
                if let Some(inner_ixs) = inner.get("instructions").and_then(|i| i.as_array()) {
                    for (ix_idx, ix) in inner_ixs.iter().enumerate() {
                        let analysis = self.analyze_instruction(ix, &account_keys, ix_idx, true)?;
                        
                        *programs_map.entry(analysis.program_id.clone()).or_insert(0) += 1;
                        
                        // Track inner ORE deployments
                        if let Some(parsed) = &analysis.parsed {
                            match parsed {
                                ParsedInstruction::OreDeploy { signer, authority, miner, round, round_id, amount_per_square, squares, .. } => {
                                    // Compare using round_id if available
                                    let matches = match (round_id, self.expected_round_id) {
                                        (Some(deploy_round), Some(expected)) => *deploy_round == expected,
                                        _ => self.expected_round_pda
                                            .map(|expected| round == &expected.to_string())
                                            .unwrap_or(true),
                                    };
                                    let total = *amount_per_square * squares.len() as u64;
                                    ore_deployments.push(OreDeploymentInfo {
                                        instruction_index: ix_idx,
                                        is_inner: true,
                                        signer: signer.clone(),
                                        authority: authority.clone(),
                                        miner: miner.clone(),
                                        round: round.clone(),
                                        round_id: *round_id,
                                        expected_round_id: self.expected_round_id,
                                        round_matches: matches,
                                        amount_per_square: *amount_per_square,
                                        squares: squares.clone(),
                                        total_lamports: total,
                                        total_sol: total as f64 / 1e9,
                                    });
                                }
                                ParsedInstruction::OreReset { .. } => ore_reset_count += 1,
                                ParsedInstruction::OreLog { .. } => ore_log_count += 1,
                                ParsedInstruction::OreOther { .. } => ore_other_count += 1,
                                _ => {}
                            }
                        }
                        
                        group_instructions.push(analysis);
                    }
                }
                
                inner_instructions.push(InnerInstructionGroup {
                    parent_index: parent_idx,
                    instructions: group_instructions,
                });
            }
        }
        
        let inner_ix_count: usize = inner_instructions.iter().map(|g| g.instructions.len()).sum();
        tracing::trace!(
            signature = %signature,
            inner_groups = inner_instructions.len(),
            inner_instruction_count = inner_ix_count,
            elapsed_us = inner_parse_start.elapsed().as_micros(),
            total_elapsed_us = analyze_start.elapsed().as_micros(),
            "analyze_value: parsed inner instructions"
        );
        
        // Build programs list
        let programs_invoked: Vec<ProgramInfo> = programs_map.into_iter()
            .map(|(pubkey, count)| ProgramInfo {
                name: self.identify_program(&pubkey).unwrap_or_else(|| "Unknown".to_string()),
                pubkey,
                invocation_count: count,
            })
            .collect();
        
        // ORE analysis
        let ore_deploy_count = ore_deployments.len();
        let has_ore = ore_deploy_count + ore_reset_count + ore_log_count + ore_other_count > 0;
        let total_deployed: u64 = ore_deployments.iter().map(|d| d.total_lamports).sum();
        
        // Parse logged deployments from text logs, filtering by expected round
        let mut logged_deployments = parse_deploy_logs(&logs, self.expected_round_id);
        
        // Correlate logged deployments with parsed deployments to get authority info
        // Match by round_id - find a parsed deployment with matching round for each logged one
        // Track which parsed deployments have been matched to avoid double-matching
        let mut parsed_used: Vec<bool> = vec![false; ore_deployments.len()];
        
        for logged in logged_deployments.iter_mut() {
            // Find a parsed deployment with matching round_id that hasn't been matched yet
            for (idx, parsed) in ore_deployments.iter().enumerate() {
                if !parsed_used[idx] {
                    // Match by round_id if available
                    let rounds_match = match parsed.round_id {
                        Some(parsed_round) => parsed_round == logged.round_id,
                        None => false, // Can't match without round_id
                    };
                    
                    if rounds_match {
                        logged.authority = Some(parsed.authority.clone());
                        logged.matched_parsed = true;
                        parsed_used[idx] = true;
                        break;
                    }
                }
            }
        }
        
        let logged_deploy_count = logged_deployments.iter().filter(|d| d.round_matches).count();
        let logged_deployed_lamports: u64 = logged_deployments
            .iter()
            .filter(|d| d.round_matches)
            .map(|d| d.total_lamports)
            .sum();
        let logged_deployed_sol = logged_deployed_lamports as f64 / 1e9;
        
        let ore_analysis = if has_ore || logged_deploy_count > 0 {
            Some(OreTransactionAnalysis {
                has_ore_instructions: has_ore,
                deploy_count: ore_deploy_count,
                reset_count: ore_reset_count,
                log_count: ore_log_count,
                other_count: ore_other_count,
                deployments: ore_deployments,
                total_deployed_lamports: total_deployed,
                total_deployed_sol: total_deployed as f64 / 1e9,
                logged_deployments,
                logged_deploy_count,
                logged_deployed_lamports,
                logged_deployed_sol,
            })
        } else {
            None
        };
        
        // Summary
        let total_inner: usize = inner_instructions.iter().map(|g| g.instructions.len()).sum();
        let primary_action = self.determine_primary_action(&instructions, ore_analysis.as_ref());
        
        let summary = TransactionSummaryInfo {
            total_instructions: instructions.len(),
            total_inner_instructions: total_inner,
            programs_count: programs_invoked.len(),
            has_failed: !success,
            is_ore_transaction: has_ore,
            is_deploy_transaction: ore_analysis.as_ref().map(|a| a.deploy_count > 0).unwrap_or(false),
            primary_action,
        };
        
        let total_elapsed_ms = analyze_start.elapsed().as_millis();
        if total_elapsed_ms > 100 {
            tracing::warn!(
                signature = %signature,
                elapsed_ms = total_elapsed_ms,
                instructions = instructions.len(),
                inner_instructions = total_inner,
                ore_deployments = ore_deploy_count,
                "analyze_value: SLOW TRANSACTION (>100ms)"
            );
        } else {
            tracing::debug!(
                signature = %signature,
                elapsed_ms = total_elapsed_ms,
                instructions = instructions.len(),
                inner_instructions = total_inner,
                ore_deployments = ore_deploy_count,
                "analyze_value: COMPLETE"
            );
        }
        
        Ok(FullTransactionAnalysis {
            signature,
            slot,
            block_time,
            block_time_formatted,
            success,
            error,
            fee,
            compute_units_consumed,
            signers,
            writable_accounts,
            readonly_accounts,
            all_accounts,
            balance_changes,
            programs_invoked,
            instructions,
            inner_instructions,
            logs,
            ore_analysis,
            summary,
        })
    }
    
    fn identify_program(&self, pubkey: &str) -> Option<String> {
        match pubkey {
            SYSTEM_PROGRAM_ID => Some("System Program".to_string()),
            COMPUTE_BUDGET_PROGRAM_ID => Some("Compute Budget".to_string()),
            TOKEN_PROGRAM_ID => Some("Token Program".to_string()),
            TOKEN_2022_PROGRAM_ID => Some("Token-2022".to_string()),
            ASSOCIATED_TOKEN_PROGRAM_ID => Some("Associated Token".to_string()),
            MEMO_PROGRAM_ID | MEMO_PROGRAM_V1_ID => Some("Memo".to_string()),
            s if s == evore::ore_api::PROGRAM_ID.to_string() => Some("ORE Program".to_string()),
            EVORE_PROGRAM_ID => Some("EVORE Program".to_string()),
            ENTROPY_PROGRAM_ID => Some("Entropy Program".to_string()),
            ORE_MINT_PROGRAM_ID => Some("ORE Mint Program".to_string()),
            _ => None,
        }
    }
    
    fn analyze_instruction(
        &self,
        ix: &Value,
        account_keys: &[Pubkey],
        ix_idx: usize,
        is_inner: bool,
    ) -> Result<InstructionAnalysis, String> {
        let ix_start = std::time::Instant::now();
        
        let program_id_index = ix.get("programIdIndex")
            .and_then(|p| p.as_u64())
            .ok_or("Missing programIdIndex")? as usize;
        
        let program_id = account_keys.get(program_id_index)
            .ok_or("programIdIndex out of range")?;
        let program_id_str = program_id.to_string();
        let program_name = self.identify_program(&program_id_str)
            .unwrap_or_else(|| format!("Unknown ({}...)", &program_id_str[..8]));
        
        tracing::trace!(
            ix_idx = ix_idx,
            is_inner = is_inner,
            program = %program_name,
            "analyze_instruction: START"
        );
        
        // Get accounts for this instruction
        let accounts_arr = ix.get("accounts")
            .and_then(|a| a.as_array())
            .ok_or("Missing accounts")?;
        
        let mut accounts: Vec<InstructionAccount> = Vec::new();
        for (i, acc_idx_val) in accounts_arr.iter().enumerate() {
            let acc_idx = acc_idx_val.as_u64().ok_or("Invalid account index")? as usize;
            let pk = account_keys.get(acc_idx).ok_or("Account index out of range")?;
            accounts.push(InstructionAccount {
                index: acc_idx,
                pubkey: pk.to_string(),
                is_signer: i == 0, // First account is usually the signer
                is_writable: false,
                role: None,
            });
        }
        
        // Get data
        let data_str = ix.get("data")
            .and_then(|d| d.as_str())
            .ok_or("Missing data")?;
        
        let data = bs58::decode(data_str).into_vec()
            .map_err(|e| format!("Base58 decode error: {}", e))?;
        
        // Add account roles based on program and instruction type
        self.add_account_roles(&program_id_str, &data, &mut accounts);
        
        let data_hex = hex::encode(&data);
        
        // Parse based on program
        let (instruction_type, parsed, parse_error) = self.parse_instruction(
            &program_id_str,
            &data,
            &accounts,
            account_keys,
        );
        
        let ix_elapsed_us = ix_start.elapsed().as_micros();
        if ix_elapsed_us > 1000 { // > 1ms is slow for a single instruction
            tracing::debug!(
                ix_idx = ix_idx,
                is_inner = is_inner,
                program = %program_name,
                instruction_type = %instruction_type,
                elapsed_us = ix_elapsed_us,
                "analyze_instruction: SLOW instruction (>1ms)"
            );
        } else {
            tracing::trace!(
                ix_idx = ix_idx,
                is_inner = is_inner,
                program = %program_name,
                instruction_type = %instruction_type,
                elapsed_us = ix_elapsed_us,
                "analyze_instruction: COMPLETE"
            );
        }
        
        Ok(InstructionAnalysis {
            index: ix_idx,
            program_id: program_id_str,
            program_name,
            instruction_type,
            accounts,
            data_base58: data_str.to_string(),
            data_hex,
            data_length: data.len(),
            parsed,
            parse_error,
        })
    }
    
    fn parse_instruction(
        &self,
        program_id: &str,
        data: &[u8],
        accounts: &[InstructionAccount],
        account_keys: &[Pubkey],
    ) -> (String, Option<ParsedInstruction>, Option<String>) {
        match program_id {
            SYSTEM_PROGRAM_ID => self.parse_system_instruction(data, accounts),
            COMPUTE_BUDGET_PROGRAM_ID => self.parse_compute_budget_instruction(data),
            TOKEN_PROGRAM_ID | TOKEN_2022_PROGRAM_ID => self.parse_token_instruction(data, accounts),
            ASSOCIATED_TOKEN_PROGRAM_ID => self.parse_ata_instruction(data, accounts),
            MEMO_PROGRAM_ID | MEMO_PROGRAM_V1_ID => self.parse_memo_instruction(data),
            s if s == evore::ore_api::PROGRAM_ID.to_string() => {
                self.parse_ore_instruction(data, accounts, account_keys)
            }
            EVORE_PROGRAM_ID => self.parse_evore_instruction(data, accounts),
            ENTROPY_PROGRAM_ID => self.parse_entropy_instruction(data, accounts),
            ORE_MINT_PROGRAM_ID => self.parse_ore_mint_instruction(data, accounts),
            _ => (
                "Unknown".to_string(),
                Some(ParsedInstruction::Unknown {
                    program: program_id.to_string(),
                    data_preview: if data.len() > 32 {
                        format!("{}...", hex::encode(&data[..32]))
                    } else {
                        hex::encode(data)
                    },
                }),
                None,
            ),
        }
    }
    
    fn parse_system_instruction(
        &self,
        data: &[u8],
        accounts: &[InstructionAccount],
    ) -> (String, Option<ParsedInstruction>, Option<String>) {
        if data.is_empty() {
            return ("Empty".to_string(), None, Some("Empty instruction data".to_string()));
        }
        
        // System instruction tag is first 4 bytes as u32
        if data.len() < 4 {
            return ("Invalid".to_string(), None, Some("Data too short".to_string()));
        }
        
        let tag = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        
        match tag {
            // Transfer = 2
            2 => {
                if data.len() < 12 || accounts.len() < 2 {
                    return ("Transfer".to_string(), None, Some("Invalid transfer data".to_string()));
                }
                let lamports = u64::from_le_bytes(data[4..12].try_into().unwrap());
                (
                    "Transfer".to_string(),
                    Some(ParsedInstruction::SystemTransfer {
                        from: accounts.get(0).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        to: accounts.get(1).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        lamports,
                        sol: lamports as f64 / 1e9,
                    }),
                    None,
                )
            }
            // CreateAccount = 0
            0 => {
                if data.len() < 52 || accounts.len() < 2 {
                    return ("CreateAccount".to_string(), None, Some("Invalid data".to_string()));
                }
                let lamports = u64::from_le_bytes(data[4..12].try_into().unwrap());
                let space = u64::from_le_bytes(data[12..20].try_into().unwrap());
                let owner = Pubkey::try_from(&data[20..52]).ok()
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "Invalid".to_string());
                (
                    "CreateAccount".to_string(),
                    Some(ParsedInstruction::SystemCreateAccount {
                        from: accounts.get(0).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        new_account: accounts.get(1).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        lamports,
                        space,
                        owner,
                    }),
                    None,
                )
            }
            // Assign = 1
            1 => {
                if data.len() < 36 || accounts.is_empty() {
                    return ("Assign".to_string(), None, Some("Invalid data".to_string()));
                }
                let owner = Pubkey::try_from(&data[4..36]).ok()
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "Invalid".to_string());
                (
                    "Assign".to_string(),
                    Some(ParsedInstruction::SystemAssign {
                        account: accounts.get(0).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        owner,
                    }),
                    None,
                )
            }
            // Allocate = 8
            8 => {
                if data.len() < 12 || accounts.is_empty() {
                    return ("Allocate".to_string(), None, Some("Invalid data".to_string()));
                }
                let space = u64::from_le_bytes(data[4..12].try_into().unwrap());
                (
                    "Allocate".to_string(),
                    Some(ParsedInstruction::SystemAllocate {
                        account: accounts.get(0).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        space,
                    }),
                    None,
                )
            }
            // AdvanceNonceAccount = 4
            4 => {
                (
                    "AdvanceNonce".to_string(),
                    Some(ParsedInstruction::SystemAdvanceNonceAccount {
                        nonce_account: accounts.get(0).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        nonce_authority: accounts.get(2).map(|a| a.pubkey.clone()).unwrap_or_default(),
                    }),
                    None,
                )
            }
            _ => (
                format!("Unknown({})", tag),
                None,
                Some(format!("Unknown system instruction tag: {}", tag)),
            ),
        }
    }
    
    fn parse_compute_budget_instruction(
        &self,
        data: &[u8],
    ) -> (String, Option<ParsedInstruction>, Option<String>) {
        if data.is_empty() {
            return ("Empty".to_string(), None, Some("Empty data".to_string()));
        }
        
        match data[0] {
            // SetComputeUnitLimit = 2
            2 => {
                if data.len() < 5 {
                    return ("SetLimit".to_string(), None, Some("Invalid data".to_string()));
                }
                let units = u32::from_le_bytes(data[1..5].try_into().unwrap());
                (
                    "SetComputeUnitLimit".to_string(),
                    Some(ParsedInstruction::ComputeSetLimit { units }),
                    None,
                )
            }
            // SetComputeUnitPrice = 3
            3 => {
                if data.len() < 9 {
                    return ("SetPrice".to_string(), None, Some("Invalid data".to_string()));
                }
                let micro_lamports = u64::from_le_bytes(data[1..9].try_into().unwrap());
                (
                    "SetComputeUnitPrice".to_string(),
                    Some(ParsedInstruction::ComputeSetPrice { micro_lamports }),
                    None,
                )
            }
            // RequestHeapFrame = 1
            1 => {
                if data.len() < 5 {
                    return ("RequestHeap".to_string(), None, Some("Invalid data".to_string()));
                }
                let bytes = u32::from_le_bytes(data[1..5].try_into().unwrap());
                (
                    "RequestHeapFrame".to_string(),
                    Some(ParsedInstruction::ComputeRequestHeapFrame { bytes }),
                    None,
                )
            }
            tag => (
                format!("Unknown({})", tag),
                None,
                Some(format!("Unknown compute budget tag: {}", tag)),
            ),
        }
    }
    
    fn parse_token_instruction(
        &self,
        data: &[u8],
        accounts: &[InstructionAccount],
    ) -> (String, Option<ParsedInstruction>, Option<String>) {
        if data.is_empty() {
            return ("Empty".to_string(), None, Some("Empty data".to_string()));
        }
        
        match data[0] {
            // Transfer = 3
            3 => {
                if data.len() < 9 || accounts.len() < 3 {
                    return ("Transfer".to_string(), None, Some("Invalid data".to_string()));
                }
                let amount = u64::from_le_bytes(data[1..9].try_into().unwrap());
                (
                    "Transfer".to_string(),
                    Some(ParsedInstruction::TokenTransfer {
                        source: accounts.get(0).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        destination: accounts.get(1).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        authority: accounts.get(2).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        amount,
                    }),
                    None,
                )
            }
            // TransferChecked = 12
            12 => {
                if data.len() < 10 || accounts.len() < 4 {
                    return ("TransferChecked".to_string(), None, Some("Invalid data".to_string()));
                }
                let amount = u64::from_le_bytes(data[1..9].try_into().unwrap());
                let decimals = data[9];
                (
                    "TransferChecked".to_string(),
                    Some(ParsedInstruction::TokenTransferChecked {
                        source: accounts.get(0).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        mint: accounts.get(1).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        destination: accounts.get(2).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        authority: accounts.get(3).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        amount,
                        decimals,
                    }),
                    None,
                )
            }
            // InitializeAccount = 1
            1 => {
                (
                    "InitializeAccount".to_string(),
                    Some(ParsedInstruction::TokenInitializeAccount {
                        account: accounts.get(0).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        mint: accounts.get(1).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        owner: accounts.get(2).map(|a| a.pubkey.clone()).unwrap_or_default(),
                    }),
                    None,
                )
            }
            // Approve = 4
            4 => {
                if data.len() < 9 || accounts.len() < 3 {
                    return ("Approve".to_string(), None, Some("Invalid data".to_string()));
                }
                let amount = u64::from_le_bytes(data[1..9].try_into().unwrap());
                (
                    "Approve".to_string(),
                    Some(ParsedInstruction::TokenApprove {
                        source: accounts.get(0).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        delegate: accounts.get(1).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        owner: accounts.get(2).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        amount,
                    }),
                    None,
                )
            }
            // MintTo = 7
            7 => {
                if data.len() < 9 || accounts.len() < 3 {
                    return ("MintTo".to_string(), None, Some("Invalid data".to_string()));
                }
                let amount = u64::from_le_bytes(data[1..9].try_into().unwrap());
                (
                    "MintTo".to_string(),
                    Some(ParsedInstruction::TokenMintTo {
                        mint: accounts.get(0).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        destination: accounts.get(1).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        authority: accounts.get(2).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        amount,
                    }),
                    None,
                )
            }
            // Burn = 8
            8 => {
                if data.len() < 9 || accounts.len() < 3 {
                    return ("Burn".to_string(), None, Some("Invalid data".to_string()));
                }
                let amount = u64::from_le_bytes(data[1..9].try_into().unwrap());
                (
                    "Burn".to_string(),
                    Some(ParsedInstruction::TokenBurn {
                        account: accounts.get(0).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        mint: accounts.get(1).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        authority: accounts.get(2).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        amount,
                    }),
                    None,
                )
            }
            // CloseAccount = 9
            9 => {
                (
                    "CloseAccount".to_string(),
                    Some(ParsedInstruction::TokenCloseAccount {
                        account: accounts.get(0).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        destination: accounts.get(1).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        authority: accounts.get(2).map(|a| a.pubkey.clone()).unwrap_or_default(),
                    }),
                    None,
                )
            }
            tag => (
                format!("Token({})", tag),
                None,
                Some(format!("Unhandled token instruction: {}", tag)),
            ),
        }
    }
    
    fn parse_ata_instruction(
        &self,
        _data: &[u8],
        accounts: &[InstructionAccount],
    ) -> (String, Option<ParsedInstruction>, Option<String>) {
        // ATA Create instruction has no data, just accounts
        if accounts.len() >= 4 {
            (
                "Create".to_string(),
                Some(ParsedInstruction::AtaCreate {
                    payer: accounts.get(0).map(|a| a.pubkey.clone()).unwrap_or_default(),
                    associated_token: accounts.get(1).map(|a| a.pubkey.clone()).unwrap_or_default(),
                    wallet: accounts.get(2).map(|a| a.pubkey.clone()).unwrap_or_default(),
                    mint: accounts.get(3).map(|a| a.pubkey.clone()).unwrap_or_default(),
                }),
                None,
            )
        } else {
            ("Create".to_string(), None, Some("Invalid ATA accounts".to_string()))
        }
    }
    
    fn parse_memo_instruction(
        &self,
        data: &[u8],
    ) -> (String, Option<ParsedInstruction>, Option<String>) {
        let message = String::from_utf8_lossy(data).to_string();
        (
            "Memo".to_string(),
            Some(ParsedInstruction::Memo { message }),
            None,
        )
    }
    
    fn parse_ore_instruction(
        &self,
        data: &[u8],
        accounts: &[InstructionAccount],
        _account_keys: &[Pubkey],
    ) -> (String, Option<ParsedInstruction>, Option<String>) {
        let parse_start = std::time::Instant::now();
        
        if data.is_empty() {
            return ("Empty".to_string(), None, Some("Empty data".to_string()));
        }
        
        let tag = data[0];
        
        match OreInstruction::try_from(tag) {
            Ok(OreInstruction::Deploy) => {
                tracing::trace!(tag = tag, "parse_ore_instruction: parsing Deploy");
                
                const DEPLOY_SIZE: usize = std::mem::size_of::<Deploy>();
                if data.len() < 1 + DEPLOY_SIZE {
                    return ("Deploy".to_string(), None, Some("Deploy data too short".to_string()));
                }
                
                let body = &data[1..1 + DEPLOY_SIZE];
                let deploy: &Deploy = bytemuck::from_bytes(body);
                
                let amount = u64::from_le_bytes(deploy.amount);
                let mask = u32::from_le_bytes(deploy.squares);
                
                let mut squares = Vec::new();
                for i in 0..25u8 {
                    if (mask & (1 << i)) != 0 {
                        squares.push(i);
                    }
                }
                
                let total = amount * squares.len() as u64;
                
                // Deploy accounts: [0:signer, 1:authority, 2:automation, 3:board, 4:config, 5:miner, 6:round, ...]
                let round_str = accounts.get(6).map(|a| a.pubkey.clone()).unwrap_or_default();
                let round_id = Pubkey::from_str(&round_str)
                    .ok()
                    .and_then(|pk| self.check_round_pda(&pk));
                
                tracing::trace!(
                    round_pda = %round_str,
                    round_id = ?round_id,
                    matches_expected = round_id.is_some(),
                    elapsed_us = parse_start.elapsed().as_micros(),
                    "parse_ore_instruction: Deploy round check"
                );
                
                (
                    "Deploy".to_string(),
                    Some(ParsedInstruction::OreDeploy {
                        signer: accounts.get(0).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        authority: accounts.get(1).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        automation_pda: accounts.get(2).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        board: accounts.get(3).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        config: accounts.get(4).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        miner: accounts.get(5).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        round: round_str,
                        round_id,
                        amount_per_square: amount,
                        amount_sol: amount as f64 / 1e9,
                        squares_mask: mask,
                        squares,
                        total_lamports: total,
                        total_sol: total as f64 / 1e9,
                    }),
                    None,
                )
            }
            Ok(OreInstruction::Checkpoint) => {
                tracing::trace!(tag = tag, "parse_ore_instruction: parsing Checkpoint");
                
                // Checkpoint accounts: [0:signer, 1:board, 2:miner, 3:round, 4:treasury, ...]
                let round_str = accounts.get(3).map(|a| a.pubkey.clone()).unwrap_or_default();
                let round_id = Pubkey::from_str(&round_str)
                    .ok()
                    .and_then(|pk| self.check_round_pda(&pk));
                
                tracing::trace!(
                    round_pda = %round_str,
                    round_id = ?round_id,
                    matches_expected = round_id.is_some(),
                    elapsed_us = parse_start.elapsed().as_micros(),
                    "parse_ore_instruction: Checkpoint round check"
                );
                
                (
                    "Checkpoint".to_string(),
                    Some(ParsedInstruction::OreCheckpoint {
                        signer: accounts.get(0).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        board: accounts.get(1).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        miner: accounts.get(2).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        round: round_str,
                        round_id,
                        treasury: accounts.get(4).map(|a| a.pubkey.clone()).unwrap_or_default(),
                    }),
                    None,
                )
            }
            Ok(OreInstruction::ClaimSOL) => {
                // ClaimSOL accounts: [0:signer, 1:miner, 2:system_program]
                (
                    "ClaimSOL".to_string(),
                    Some(ParsedInstruction::OreClaimSOL {
                        signer: accounts.get(0).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        miner: accounts.get(1).map(|a| a.pubkey.clone()).unwrap_or_default(),
                    }),
                    None,
                )
            }
            Ok(OreInstruction::ClaimORE) => {
                // ClaimORE accounts: [0:signer, 1:miner, 2:mint, 3:recipient_tokens, 4:treasury, 5:treasury_tokens, ...]
                (
                    "ClaimORE".to_string(),
                    Some(ParsedInstruction::OreClaimORE {
                        signer: accounts.get(0).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        miner: accounts.get(1).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        mint: accounts.get(2).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        recipient_tokens: accounts.get(3).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        treasury: accounts.get(4).map(|a| a.pubkey.clone()).unwrap_or_default(),
                    }),
                    None,
                )
            }
            Ok(OreInstruction::Automate) => {
                // Automate accounts: [0:signer, 1:automation, 2:executor, 3:miner, 4:system_program]
                (
                    "Automate".to_string(),
                    Some(ParsedInstruction::OreAutomate {
                        signer: accounts.get(0).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        automation_pda: accounts.get(1).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        executor: accounts.get(2).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        miner: accounts.get(3).map(|a| a.pubkey.clone()).unwrap_or_default(),
                    }),
                    None,
                )
            }
            Ok(OreInstruction::Reset) => {
                // Reset accounts: [0:signer, 1:board, 2:config, 3:fee_collector, 4:mint, 5:round, 6:round_next, 7:top_miner, 8:treasury, ...]
                (
                    "Reset".to_string(),
                    Some(ParsedInstruction::OreReset {
                        signer: accounts.get(0).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        board: accounts.get(1).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        config: accounts.get(2).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        fee_collector: accounts.get(3).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        round: accounts.get(5).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        round_next: accounts.get(6).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        top_miner: accounts.get(7).map(|a| a.pubkey.clone()).unwrap_or_default(),
                        treasury: accounts.get(8).map(|a| a.pubkey.clone()).unwrap_or_default(),
                    }),
                    None,
                )
            }
            Ok(OreInstruction::Log) => {
                let event_type = if data.len() > 1 {
                    format!("Event({})", data[1])
                } else {
                    "Unknown".to_string()
                };
                (
                    "Log".to_string(),
                    Some(ParsedInstruction::OreLog {
                        event_type,
                        data_hex: hex::encode(&data[1..]),
                    }),
                    None,
                )
            }
            Ok(other) => {
                let name = format!("{:?}", other);
                (
                    name.clone(),
                    Some(ParsedInstruction::OreOther {
                        instruction_tag: tag,
                        instruction_name: name,
                        accounts_count: accounts.len(),
                    }),
                    None,
                )
            }
            Err(_) => (
                format!("Unknown({})", tag),
                None,
                Some(format!("Unknown ORE instruction tag: {}", tag)),
            ),
        }
    }
    
    /// Parse EVORE program instructions
    fn parse_evore_instruction(
        &self,
        data: &[u8],
        _accounts: &[InstructionAccount],
    ) -> (String, Option<ParsedInstruction>, Option<String>) {
        if data.is_empty() {
            return ("Empty".to_string(), None, Some("Empty instruction data".to_string()));
        }
        
        let tag = data[0];
        let instruction_name = match tag {
            0 => "CreateManager",
            1 => "MMDeploy",
            2 => "MMCheckpoint",
            3 => "MMClaimSOL",
            4 => "MMClaimORE",
            5 => "CreateDeployer",
            6 => "UpdateDeployer",
            7 => "MMAutodeploy",
            8 => "DepositAutodeployBalance",
            9 => "RecycleSol",
            10 => "WithdrawAutodeployBalance",
            11 => "MMAutocheckpoint",
            12 => "MMFullAutodeploy",
            13 => "TransferManager",
            14 => "MMCreateMiner",
            _ => {
                return (
                    format!("Unknown({})", tag),
                    Some(ParsedInstruction::Unknown {
                        program: EVORE_PROGRAM_ID.to_string(),
                        data_preview: if data.len() > 32 {
                            format!("{}...", hex::encode(&data[..32]))
                        } else {
                            hex::encode(data)
                        },
                    }),
                    Some(format!("Unknown EVORE instruction tag: {}", tag)),
                );
            }
        };
        
        // For known instructions, return the name with data preview
        (
            instruction_name.to_string(),
            Some(ParsedInstruction::Unknown {
                program: EVORE_PROGRAM_ID.to_string(),
                data_preview: if data.len() > 32 {
                    format!("{}...", hex::encode(&data[..32]))
                } else {
                    hex::encode(data)
                },
            }),
            None,
        )
    }
    
    /// Parse Entropy program instructions
    fn parse_entropy_instruction(
        &self,
        data: &[u8],
        _accounts: &[InstructionAccount],
    ) -> (String, Option<ParsedInstruction>, Option<String>) {
        if data.is_empty() {
            return ("Empty".to_string(), None, Some("Empty instruction data".to_string()));
        }
        
        let tag = data[0];
        // Basic entropy instruction identification (can be expanded)
        let instruction_name = match tag {
            0 => "Open",
            1 => "Commit",
            2 => "Reveal",
            3 => "Sample",
            4 => "Close",
            _ => {
                return (
                    format!("Unknown({})", tag),
                    Some(ParsedInstruction::Unknown {
                        program: ENTROPY_PROGRAM_ID.to_string(),
                        data_preview: hex::encode(data),
                    }),
                    Some(format!("Unknown Entropy instruction tag: {}", tag)),
                );
            }
        };
        
        (
            instruction_name.to_string(),
            Some(ParsedInstruction::Unknown {
                program: ENTROPY_PROGRAM_ID.to_string(),
                data_preview: hex::encode(data),
            }),
            None,
        )
    }
    
    /// Parse ORE Mint program instructions
    fn parse_ore_mint_instruction(
        &self,
        data: &[u8],
        _accounts: &[InstructionAccount],
    ) -> (String, Option<ParsedInstruction>, Option<String>) {
        if data.is_empty() {
            return ("Empty".to_string(), None, Some("Empty instruction data".to_string()));
        }
        
        let tag = data[0];
        // Basic ore mint instruction identification (can be expanded)
        let instruction_name = match tag {
            0 => "Initialize",
            1 => "Mint",
            _ => {
                return (
                    format!("Unknown({})", tag),
                    Some(ParsedInstruction::Unknown {
                        program: ORE_MINT_PROGRAM_ID.to_string(),
                        data_preview: hex::encode(data),
                    }),
                    Some(format!("Unknown ORE Mint instruction tag: {}", tag)),
                );
            }
        };
        
        (
            instruction_name.to_string(),
            Some(ParsedInstruction::Unknown {
                program: ORE_MINT_PROGRAM_ID.to_string(),
                data_preview: hex::encode(data),
            }),
            None,
        )
    }
    
    fn determine_primary_action(
        &self,
        instructions: &[InstructionAnalysis],
        ore_analysis: Option<&OreTransactionAnalysis>,
    ) -> String {
        // Check for ORE actions first
        if let Some(ore) = ore_analysis {
            if ore.deploy_count > 0 {
                return format!("ORE Deploy ({} squares)", 
                    ore.deployments.iter().map(|d| d.squares.len()).sum::<usize>());
            }
            if ore.reset_count > 0 {
                return "ORE Reset".to_string();
            }
        }
        
        // Check for common actions
        for ix in instructions {
            match ix.instruction_type.as_str() {
                "Transfer" if ix.program_name.contains("System") => {
                    return "SOL Transfer".to_string();
                }
                "Transfer" | "TransferChecked" if ix.program_name.contains("Token") => {
                    return "Token Transfer".to_string();
                }
                "CreateAccount" => return "Account Creation".to_string(),
                "Create" if ix.program_name.contains("Associated") => {
                    return "ATA Creation".to_string();
                }
                _ => {}
            }
        }
        
        if instructions.is_empty() {
            "Empty Transaction".to_string()
        } else {
            instructions.first()
                .map(|ix| format!("{} - {}", ix.program_name, ix.instruction_type))
                .unwrap_or_else(|| "Unknown".to_string())
        }
    }
    
    /// Add account role labels based on program and instruction type
    fn add_account_roles(
        &self,
        program_id: &str,
        data: &[u8],
        accounts: &mut [InstructionAccount],
    ) {
        // System Program
        if program_id == SYSTEM_PROGRAM_ID && data.len() >= 4 {
            let tag = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            match tag {
                // Transfer
                2 => {
                    if let Some(a) = accounts.get_mut(0) { a.role = Some("Source".into()); a.is_signer = true; }
                    if let Some(a) = accounts.get_mut(1) { a.role = Some("Destination".into()); }
                }
                // CreateAccount
                0 => {
                    if let Some(a) = accounts.get_mut(0) { a.role = Some("Funder".into()); a.is_signer = true; }
                    if let Some(a) = accounts.get_mut(1) { a.role = Some("New Account".into()); }
                }
                _ => {}
            }
        }
        
        // Compute Budget - no accounts typically
        
        // ORE Program
        if program_id == evore::ore_api::PROGRAM_ID.to_string() && !data.is_empty() {
            let tag = data[0];
            if let Ok(ore_ix) = OreInstruction::try_from(tag) {
                match ore_ix {
                    OreInstruction::Deploy => {
                        // [0:signer, 1:authority, 2:automation, 3:board, 4:config, 5:miner, 6:round, 7:system, 8:ore_program, ...]
                        if let Some(a) = accounts.get_mut(0) { a.role = Some("Signer".into()); a.is_signer = true; }
                        if let Some(a) = accounts.get_mut(1) { a.role = Some("Authority".into()); }
                        if let Some(a) = accounts.get_mut(2) { a.role = Some("Automation PDA".into()); a.is_writable = true; }
                        if let Some(a) = accounts.get_mut(3) { a.role = Some("Board".into()); a.is_writable = true; }
                        if let Some(a) = accounts.get_mut(4) { a.role = Some("Config".into()); a.is_writable = true; }
                        if let Some(a) = accounts.get_mut(5) { a.role = Some("Miner".into()); a.is_writable = true; }
                        if let Some(a) = accounts.get_mut(6) { a.role = Some("Round".into()); a.is_writable = true; }
                        if let Some(a) = accounts.get_mut(7) { a.role = Some("System Program".into()); }
                        if let Some(a) = accounts.get_mut(8) { a.role = Some("ORE Program".into()); }
                    }
                    OreInstruction::Reset => {
                        // [0:signer, 1:board, 2:config, 3:fee_collector, 4:mint, 5:round, 6:round_next, 7:top_miner, 8:treasury, ...]
                        if let Some(a) = accounts.get_mut(0) { a.role = Some("Signer".into()); a.is_signer = true; }
                        if let Some(a) = accounts.get_mut(1) { a.role = Some("Board".into()); a.is_writable = true; }
                        if let Some(a) = accounts.get_mut(2) { a.role = Some("Config".into()); a.is_writable = true; }
                        if let Some(a) = accounts.get_mut(3) { a.role = Some("Fee Collector".into()); a.is_writable = true; }
                        if let Some(a) = accounts.get_mut(4) { a.role = Some("Mint".into()); a.is_writable = true; }
                        if let Some(a) = accounts.get_mut(5) { a.role = Some("Round".into()); a.is_writable = true; }
                        if let Some(a) = accounts.get_mut(6) { a.role = Some("Round Next".into()); a.is_writable = true; }
                        if let Some(a) = accounts.get_mut(7) { a.role = Some("Top Miner".into()); a.is_writable = true; }
                        if let Some(a) = accounts.get_mut(8) { a.role = Some("Treasury".into()); a.is_writable = true; }
                    }
                    OreInstruction::Checkpoint => {
                        // [0:signer, 1:board, 2:miner, 3:round, 4:treasury, 5:system_program]
                        if let Some(a) = accounts.get_mut(0) { a.role = Some("Signer".into()); a.is_signer = true; }
                        if let Some(a) = accounts.get_mut(1) { a.role = Some("Board".into()); a.is_writable = true; }
                        if let Some(a) = accounts.get_mut(2) { a.role = Some("Miner".into()); a.is_writable = true; }
                        if let Some(a) = accounts.get_mut(3) { a.role = Some("Round".into()); }
                        if let Some(a) = accounts.get_mut(4) { a.role = Some("Treasury".into()); a.is_writable = true; }
                    }
                    OreInstruction::ClaimSOL => {
                        // [0:signer, 1:miner, 2:system_program]
                        if let Some(a) = accounts.get_mut(0) { a.role = Some("Signer".into()); a.is_signer = true; }
                        if let Some(a) = accounts.get_mut(1) { a.role = Some("Miner".into()); a.is_writable = true; }
                    }
                    OreInstruction::ClaimORE => {
                        // [0:signer, 1:miner, 2:mint, 3:recipient_tokens, 4:treasury, 5:treasury_tokens, ...]
                        if let Some(a) = accounts.get_mut(0) { a.role = Some("Signer".into()); a.is_signer = true; }
                        if let Some(a) = accounts.get_mut(1) { a.role = Some("Miner".into()); a.is_writable = true; }
                        if let Some(a) = accounts.get_mut(2) { a.role = Some("Mint".into()); a.is_writable = true; }
                        if let Some(a) = accounts.get_mut(3) { a.role = Some("Recipient Tokens".into()); a.is_writable = true; }
                        if let Some(a) = accounts.get_mut(4) { a.role = Some("Treasury".into()); a.is_writable = true; }
                        if let Some(a) = accounts.get_mut(5) { a.role = Some("Treasury Tokens".into()); a.is_writable = true; }
                    }
                    OreInstruction::Automate => {
                        // [0:signer, 1:automation, 2:executor, 3:miner, 4:system_program]
                        if let Some(a) = accounts.get_mut(0) { a.role = Some("Signer".into()); a.is_signer = true; }
                        if let Some(a) = accounts.get_mut(1) { a.role = Some("Automation PDA".into()); a.is_writable = true; }
                        if let Some(a) = accounts.get_mut(2) { a.role = Some("Executor".into()); a.is_writable = true; }
                        if let Some(a) = accounts.get_mut(3) { a.role = Some("Miner".into()); a.is_writable = true; }
                    }
                    _ => {
                        // Generic: first account is signer
                        if let Some(a) = accounts.get_mut(0) { a.role = Some("Signer".into()); a.is_signer = true; }
                    }
                }
            }
        }
        
        // Token Program
        if (program_id == TOKEN_PROGRAM_ID || program_id == TOKEN_2022_PROGRAM_ID) && !data.is_empty() {
            match data[0] {
                // Transfer
                3 => {
                    if let Some(a) = accounts.get_mut(0) { a.role = Some("Source".into()); a.is_writable = true; }
                    if let Some(a) = accounts.get_mut(1) { a.role = Some("Destination".into()); a.is_writable = true; }
                    if let Some(a) = accounts.get_mut(2) { a.role = Some("Authority".into()); a.is_signer = true; }
                }
                // TransferChecked
                12 => {
                    if let Some(a) = accounts.get_mut(0) { a.role = Some("Source".into()); a.is_writable = true; }
                    if let Some(a) = accounts.get_mut(1) { a.role = Some("Mint".into()); }
                    if let Some(a) = accounts.get_mut(2) { a.role = Some("Destination".into()); a.is_writable = true; }
                    if let Some(a) = accounts.get_mut(3) { a.role = Some("Authority".into()); a.is_signer = true; }
                }
                _ => {}
            }
        }
        
        // Associated Token Account Program
        if program_id == ASSOCIATED_TOKEN_PROGRAM_ID && accounts.len() >= 4 {
            if let Some(a) = accounts.get_mut(0) { a.role = Some("Payer".into()); a.is_signer = true; }
            if let Some(a) = accounts.get_mut(1) { a.role = Some("ATA".into()); a.is_writable = true; }
            if let Some(a) = accounts.get_mut(2) { a.role = Some("Wallet".into()); }
            if let Some(a) = accounts.get_mut(3) { a.role = Some("Mint".into()); }
        }
    }
}

impl Default for TransactionAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

