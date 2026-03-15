use solana_program::program_error::ProgramError;
use thiserror::Error;

#[derive(Error, Debug, Copy, Clone)]
pub enum CrowdfundingError {
    /// Invalid Instruction
    #[error("Invalid Instruction")]
    InvalidInstruction,
    /// Campaign Deadline Passed
    #[error("Campaign Deadline Passed")]
    DeadlinePassed,
    /// Campaign Deadline Not Reached
    #[error("Campaign Deadline Not Reached")]
    DeadlineNotReached,
    /// Campaign Goal Not Met
    #[error("Campaign Goal Not Met")]
    GoalNotMet,
    /// Campaign Goal Met Successfully
    #[error("Campaign Goal Met Successfully")]
    GoalMet,
    /// Campaign Already Claimed
    #[error("Campaign Already Claimed")]
    AlreadyClaimed,
    /// Invalid PDA Seeds
    #[error("Invalid PDA Seeds Provided")]
    InvalidPDA,
    /// Arithmetic Overflow
    #[error("Arithmetic Overflow Error")]
    ArithmeticOverflow,
    /// Invalid Amount
    #[error("Invalid Amount Given")]
    InvalidAmount,
}

impl From<CrowdfundingError> for ProgramError {
    fn from(e: CrowdfundingError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
