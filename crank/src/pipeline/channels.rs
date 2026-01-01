//! Channel types for inter-system communication
//!
//! Each system in the pipeline communicates via tokio mpsc channels.

use tokio::sync::{broadcast, mpsc};

use super::types::{BatchedTx, FailedBatch, MinerTask, PendingConfirmation, SignedTx};

/// Channel capacity constants
pub const MINER_CHANNEL_SIZE: usize = 1000;
pub const TX_CHANNEL_SIZE: usize = 100;
pub const CONFIRMATION_CHANNEL_SIZE: usize = 200;

/// All channels used in the pipeline
pub struct PipelineChannels {
    // === Miner flow channels (in pipeline order) ===
    
    /// Entry point: miners go to fee check first
    pub to_fee_check: mpsc::Sender<MinerTask>,
    pub from_fee_check: mpsc::Receiver<MinerTask>,
    
    /// Miners needing expected fee updates
    pub to_expected_fee_updater: mpsc::Sender<MinerTask>,
    pub from_expected_fee_updater: mpsc::Receiver<MinerTask>,
    
    /// Miners with valid fees go to LUT check
    pub to_lut_check: mpsc::Sender<MinerTask>,
    pub from_lut_check: mpsc::Receiver<MinerTask>,
    
    /// Miners needing LUT creation
    pub to_lut_creation: mpsc::Sender<MinerTask>,
    pub from_lut_creation: mpsc::Receiver<MinerTask>,
    
    /// Miners with LUT go to deployment check (3 workers read from this)
    pub to_deployment_check: mpsc::Sender<MinerTask>,
    pub from_deployment_check: mpsc::Receiver<MinerTask>,
    
    /// Miners needing checkpoint only
    pub to_checkpoint_batcher: mpsc::Sender<MinerTask>,
    pub from_checkpoint_batcher: mpsc::Receiver<MinerTask>,
    
    /// Miners ready to deploy
    pub to_deployer_batcher: mpsc::Sender<MinerTask>,
    pub from_deployer_batcher: mpsc::Receiver<MinerTask>,
    
    // === Transaction flow channels ===
    
    /// Batched transactions ready for signing
    pub to_tx_processor: mpsc::Sender<BatchedTx>,
    pub from_tx_processor: mpsc::Receiver<BatchedTx>,
    
    /// Signed transactions ready for sending
    pub to_tx_sender: mpsc::Sender<SignedTx>,
    pub from_tx_sender: mpsc::Receiver<SignedTx>,
    
    /// Transactions pending confirmation
    pub to_confirmation: mpsc::Sender<PendingConfirmation>,
    pub from_confirmation: mpsc::Receiver<PendingConfirmation>,
    
    // === Failure handling ===
    
    /// Failed batches for refresh and retry
    pub to_failure_handler: mpsc::Sender<FailedBatch>,
    pub from_failure_handler: mpsc::Receiver<FailedBatch>,
    
    // === Control signals ===
    
    /// Broadcast when a new round is detected
    pub round_changed: broadcast::Sender<u64>,
    
    /// Shutdown signal
    pub shutdown: broadcast::Sender<()>,
}

impl PipelineChannels {
    /// Create all pipeline channels
    pub fn new() -> Self {
        let (to_fee_check, from_fee_check) = mpsc::channel(MINER_CHANNEL_SIZE);
        let (to_expected_fee_updater, from_expected_fee_updater) = mpsc::channel(MINER_CHANNEL_SIZE);
        let (to_lut_check, from_lut_check) = mpsc::channel(MINER_CHANNEL_SIZE);
        let (to_lut_creation, from_lut_creation) = mpsc::channel(MINER_CHANNEL_SIZE);
        let (to_deployment_check, from_deployment_check) = mpsc::channel(MINER_CHANNEL_SIZE);
        let (to_checkpoint_batcher, from_checkpoint_batcher) = mpsc::channel(MINER_CHANNEL_SIZE);
        let (to_deployer_batcher, from_deployer_batcher) = mpsc::channel(MINER_CHANNEL_SIZE);
        
        let (to_tx_processor, from_tx_processor) = mpsc::channel(TX_CHANNEL_SIZE);
        let (to_tx_sender, from_tx_sender) = mpsc::channel(TX_CHANNEL_SIZE);
        let (to_confirmation, from_confirmation) = mpsc::channel(CONFIRMATION_CHANNEL_SIZE);
        let (to_failure_handler, from_failure_handler) = mpsc::channel(TX_CHANNEL_SIZE);
        
        let (round_changed, _) = broadcast::channel(16);
        let (shutdown, _) = broadcast::channel(1);
        
        Self {
            to_fee_check,
            from_fee_check,
            to_expected_fee_updater,
            from_expected_fee_updater,
            to_lut_check,
            from_lut_check,
            to_lut_creation,
            from_lut_creation,
            to_deployment_check,
            from_deployment_check,
            to_checkpoint_batcher,
            from_checkpoint_batcher,
            to_deployer_batcher,
            from_deployer_batcher,
            to_tx_processor,
            from_tx_processor,
            to_tx_sender,
            from_tx_sender,
            to_confirmation,
            from_confirmation,
            to_failure_handler,
            from_failure_handler,
            round_changed,
            shutdown,
        }
    }
    
    /// Subscribe to round change notifications
    pub fn subscribe_round_changed(&self) -> broadcast::Receiver<u64> {
        self.round_changed.subscribe()
    }
    
    /// Subscribe to shutdown notifications
    pub fn subscribe_shutdown(&self) -> broadcast::Receiver<()> {
        self.shutdown.subscribe()
    }
}

impl Default for PipelineChannels {
    fn default() -> Self {
        Self::new()
    }
}

/// Sender handles that can be cloned and passed to systems
/// 
/// This is a wrapper that only contains the sender halves of channels,
/// allowing systems to send to other systems without owning the receivers.
#[derive(Clone)]
pub struct ChannelSenders {
    pub to_fee_check: mpsc::Sender<MinerTask>,
    pub to_expected_fee_updater: mpsc::Sender<MinerTask>,
    pub to_lut_check: mpsc::Sender<MinerTask>,
    pub to_lut_creation: mpsc::Sender<MinerTask>,
    pub to_deployment_check: mpsc::Sender<MinerTask>,
    pub to_checkpoint_batcher: mpsc::Sender<MinerTask>,
    pub to_deployer_batcher: mpsc::Sender<MinerTask>,
    pub to_tx_processor: mpsc::Sender<BatchedTx>,
    pub to_tx_sender: mpsc::Sender<SignedTx>,
    pub to_confirmation: mpsc::Sender<PendingConfirmation>,
    pub to_failure_handler: mpsc::Sender<FailedBatch>,
    pub round_changed: broadcast::Sender<u64>,
    pub shutdown: broadcast::Sender<()>,
}

impl ChannelSenders {
    /// Create senders from the main channels struct
    pub fn from_channels(channels: &PipelineChannels) -> Self {
        Self {
            to_fee_check: channels.to_fee_check.clone(),
            to_expected_fee_updater: channels.to_expected_fee_updater.clone(),
            to_lut_check: channels.to_lut_check.clone(),
            to_lut_creation: channels.to_lut_creation.clone(),
            to_deployment_check: channels.to_deployment_check.clone(),
            to_checkpoint_batcher: channels.to_checkpoint_batcher.clone(),
            to_deployer_batcher: channels.to_deployer_batcher.clone(),
            to_tx_processor: channels.to_tx_processor.clone(),
            to_tx_sender: channels.to_tx_sender.clone(),
            to_confirmation: channels.to_confirmation.clone(),
            to_failure_handler: channels.to_failure_handler.clone(),
            round_changed: channels.round_changed.clone(),
            shutdown: channels.shutdown.clone(),
        }
    }
}

