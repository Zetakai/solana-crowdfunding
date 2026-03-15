use crate::error::CrowdfundingError;
use crate::instruction::CrowdfundingInstruction;
use crate::state::{Campaign, Contribution};
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::Clock,
    entrypoint::ProgramResult,
    msg,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    system_instruction, system_program,
    sysvar::Sysvar,
};

pub struct Processor;

impl Processor {
    pub fn process(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        instruction_data: &[u8],
    ) -> ProgramResult {
        let instruction = CrowdfundingInstruction::try_from_slice(instruction_data)
            .map_err(|_| CrowdfundingError::InvalidInstruction)?;

        match instruction {
            CrowdfundingInstruction::CreateCampaign { goal, deadline } => {
                Self::process_create_campaign(program_id, accounts, goal, deadline)
            }
            CrowdfundingInstruction::Contribute { amount } => {
                Self::process_contribute(program_id, accounts, amount)
            }
            CrowdfundingInstruction::Withdraw => Self::process_withdraw(program_id, accounts),
            CrowdfundingInstruction::Refund => Self::process_refund(program_id, accounts),
        }
    }

    fn process_create_campaign(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        goal: u64,
        deadline: i64,
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let creator_account = next_account_info(account_info_iter)?;
        let campaign_account = next_account_info(account_info_iter)?;

        if !campaign_account.is_writable {
            return Err(ProgramError::InvalidAccountData);
        }

        if !creator_account.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        if campaign_account.owner != program_id {
            return Err(ProgramError::IncorrectProgramId);
        }

        // Ensure the campaign account is rent-exempt to prevent network purging
        let rent = Rent::get()?;
        if !rent.is_exempt(campaign_account.lamports(), campaign_account.data_len()) {
            return Err(ProgramError::AccountNotRentExempt);
        }

        let clock = Clock::get()?;
        if deadline <= clock.unix_timestamp {
            return Err(CrowdfundingError::DeadlinePassed.into());
        }

        let mut campaign_data = Campaign::try_from_slice(&campaign_account.data.borrow())
            .map_err(|_| ProgramError::InvalidAccountData)?;

        // Prevent re-initialization using a dedicated flag (safe even for goal=0 campaigns)
        if campaign_data.is_initialized {
            return Err(ProgramError::AccountAlreadyInitialized);
        }

        campaign_data.is_initialized = true;
        campaign_data.creator = *creator_account.key;
        campaign_data.goal = goal;
        campaign_data.raised = 0;
        campaign_data.deadline = deadline;
        campaign_data.claimed = false;

        campaign_data.serialize(&mut *campaign_account.data.borrow_mut())?;

        msg!("Campaign created: goal={}, deadline={}", goal, deadline);

        Ok(())
    }

    fn process_contribute(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        amount: u64,
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let donor_account = next_account_info(account_info_iter)?;
        let campaign_account = next_account_info(account_info_iter)?;
        let contribution_account = next_account_info(account_info_iter)?;
        let vault_account = next_account_info(account_info_iter)?;
        let system_program = next_account_info(account_info_iter)?;

        if !donor_account.is_writable
            || !campaign_account.is_writable
            || !contribution_account.is_writable
            || !vault_account.is_writable
        {
            return Err(ProgramError::InvalidAccountData);
        }

        if !donor_account.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        if amount == 0 {
            return Err(CrowdfundingError::InvalidAmount.into());
        }

        if campaign_account.owner != program_id {
            return Err(ProgramError::IllegalOwner);
        }

        if system_program.key != &system_program::id() {
            return Err(ProgramError::IncorrectProgramId);
        }

        let clock = Clock::get()?;

        let mut campaign_data = Campaign::try_from_slice(&campaign_account.data.borrow())
            .map_err(|_| ProgramError::InvalidAccountData)?;

        if clock.unix_timestamp >= campaign_data.deadline {
            return Err(CrowdfundingError::DeadlinePassed.into());
        }

        // Vault validation
        let (vault_pda, _bump) =
            Pubkey::find_program_address(&[b"vault", campaign_account.key.as_ref()], program_id);
        if vault_pda != *vault_account.key {
            return Err(CrowdfundingError::InvalidPDA.into());
        }

        // Create or update contribution
        let rent = Rent::get()?;
        let contribution_seeds = &[
            b"contribution",
            campaign_account.key.as_ref(),
            donor_account.key.as_ref(),
        ];
        let (contribution_pda, record_bump) =
            Pubkey::find_program_address(contribution_seeds, program_id);

        if contribution_pda != *contribution_account.key {
            return Err(CrowdfundingError::InvalidPDA.into());
        }

        let mut current_contribution = 0u64;

        if contribution_account.data_is_empty() {
            let space = 8; // u64 length for amount
            let rent_lamports = rent.minimum_balance(space);
            let required_lamports = rent_lamports.saturating_sub(contribution_account.lamports());

            if required_lamports > 0 {
                invoke(
                    &system_instruction::transfer(
                        donor_account.key,
                        contribution_account.key,
                        required_lamports,
                    ),
                    &[
                        donor_account.clone(),
                        contribution_account.clone(),
                        system_program.clone(),
                    ],
                )?;
            }

            invoke_signed(
                &system_instruction::allocate(contribution_account.key, space as u64),
                &[contribution_account.clone(), system_program.clone()],
                &[&[
                    b"contribution",
                    campaign_account.key.as_ref(),
                    donor_account.key.as_ref(),
                    &[record_bump],
                ]],
            )?;

            invoke_signed(
                &system_instruction::assign(contribution_account.key, program_id),
                &[contribution_account.clone(), system_program.clone()],
                &[&[
                    b"contribution",
                    campaign_account.key.as_ref(),
                    donor_account.key.as_ref(),
                    &[record_bump],
                ]],
            )?;
        } else {
            let record = match Contribution::try_from_slice(&contribution_account.data.borrow()) {
                Ok(c) => c,
                Err(_) => Contribution { amount: 0 },
            };
            current_contribution = record.amount;
        }

        // Save contribution amount
        let new_contribution = current_contribution
            .checked_add(amount)
            .ok_or(CrowdfundingError::ArithmeticOverflow)?;
        let record = Contribution {
            amount: new_contribution,
        };
        record.serialize(&mut *contribution_account.data.borrow_mut())?;

        // Transfer funds to vault
        invoke(
            &system_instruction::transfer(donor_account.key, vault_account.key, amount),
            &[
                donor_account.clone(),
                vault_account.clone(),
                system_program.clone(),
            ],
        )?;

        campaign_data.raised = campaign_data
            .raised
            .checked_add(amount)
            .ok_or(CrowdfundingError::ArithmeticOverflow)?;
        campaign_data.serialize(&mut *campaign_account.data.borrow_mut())?;

        msg!(
            "Contributed: {} lamports, total={}",
            amount,
            campaign_data.raised
        );

        Ok(())
    }

    fn process_withdraw(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let creator_account = next_account_info(account_info_iter)?;
        let campaign_account = next_account_info(account_info_iter)?;
        let vault_account = next_account_info(account_info_iter)?;
        let system_program = next_account_info(account_info_iter)?;

        if !creator_account.is_writable
            || !campaign_account.is_writable
            || !vault_account.is_writable
        {
            return Err(ProgramError::InvalidAccountData);
        }

        if !creator_account.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        if campaign_account.owner != program_id {
            return Err(ProgramError::IllegalOwner);
        }

        if system_program.key != &system_program::id() {
            return Err(ProgramError::IncorrectProgramId);
        }

        let mut campaign_data = Campaign::try_from_slice(&campaign_account.data.borrow())
            .map_err(|_| ProgramError::InvalidAccountData)?;

        if *creator_account.key != campaign_data.creator {
            return Err(ProgramError::InvalidAccountData);
        }

        if campaign_data.claimed {
            return Err(CrowdfundingError::AlreadyClaimed.into());
        }

        let clock = Clock::get()?;
        if clock.unix_timestamp < campaign_data.deadline {
            return Err(CrowdfundingError::DeadlineNotReached.into());
        }

        if campaign_data.raised < campaign_data.goal {
            return Err(CrowdfundingError::GoalNotMet.into());
        }

        let (vault_pda, bump) =
            Pubkey::find_program_address(&[b"vault", campaign_account.key.as_ref()], program_id);

        if vault_pda != *vault_account.key {
            return Err(CrowdfundingError::InvalidPDA.into());
        }

        let amount = vault_account.lamports();

        invoke_signed(
            &system_instruction::transfer(vault_account.key, creator_account.key, amount),
            &[
                vault_account.clone(),
                creator_account.clone(),
                system_program.clone(),
            ],
            &[&[b"vault", campaign_account.key.as_ref(), &[bump]]],
        )?;

        campaign_data.claimed = true;
        campaign_data.serialize(&mut *campaign_account.data.borrow_mut())?;

        msg!("Withdrawn: {} lamports", amount);

        Ok(())
    }

    fn process_refund(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let donor_account = next_account_info(account_info_iter)?;
        let campaign_account = next_account_info(account_info_iter)?;
        let contribution_account = next_account_info(account_info_iter)?;
        let vault_account = next_account_info(account_info_iter)?;
        let system_program = next_account_info(account_info_iter)?;

        if !donor_account.is_writable
            || !campaign_account.is_writable
            || !contribution_account.is_writable
            || !vault_account.is_writable
        {
            return Err(ProgramError::InvalidAccountData);
        }

        if campaign_account.owner != program_id {
            return Err(ProgramError::IllegalOwner);
        }

        if system_program.key != &system_program::id() {
            return Err(ProgramError::IncorrectProgramId);
        }

        let campaign_data = Campaign::try_from_slice(&campaign_account.data.borrow())
            .map_err(|_| ProgramError::InvalidAccountData)?;

        let clock = Clock::get()?;
        if clock.unix_timestamp < campaign_data.deadline {
            return Err(CrowdfundingError::DeadlineNotReached.into());
        }

        if campaign_data.raised >= campaign_data.goal {
            return Err(CrowdfundingError::GoalMet.into());
        }

        let contribution_seeds = &[
            b"contribution",
            campaign_account.key.as_ref(),
            donor_account.key.as_ref(),
        ];
        let (contribution_pda, _bump) =
            Pubkey::find_program_address(contribution_seeds, program_id);

        if contribution_pda != *contribution_account.key {
            return Err(CrowdfundingError::InvalidPDA.into());
        }

        // Verify the contribution account is owned by this program
        if contribution_account.owner != program_id {
            return Err(ProgramError::IllegalOwner);
        }

        let record = Contribution::try_from_slice(&contribution_account.data.borrow())
            .map_err(|_| ProgramError::InvalidAccountData)?;

        let amount = record.amount;
        if amount == 0 {
            return Err(CrowdfundingError::InvalidAmount.into());
        }

        let (vault_pda, _vault_bump) =
            Pubkey::find_program_address(&[b"vault", campaign_account.key.as_ref()], program_id);

        if vault_pda != *vault_account.key {
            return Err(CrowdfundingError::InvalidPDA.into());
        }

        // Always refund the exact contributed amount — never drain the vault
        // to avoid stealing from other donors who haven't yet refunded.
        invoke_signed(
            &system_instruction::transfer(vault_account.key, donor_account.key, amount),
            &[
                vault_account.clone(),
                donor_account.clone(),
                system_program.clone(),
            ],
            &[&[b"vault", campaign_account.key.as_ref(), &[_vault_bump]]],
        )?;

        // Close the contribution account cleanly: zero its data and return rent to donor.
        // This must happen AFTER the CPI to preserve the lamport balance invariant.
        {
            let mut data = contribution_account.data.borrow_mut();
            for byte in data.iter_mut() {
                *byte = 0;
            }
        }
        let contribution_rent = contribution_account.lamports();
        **contribution_account.try_borrow_mut_lamports()? -= contribution_rent;
        **donor_account.try_borrow_mut_lamports()? += contribution_rent;

        msg!("Refunded: {} lamports + {} rent returned", amount, contribution_rent);

        Ok(())
    }
}
