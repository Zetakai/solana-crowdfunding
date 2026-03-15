use borsh::{BorshDeserialize, BorshSerialize};

#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub enum CrowdfundingInstruction {
    /// Creates a new campaign
    ///
    /// Accounts expected:
    /// 0. `[signer]` The creator of the campaign
    /// 1. `[writable]` The campaign account (PDA)
    /// 2. `[]` System Program
    CreateCampaign { goal: u64, deadline: i64 },

    /// Contributes logic to the campaign
    ///
    /// Accounts expected:
    /// 0. `[signer, writable]` Donor
    /// 1. `[writable]` The campaign account (PDA)
    /// 2. `[writable]` The donor's contribution record (PDA)
    /// 3. `[writable]` The campaign vault account (PDA)
    /// 4. `[]` System Program
    Contribute { amount: u64 },

    /// Withdraws funds if the campaign succeeded after deadline
    ///
    /// Accounts expected:
    /// 0. `[signer, writable]` Creator
    /// 1. `[writable]` The campaign account (PDA)
    /// 2. `[writable]` The campaign vault account (PDA)
    /// 3. `[]` System Program
    Withdraw,

    /// Refunds donor if the campaign failed after deadline
    ///
    /// Accounts expected:
    /// 0. `[writable]` Donor account
    /// 1. `[writable]` The campaign account (PDA)
    /// 2. `[writable]` The donor's contribution record (PDA)
    /// 3. `[writable]` The campaign vault account (PDA)
    /// 4. `[]` System Program
    Refund,
}
