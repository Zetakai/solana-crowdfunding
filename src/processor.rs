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
    system_instruction,
    system_program,
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
            .map_err(|_| ProgramError::InvalidInstructionData)?;

        match instruction {
            CrowdfundingInstruction::CreateCampaign { goal, deadline } => {
                Self::process_create_campaign(program_id, accounts, goal, deadline)
            }
            CrowdfundingInstruction::Contribute { amount } => {
                Self::process_contribute(program_id, accounts, amount)
            }
            CrowdfundingInstruction::Withdraw => {
                Self::process_withdraw(program_id, accounts)
            }
            CrowdfundingInstruction::Refund => {
                Self::process_refund(program_id, accounts)
            }
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

        if !creator_account.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        if campaign_account.owner != program_id {
            return Err(ProgramError::IncorrectProgramId);
        }

        let clock = Clock::get()?;
        if deadline <= clock.unix_timestamp {
            return Err(ProgramError::InvalidArgument); // deadline must be in future
        }

        let mut campaign_data = Campaign::try_from_slice(&campaign_account.data.borrow())
            .map_err(|_| ProgramError::InvalidAccountData)?;

        // Ensure it's not already initialized by checking if the goal is 0
        if campaign_data.goal != 0 {
            return Err(ProgramError::AccountAlreadyInitialized);
        }

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

        if !donor_account.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let clock = Clock::get()?;

        let mut campaign_data = Campaign::try_from_slice(&campaign_account.data.borrow())
            .map_err(|_| ProgramError::InvalidAccountData)?;

        if clock.unix_timestamp >= campaign_data.deadline {
            return Err(ProgramError::InvalidArgument); // campaign ended
        }

        // Vault validation
        let (vault_pda, bump) = Pubkey::find_program_address(
            &[b"vault", campaign_account.key.as_ref()],
            program_id,
        );
        if vault_pda != *vault_account.key {
            return Err(ProgramError::InvalidSeeds);
        }

        // Create or update contribution
        let rent = Rent::get()?;
        let contribution_seeds = &[
            b"contribution",
            campaign_account.key.as_ref(),
            donor_account.key.as_ref(),
        ];
        let (contribution_pda, record_bump) = Pubkey::find_program_address(contribution_seeds, program_id);

        if contribution_pda != *contribution_account.key {
            return Err(ProgramError::InvalidSeeds);
        }

        let mut current_contribution = 0u64;
        
        if contribution_account.data_is_empty() {
            let space = 8; // u64 length for amount
            let rent_lamports = rent.minimum_balance(space);

            invoke_signed(
                &system_instruction::create_account(
                    donor_account.key,
                    contribution_account.key,
                    rent_lamports,
                    space as u64,
                    program_id,
                ),
                &[
                    donor_account.clone(),
                    contribution_account.clone(),
                    system_program.clone(),
                ],
                &[&[
                    b"contribution",
                    campaign_account.key.as_ref(),
                    donor_account.key.as_ref(),
                    &[record_bump],
                ]],
            )?;
        } else {
            let record = Contribution::try_from_slice(&contribution_account.data.borrow())
                .unwrap_or(Contribution { amount: 0 });
            current_contribution = record.amount;
        }

        // Save contribution amount
        let new_contribution = current_contribution.checked_add(amount).ok_or(ProgramError::InvalidInstructionData)?;
        let record = Contribution { amount: new_contribution };
        record.serialize(&mut *contribution_account.data.borrow_mut())?;

        // Transfer funds to vault
        invoke(
            &system_instruction::transfer(donor_account.key, vault_account.key, amount),
            &[donor_account.clone(), vault_account.clone(), system_program.clone()],
        )?;

        campaign_data.raised = campaign_data.raised.checked_add(amount).ok_or(ProgramError::InvalidInstructionData)?;
        campaign_data.serialize(&mut *campaign_account.data.borrow_mut())?;

        msg!("Contributed: {} lamports, total={}", amount, campaign_data.raised);

        Ok(())
    }

    fn process_withdraw(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let creator_account = next_account_info(account_info_iter)?;
        let campaign_account = next_account_info(account_info_iter)?;
        let vault_account = next_account_info(account_info_iter)?;
        let system_program = next_account_info(account_info_iter)?;

        if !creator_account.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let mut campaign_data = Campaign::try_from_slice(&campaign_account.data.borrow())
            .map_err(|_| ProgramError::InvalidAccountData)?;

        if *creator_account.key != campaign_data.creator {
            return Err(ProgramError::InvalidAccountData);
        }

        if campaign_data.claimed {
            return Err(ProgramError::InvalidArgument);
        }

        let clock = Clock::get()?;
        if clock.unix_timestamp < campaign_data.deadline {
            return Err(ProgramError::InvalidArgument); // too early
        }

        if campaign_data.raised < campaign_data.goal {
            return Err(ProgramError::InvalidArgument); // goal not met
        }

        let (vault_pda, bump) = Pubkey::find_program_address(
            &[b"vault", campaign_account.key.as_ref()],
            program_id,
        );

        if vault_pda != *vault_account.key {
            return Err(ProgramError::InvalidSeeds);
        }

        let amount = vault_account.lamports();
        
        invoke_signed(
            &system_instruction::transfer(vault_account.key, creator_account.key, amount),
            &[vault_account.clone(), creator_account.clone(), system_program.clone()],
            &[&[b"vault", campaign_account.key.as_ref(), &[bump]]],
        )?;

        campaign_data.claimed = true;
        campaign_data.serialize(&mut *campaign_account.data.borrow_mut())?;

        msg!("Withdrawn: {} lamports", amount);

        Ok(())
    }

    fn process_refund(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let donor_account = next_account_info(account_info_iter)?;
        let campaign_account = next_account_info(account_info_iter)?;
        let contribution_account = next_account_info(account_info_iter)?;
        let vault_account = next_account_info(account_info_iter)?;
        let system_program = next_account_info(account_info_iter)?;

        let campaign_data = Campaign::try_from_slice(&campaign_account.data.borrow())
            .map_err(|_| ProgramError::InvalidAccountData)?;

        let clock = Clock::get()?;
        if clock.unix_timestamp < campaign_data.deadline {
            return Err(ProgramError::InvalidArgument); // too early
        }

        if campaign_data.raised >= campaign_data.goal {
            return Err(ProgramError::InvalidArgument); // goal met, can't refund
        }

        let contribution_seeds = &[
            b"contribution",
            campaign_account.key.as_ref(),
            donor_account.key.as_ref(),
        ];
        let (contribution_pda, _bump) = Pubkey::find_program_address(contribution_seeds, program_id);

        if contribution_pda != *contribution_account.key {
            return Err(ProgramError::InvalidSeeds);
        }

        let record = Contribution::try_from_slice(&contribution_account.data.borrow())
            .map_err(|_| ProgramError::InvalidAccountData)?;

        let amount = record.amount;
        if amount == 0 {
            return Err(ProgramError::InvalidArgument); // nothing to refund
        }

        let (vault_pda, _vault_bump) = Pubkey::find_program_address(
            &[b"vault", campaign_account.key.as_ref()],
            program_id,
        );

        if vault_pda != *vault_account.key {
            return Err(ProgramError::InvalidSeeds);
        }

        // Empty the contribution account record
        let new_record = Contribution { amount: 0 };
        new_record.serialize(&mut *contribution_account.data.borrow_mut())?;

        // Transfer SOL back
        invoke_signed(
            &system_instruction::transfer(vault_account.key, donor_account.key, amount),
            &[vault_account.clone(), donor_account.clone(), system_program.clone()],
            &[&[b"vault", campaign_account.key.as_ref(), &[_vault_bump]]],
        )?;

        msg!("Refunded: {} lamports", amount);

        Ok(())
    }
}
