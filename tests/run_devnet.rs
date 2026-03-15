use solana_client::rpc_client::RpcClient;
use solana_crowdfunding::instruction::CrowdfundingInstruction;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{read_keypair_file, Keypair, Signer},
    system_program,
    transaction::Transaction,
};
use std::str::FromStr;
use std::thread::sleep;
use std::time::Duration;

#[test]
fn test_devnet() {
    let rpc_url = "https://api.devnet.solana.com";
    let client = RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed());

    // Load payer keypair
    let payer = read_keypair_file("/home/glianalabs/.config/solana/id.json")
        .expect("Failed to read keypair file");
    println!("Payer: {}", payer.pubkey());

    // Program ID
    let program_id = Pubkey::from_str("q1wfubYgXPQGTfCmRWfiuGAFPKUAb7kwWyXctWycyas").unwrap();

    // Campaign keypair
    let campaign = Keypair::new();
    println!("Campaign: {}", campaign.pubkey());

    // 1. Create a campaign with goal=100,000,000 lamports (0.1 SOL), deadline=20 seconds from now
    let goal = 100_000_000;
    let deadline = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 20;

    let campaign_rent = client
        .get_minimum_balance_for_rent_exemption(32 + 8 + 8 + 8 + 1)
        .unwrap();

    let create_instrs = vec![
        solana_sdk::system_instruction::create_account(
            &payer.pubkey(),
            &campaign.pubkey(),
            campaign_rent,
            (32 + 8 + 8 + 8 + 1) as u64,
            &program_id,
        ),
        Instruction {
            program_id,
            accounts: vec![
                AccountMeta::new(payer.pubkey(), true),
                AccountMeta::new(campaign.pubkey(), false),
            ],
            data: borsh::to_vec(&CrowdfundingInstruction::CreateCampaign { goal, deadline })
                .unwrap(),
        },
    ];

    let mut tx = Transaction::new_with_payer(&create_instrs, Some(&payer.pubkey()));
    let blockhash = client.get_latest_blockhash().unwrap();
    tx.sign(&[&payer, &campaign], blockhash);

    let sig = client.send_and_confirm_transaction(&tx).unwrap();
    println!("Created campaign TX: {}", sig);

    let (vault_pda, _bump) =
        Pubkey::find_program_address(&[b"vault", campaign.pubkey().as_ref()], &program_id);

    let (contribution_pda, _bump) = Pubkey::find_program_address(
        &[
            b"contribution",
            campaign.pubkey().as_ref(),
            payer.pubkey().as_ref(),
        ],
        &program_id,
    );

    // 2. Contribute 60,000,000 lamports (0.06 SOL) -> should succeed
    let contribute_instr1 = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(campaign.pubkey(), false),
            AccountMeta::new(contribution_pda, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data: borsh::to_vec(&CrowdfundingInstruction::Contribute { amount: 60_000_000 }).unwrap(),
    };

    let mut tx = Transaction::new_with_payer(&[contribute_instr1], Some(&payer.pubkey()));
    let blockhash = client.get_latest_blockhash().unwrap();
    tx.sign(&[&payer], blockhash);
    let sig = client.send_and_confirm_transaction(&tx).unwrap();
    println!("Contribution 1 (60,000,000) TX: {}", sig);

    // 3. Contribute 50,000,000 lamports (0.05 SOL) -> should succeed (total 110,000,000 > goal of 100,000,000)
    let contribute_instr2 = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(campaign.pubkey(), false),
            AccountMeta::new(contribution_pda, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data: borsh::to_vec(&CrowdfundingInstruction::Contribute { amount: 50_000_000 }).unwrap(),
    };

    let mut tx = Transaction::new_with_payer(&[contribute_instr2], Some(&payer.pubkey()));
    let blockhash = client.get_latest_blockhash().unwrap();
    tx.sign(&[&payer], blockhash);
    let sig = client.send_and_confirm_transaction(&tx).unwrap();
    println!("Contribution 2 (500) TX: {}", sig);

    // 4. Try withdraw before deadline -> should fail
    let withdraw_instr = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(campaign.pubkey(), false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data: borsh::to_vec(&CrowdfundingInstruction::Withdraw).unwrap(),
    };

    let mut tx = Transaction::new_with_payer(&[withdraw_instr.clone()], Some(&payer.pubkey()));
    let blockhash = client.get_latest_blockhash().unwrap();
    tx.sign(&[&payer], blockhash);
    match client.send_and_confirm_transaction(&tx) {
        Ok(sig) => println!("Early withdrawal succeeded (UNEXPECTED): {}", sig),
        Err(_e) => println!("Early withdrawal failed as expected!"),
    }

    // 5. Wait until after deadline
    println!("Waiting for deadline to pass...");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let wait_time = (deadline - now).max(0) as u64 + 2; // +2 buffer
    sleep(Duration::from_secs(wait_time));

    // Withdraw should succeed
    let mut tx = Transaction::new_with_payer(&[withdraw_instr.clone()], Some(&payer.pubkey()));
    let blockhash = client.get_latest_blockhash().unwrap();
    tx.sign(&[&payer], blockhash);
    let sig = client.send_and_confirm_transaction(&tx).unwrap();
    println!("Valid withdrawal TX: {}", sig);

    // 6. Try withdraw again -> should fail (already claimed)
    let mut tx = Transaction::new_with_payer(&[withdraw_instr], Some(&payer.pubkey()));
    let blockhash = client.get_latest_blockhash().unwrap();
    tx.sign(&[&payer], blockhash);
    match client.send_and_confirm_transaction(&tx) {
        Ok(sig) => println!("Double withdrawal succeeded (UNEXPECTED): {}", sig),
        Err(_e) => println!("Double withdrawal failed as expected!"),
    }
}
