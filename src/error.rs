use steel::*;

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq, IntoPrimitive)]
#[repr(u32)]
pub enum EvoreError {
    #[error("Not authorized")]
    NotAuthorized = 1,
    #[error("Too many slots left")]
    TooManySlotsLeft = 2,
    #[error("End slot exceeded")]
    EndSlotExceeded = 3,
}

error!(EvoreError);
