// Removed feature constraint
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    system_program,
};
use solana_program_test::{processor, ProgramTest, ProgramTestContext};
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::Transaction,
};

use solana_crowdfunding::{
    instruction::CrowdfundingInstruction,
    processor::Processor,
    state::Campaign,
};

fn program_id() -> Pubkey {
    Pubkey::new_from_array([1u8; 32])
}

fn program_test() -> ProgramTest {
    ProgramTest::new(
        "solana_crowdfunding",
        program_id(),
        processor!(Processor::process),
    )
}

async fn setup_campaign(
    context: &mut ProgramTestContext,
    payer: &Keypair,
    goal: u64,
    deadline: i64,
) -> Keypair {
    let program_id = program_id();
    let campaign_keypair = Keypair::new();

    let rent = context.banks_client.get_rent().await.unwrap();
    let campaign_rent = rent.minimum_balance(32 + 8 + 8 + 8 + 1);

    let mut instructions = vec![solana_sdk::system_instruction::create_account(
        &payer.pubkey(),
        &campaign_keypair.pubkey(),
        campaign_rent,
        (32 + 8 + 8 + 8 + 1) as u64,
        &program_id,
    )];

    let create_instr = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(campaign_keypair.pubkey(), false),
        ],
        data: borsh::to_vec(&CrowdfundingInstruction::CreateCampaign { goal, deadline })
            .unwrap(),
    };
    instructions.push(create_instr);

    let mut transaction = Transaction::new_with_payer(&instructions, Some(&payer.pubkey()));
    transaction.sign(&[payer, &campaign_keypair], context.last_blockhash);

    context.banks_client.process_transaction(transaction).await.unwrap();

    campaign_keypair
}

#[tokio::test]
async fn test_create_campaign() {
    let mut pt = program_test();
    let mut context = pt.start_with_context().await;
    
    // Convert to Keypair matching
    let payer = Keypair::from_bytes(&context.payer.to_bytes()).unwrap();

    let goal = 1000 * 1_000_000_000;
    let deadline = 10000000000;

    let campaign_keypair = setup_campaign(&mut context, &payer, goal, deadline).await;

    let campaign_account = context
        .banks_client
        .get_account(campaign_keypair.pubkey())
        .await
        .unwrap()
        .unwrap();

    let campaign_data = Campaign::try_from_slice(&campaign_account.data).unwrap();
    assert_eq!(campaign_data.creator, payer.pubkey());
    assert_eq!(campaign_data.goal, goal);
    assert_eq!(campaign_data.raised, 0);
    assert_eq!(campaign_data.deadline, deadline);
    assert_eq!(campaign_data.claimed, false);
}

#[tokio::test]
async fn test_contribute() {
    let mut pt = program_test();
    let mut context = pt.start_with_context().await;
    let payer = Keypair::from_bytes(&context.payer.to_bytes()).unwrap();

    let goal = 1000 * 1_000_000_000;
    let deadline = 10000000000;
    let campaign_keypair = setup_campaign(&mut context, &payer, goal, deadline).await;

    let program_id = program_id();

    let (vault_pda, _bump) = Pubkey::find_program_address(
        &[b"vault", campaign_keypair.pubkey().as_ref()],
        &program_id,
    );

    let (contribution_pda, _bump) = Pubkey::find_program_address(
        &[
            b"contribution",
            campaign_keypair.pubkey().as_ref(),
            payer.pubkey().as_ref(),
        ],
        &program_id,
    );

    let amount = 600 * 1_000_000_000;

    let contribute_instr = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(campaign_keypair.pubkey(), false),
            AccountMeta::new(contribution_pda, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data: borsh::to_vec(&CrowdfundingInstruction::Contribute { amount })
            .unwrap(),
    };

    let mut transaction = Transaction::new_with_payer(&[contribute_instr], Some(&payer.pubkey()));
    transaction.sign(&[&payer], context.last_blockhash);

    context.banks_client.process_transaction(transaction).await.unwrap();

    let campaign_account = context
        .banks_client
        .get_account(campaign_keypair.pubkey())
        .await
        .unwrap()
        .unwrap();

    let campaign_data = Campaign::try_from_slice(&campaign_account.data).unwrap();
    assert_eq!(campaign_data.raised, amount);
}

#[tokio::test]
async fn test_withdraw() {
    let mut pt = program_test();
    let mut context = pt.start_with_context().await;
    let payer = Keypair::from_bytes(&context.payer.to_bytes()).unwrap();

    let goal = 1000 * 1_000_000_000;
    let deadline = 500;
    
    // Set clock before setup_campaign
    let mut clock = context.banks_client.get_sysvar::<solana_program::clock::Clock>().await.unwrap();
    clock.unix_timestamp = 100; // Before the deadline
    context.set_sysvar(&clock);

    let campaign_keypair = setup_campaign(&mut context, &payer, goal, deadline).await;

    let program_id = program_id();

    let (vault_pda, _bump) = Pubkey::find_program_address(
        &[b"vault", campaign_keypair.pubkey().as_ref()],
        &program_id,
    );
    let (contribution_pda, _bump) = Pubkey::find_program_address(
        &[
            b"contribution",
            campaign_keypair.pubkey().as_ref(),
            payer.pubkey().as_ref(),
        ],
        &program_id,
    );

    let amount = 1000 * 1_000_000_000;

    let contribute_instr = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(campaign_keypair.pubkey(), false),
            AccountMeta::new(contribution_pda, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data: borsh::to_vec(&CrowdfundingInstruction::Contribute { amount }).unwrap(),
    };

    let mut transaction = Transaction::new_with_payer(&[contribute_instr], Some(&payer.pubkey()));
    transaction.sign(&[&payer], context.last_blockhash);

    context.banks_client.process_transaction(transaction).await.unwrap();

    // To withdraw, we need to artificially advance the clock so deadline is passed
    let mut clock = context.banks_client.get_sysvar::<solana_program::clock::Clock>().await.unwrap();
    clock.unix_timestamp = 1000; // Past the deadline
    context.set_sysvar(&clock);

    // Now withdraw
    let withdraw_instr = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(campaign_keypair.pubkey(), false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data: borsh::to_vec(&CrowdfundingInstruction::Withdraw).unwrap(),
    };

    // Need a new blockhash to prevent duplicate tx
    let recent_blockhash = context.banks_client.get_latest_blockhash().await.unwrap();
    let mut transaction = Transaction::new_with_payer(&[withdraw_instr], Some(&payer.pubkey()));
    transaction.sign(&[&payer], recent_blockhash);

    context.banks_client.process_transaction(transaction).await.unwrap();

    let campaign_account = context
        .banks_client
        .get_account(campaign_keypair.pubkey())
        .await
        .unwrap()
        .unwrap();

    let campaign_data = Campaign::try_from_slice(&campaign_account.data).unwrap();
    assert!(campaign_data.claimed);
}

#[tokio::test]
async fn test_refund() {
    let mut pt = program_test();
    let mut context = pt.start_with_context().await;
    let payer = Keypair::from_bytes(&context.payer.to_bytes()).unwrap();

    let goal = 1000 * 1_000_000_000;
    let deadline = 500;
    
    // Set clock before setup_campaign
    let mut clock = context.banks_client.get_sysvar::<solana_program::clock::Clock>().await.unwrap();
    clock.unix_timestamp = 100; // Before the deadline
    context.set_sysvar(&clock);

    let campaign_keypair = setup_campaign(&mut context, &payer, goal, deadline).await;

    let program_id = program_id();

    let (vault_pda, _bump) = Pubkey::find_program_address(
        &[b"vault", campaign_keypair.pubkey().as_ref()],
        &program_id,
    );
    let (contribution_pda, _bump) = Pubkey::find_program_address(
        &[
            b"contribution",
            campaign_keypair.pubkey().as_ref(),
            payer.pubkey().as_ref(),
        ],
        &program_id,
    );

    let amount = 500 * 1_000_000_000; // Under goal

    let contribute_instr = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(campaign_keypair.pubkey(), false),
            AccountMeta::new(contribution_pda, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data: borsh::to_vec(&CrowdfundingInstruction::Contribute { amount }).unwrap(),
    };

    let mut transaction = Transaction::new_with_payer(&[contribute_instr], Some(&payer.pubkey()));
    transaction.sign(&[&payer], context.last_blockhash);

    context.banks_client.process_transaction(transaction).await.unwrap();

    // Fast-forward past deadline
    clock.unix_timestamp = 1000; // Past the deadline
    context.set_sysvar(&clock);

    // Now refund
    let refund_instr = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true), // Payer is acting as the donor here
            AccountMeta::new(campaign_keypair.pubkey(), false),
            AccountMeta::new(contribution_pda, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data: borsh::to_vec(&CrowdfundingInstruction::Refund).unwrap(),
    };

    let recent_blockhash = context.banks_client.get_latest_blockhash().await.unwrap();
    let mut transaction = Transaction::new_with_payer(&[refund_instr], Some(&payer.pubkey()));
    transaction.sign(&[&payer], recent_blockhash);

    context.banks_client.process_transaction(transaction).await.unwrap();

    // Verify refund (vault should be 0 since payer was the only donor, so the account is deleted)
    let vault_account = context
        .banks_client
        .get_account(vault_pda)
        .await
        .unwrap();

    assert!(vault_account.is_none());
}
