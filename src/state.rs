use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::pubkey::Pubkey;

/// State of a Crowdfunding Campaign
#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct Campaign {
    pub creator: Pubkey,
    pub goal: u64,
    pub raised: u64,
    pub deadline: i64,
    pub claimed: bool,
}

/// State of an individual User's Contribution
#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct Contribution {
    pub amount: u64,
}
