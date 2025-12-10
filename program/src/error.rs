use steel::*;

/// Custom errors for the Evore program.
/// 
/// Error codes are grouped by category but maintain backward compatibility.
/// Each error provides a descriptive message for debugging.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq, IntoPrimitive)]
#[repr(u32)]
pub enum EvoreError {
    // ========================
    // Original Errors (maintain backward compatibility)
    // ========================
    
    /// The signer is not the manager's authority
    #[error("Not authorized: signer is not the manager authority")]
    NotAuthorized = 1,
    
    /// Tried to deploy when too many slots remain in the round
    #[error("Too many slots left: wait until closer to round end")]
    TooManySlotsLeft = 2,
    
    /// The round has already ended
    #[error("End slot reached: round has already ended")]
    EndSlotReached = 3,
    
    // ========================
    // Account Validation Errors
    // ========================
    
    /// The provided PDA does not match the expected derived address
    #[error("Invalid PDA: address does not match expected derivation")]
    InvalidPDA = 4,
    
    /// The manager account has not been initialized
    #[error("Manager not initialized: create manager first")]
    ManagerNotInitialized = 5,
    
    /// The fee collector address does not match the expected address
    #[error("Invalid fee collector: address does not match expected")]
    InvalidFeeCollector = 6,
    
    // ========================
    // Calculation Errors
    // ========================
    
    /// No profitable deployments found with current parameters
    #[error("No deployments: EV calculation found no profitable squares")]
    NoDeployments = 7,
    
    /// Arithmetic overflow during calculation
    #[error("Arithmetic overflow: calculation exceeded safe bounds")]
    ArithmeticOverflow = 8,
    
    // ========================
    // Multi-Deploy Errors
    // ========================
    
    /// Already deployed to this round and multi-deploy is not allowed
    #[error("Already deployed: multi-deploy not allowed for this strategy")]
    AlreadyDeployedThisRound = 9,
}

error!(EvoreError);
