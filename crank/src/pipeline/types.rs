//! Types used throughout the pipeline
//!
//! These types are passed through channels between pipeline systems.

use solana_sdk::{
    pubkey::Pubkey,
    signature::Signature,
    transaction::VersionedTransaction,
};
use std::time::Instant;

use crate::config::DeployerInfo;

/// Transaction type for tracking different kinds of transactions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxType {
    /// Autodeploy transaction (batch of up to 7 miners)
    Deploy,
    /// Checkpoint transaction (batch of up to 5 miners)
    Checkpoint,
    /// Fee update transaction (batch of up to 10 miners)
    FeeUpdate,
}

impl std::fmt::Display for TxType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TxType::Deploy => write!(f, "Deploy"),
            TxType::Checkpoint => write!(f, "Checkpoint"),
            TxType::FeeUpdate => write!(f, "FeeUpdate"),
        }
    }
}

/// A miner task flowing through the pipeline
#[derive(Debug, Clone)]
pub struct MinerTask {
    /// The deployer info for this miner
    pub deployer: DeployerInfo,
    /// The ORE miner PDA address
    pub miner_address: Pubkey,
    /// The managed_miner_auth PDA
    pub miner_auth: Pubkey,
    /// Number of times this task has been retried after failure
    pub retry_count: u8,
    /// When this task was created
    pub created_at: Instant,
    /// Round ID this task is for
    pub round_id: u64,
}

impl MinerTask {
    /// Maximum number of retries before giving up
    pub const MAX_RETRIES: u8 = 3;

    /// Create a new miner task
    pub fn new(
        deployer: DeployerInfo,
        miner_address: Pubkey,
        miner_auth: Pubkey,
        round_id: u64,
    ) -> Self {
        Self {
            deployer,
            miner_address,
            miner_auth,
            retry_count: 0,
            created_at: Instant::now(),
            round_id,
        }
    }

    /// Check if we can retry this task
    pub fn can_retry(&self) -> bool {
        self.retry_count < Self::MAX_RETRIES
    }

    /// Increment retry count and return new task
    pub fn with_retry(&self) -> Self {
        Self {
            deployer: self.deployer.clone(),
            miner_address: self.miner_address,
            miner_auth: self.miner_auth,
            retry_count: self.retry_count + 1,
            created_at: self.created_at,
            round_id: self.round_id,
        }
    }

    /// Get the manager address
    pub fn manager(&self) -> Pubkey {
        self.deployer.manager_address
    }

    /// Get the deployer address
    pub fn deployer_address(&self) -> Pubkey {
        self.deployer.deployer_address
    }
}

/// A batched transaction ready for processing
#[derive(Debug)]
pub struct BatchedTx {
    /// The unsigned versioned transaction
    pub tx: VersionedTransaction,
    /// The miners included in this batch
    pub miners: Vec<MinerTask>,
    /// Type of transaction
    pub tx_type: TxType,
    /// When this batch was created
    pub created_at: Instant,
    /// Round ID this batch is for
    pub round_id: u64,
}

impl BatchedTx {
    /// Create a new batched transaction
    pub fn new(
        tx: VersionedTransaction,
        miners: Vec<MinerTask>,
        tx_type: TxType,
        round_id: u64,
    ) -> Self {
        Self {
            tx,
            miners,
            tx_type,
            created_at: Instant::now(),
            round_id,
        }
    }

    /// Get the number of miners in this batch
    pub fn batch_size(&self) -> usize {
        self.miners.len()
    }
}

/// A signed transaction ready for sending
#[derive(Debug)]
pub struct SignedTx {
    /// The signed versioned transaction
    pub tx: VersionedTransaction,
    /// The transaction signature
    pub signature: Signature,
    /// The miners included in this transaction
    pub miners: Vec<MinerTask>,
    /// Type of transaction
    pub tx_type: TxType,
    /// When this was signed
    pub signed_at: Instant,
    /// Round ID
    pub round_id: u64,
}

impl SignedTx {
    /// Create a new signed transaction
    pub fn new(
        tx: VersionedTransaction,
        signature: Signature,
        miners: Vec<MinerTask>,
        tx_type: TxType,
        round_id: u64,
    ) -> Self {
        Self {
            tx,
            signature,
            miners,
            tx_type,
            signed_at: Instant::now(),
            round_id,
        }
    }
}

/// A pending confirmation being tracked
#[derive(Debug)]
pub struct PendingConfirmation {
    /// The transaction signature
    pub signature: Signature,
    /// The miners included in this transaction
    pub miners: Vec<MinerTask>,
    /// Type of transaction
    pub tx_type: TxType,
    /// When the transaction was sent
    pub sent_at: Instant,
    /// Round ID
    pub round_id: u64,
    /// Number of times we've checked this signature
    pub check_count: u32,
}

impl PendingConfirmation {
    /// Create a new pending confirmation
    pub fn new(
        signature: Signature,
        miners: Vec<MinerTask>,
        tx_type: TxType,
        round_id: u64,
    ) -> Self {
        Self {
            signature,
            miners,
            tx_type,
            sent_at: Instant::now(),
            round_id,
            check_count: 0,
        }
    }

    /// How long since this was sent (in milliseconds)
    pub fn age_ms(&self) -> u64 {
        self.sent_at.elapsed().as_millis() as u64
    }
}

/// Result of a confirmation check
#[derive(Debug, Clone)]
pub enum ConfirmationResult {
    /// Transaction confirmed successfully
    Confirmed,
    /// Transaction failed with error
    Failed(String),
    /// Transaction still pending
    Pending,
    /// Timed out waiting for confirmation
    Timeout,
}

/// A failed batch of miners that needs handling
#[derive(Debug)]
pub struct FailedBatch {
    /// The miners that were in the failed transaction
    pub miners: Vec<MinerTask>,
    /// The transaction signature that failed
    pub signature: Signature,
    /// Type of transaction that failed
    pub tx_type: TxType,
    /// Round ID
    pub round_id: u64,
    /// Error message if available
    pub error: Option<String>,
}

