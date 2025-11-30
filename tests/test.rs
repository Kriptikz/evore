use evore::{
    consts::FEE_COLLECTOR,
    entropy_api::{self, var_pda, Var},
    ore_api::{
        self, board_pda, config_pda, miner_pda, round_pda,
        Board, Miner, Round, MINT_ADDRESS, TREASURY_ADDRESS,
    },
    state::{managed_miner_auth_pda, Manager, EvoreAccount},
};
use solana_program::{rent::Rent, system_instruction};
use solana_program_test::{processor, read_file, ProgramTest};
use solana_sdk::{
    account::Account, compute_budget::ComputeBudgetInstruction, pubkey,
    pubkey::Pubkey, signature::Keypair, signer::Signer, transaction::Transaction,
};
use steel::{AccountDeserialize, Numeric};

// ============================================================================
// Constants
// ============================================================================

const TEST_ROUND_ID: u64 = 70149;

// ============================================================================
// Test Setup - Programs Only
// ============================================================================

/// Sets up the program test with only the required programs (no accounts).
/// Returns ProgramTest before starting - caller adds accounts and starts context.
pub fn setup_programs() -> ProgramTest {
    let mut program_test = ProgramTest::new(
        "evore",
        evore::id(),
        processor!(evore::process_instruction),
    );

    // Add Ore Program
    let data = read_file(&"tests/buffers/oreV3.so");
    program_test.add_account(
        ore_api::id(),
        Account {
            lamports: Rent::default().minimum_balance(data.len()).max(1),
            data,
            owner: solana_sdk::bpf_loader::id(),
            executable: true,
            rent_epoch: 0,
        },
    );

    // Add Entropy Program
    let data = read_file(&"tests/buffers/entropy.so");
    program_test.add_account(
        entropy_api::id(),
        Account {
            lamports: Rent::default().minimum_balance(data.len()).max(1),
            data,
            owner: solana_sdk::bpf_loader::id(),
            executable: true,
            rent_epoch: 0,
        },
    );

    program_test
}

// ============================================================================
// Evore Account Helpers
// ============================================================================

/// Creates an Evore Manager account with specified authority
pub fn add_manager_account(
    program_test: &mut ProgramTest,
    manager_address: Pubkey,
    authority: Pubkey,
) {
    let manager = Manager { authority };
    
    let mut data = Vec::new();
    let discr = (EvoreAccount::Manager as u64).to_le_bytes();
    data.extend_from_slice(&discr);
    data.extend_from_slice(manager.to_bytes());
    
    program_test.add_account(
        manager_address,
        Account {
            lamports: Rent::default().minimum_balance(data.len()).max(1),
            data,
            owner: evore::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
}

// ============================================================================
// ORE Account Helpers - Configurable State
// ============================================================================

/// Creates an ORE Board account with specified state
pub fn add_board_account(
    program_test: &mut ProgramTest,
    round_id: u64,
    start_slot: u64,
    end_slot: u64,
) -> Board {
    let board = Board {
        round_id,
        start_slot,
        end_slot,
    };
    
    let mut data = Vec::new();
    let discr = (ore_api::OreAccount::Board as u64).to_le_bytes();
    data.extend_from_slice(&discr);
    data.extend_from_slice(board.to_bytes());
    
    program_test.add_account(
        board_pda().0,
        Account {
            lamports: Rent::default().minimum_balance(data.len()).max(1),
            data,
            owner: ore_api::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    
    board
}

/// Creates an ORE Round account with specified state
pub fn add_round_account(
    program_test: &mut ProgramTest,
    round_id: u64,
    deployed: [u64; 25],
    total_deployed: u64,
    expires_at: u64,
) {
    let round = Round {
        id: round_id,
        deployed,
        slot_hash: [0u8; 32],
        count: [0u64; 25],
        expires_at,
        motherlode: 0,
        rent_payer: Pubkey::default(),
        top_miner: Pubkey::default(),
        top_miner_reward: 0,
        total_deployed,
        total_vaulted: 0,
        total_winnings: 0,
    };
    
    let mut data = Vec::new();
    let discr = (ore_api::OreAccount::Round as u64).to_le_bytes();
    data.extend_from_slice(&discr);
    data.extend_from_slice(round.to_bytes());
    
    program_test.add_account(
        round_pda(round_id).0,
        Account {
            lamports: Rent::default().minimum_balance(data.len()).max(1),
            data,
            owner: ore_api::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
}

/// Creates an ORE Miner account with specified state
pub fn add_ore_miner_account(
    program_test: &mut ProgramTest,
    authority: Pubkey,
    deployed: [u64; 25],
    rewards_sol: u64,
    rewards_ore: u64,
    checkpoint_id: u64,
    round_id: u64,
) {
    let miner = Miner {
        authority,
        deployed,
        cumulative: [0; 25],
        checkpoint_fee: 10000,
        checkpoint_id,
        last_claim_ore_at: 0,
        last_claim_sol_at: 0,
        rewards_factor: Numeric::ZERO,
        rewards_sol,
        rewards_ore,
        refined_ore: 0,
        round_id,
        lifetime_rewards_sol: 0,
        lifetime_rewards_ore: 0,
    };

    let mut data = Vec::new();
    let discr = (ore_api::OreAccount::Miner as u64).to_le_bytes();
    data.extend_from_slice(&discr);
    data.extend_from_slice(miner.to_bytes());

    program_test.add_account(
        miner_pda(authority).0,
        Account {
            lamports: Rent::default().minimum_balance(data.len()).max(1),
            data,
            owner: ore_api::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
}

/// Creates an Entropy Var account with specified state
pub fn add_entropy_var_account(
    program_test: &mut ProgramTest,
    board_address: Pubkey,
    end_at: u64,
) {
    let var = Var {
        authority: board_address,
        id: 0,
        provider: Pubkey::default(),
        commit: [0u8; 32],
        seed: [0u8; 32],
        slot_hash: [0u8; 32],
        value: [0u8; 32],
        samples: 0,
        is_auto: 0,
        start_at: 0,
        end_at,
    };

    let mut data = Vec::new();
    let discr = (entropy_api::EntropyAccount::Var as u64).to_le_bytes();
    data.extend_from_slice(&discr);
    data.extend_from_slice(var.to_bytes());

    program_test.add_account(
        var_pda(board_address, 0).0,
        Account {
            lamports: Rent::default().minimum_balance(data.len()).max(1),
            data,
            owner: entropy_api::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
}

// ============================================================================
// ORE Account Helpers - From Snapshots (for complex state)
// ============================================================================

/// Adds the ORE Treasury account from snapshot
pub fn add_treasury_account(program_test: &mut ProgramTest) {
    let data = read_file(&"tests/buffers/treasury_account.so");
    program_test.add_account(
        TREASURY_ADDRESS,
        Account {
            lamports: Rent::default().minimum_balance(data.len()).max(1),
            data,
            owner: ore_api::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
}

/// Adds the ORE Mint account from snapshot
pub fn add_mint_account(program_test: &mut ProgramTest) {
    let data = read_file(&"tests/buffers/mint_account.so");
    program_test.add_account(
        MINT_ADDRESS,
        Account {
            lamports: Rent::default().minimum_balance(data.len()).max(1),
            data,
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
}

/// Adds the Treasury ATA account from snapshot
pub fn add_treasury_ata_account(program_test: &mut ProgramTest) {
    let data = read_file(&"tests/buffers/treasury_at_account.so");
    program_test.add_account(
        pubkey!("GwZS8yBuPPkPgY4uh7eEhHN5EEdpkf7EBZ1za6nuP3wF"),
        Account {
            lamports: Rent::default().minimum_balance(data.len()).max(1),
            data,
            owner: spl_token::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
}

/// Adds the Config account from snapshot
pub fn add_config_account(program_test: &mut ProgramTest) {
    let data = read_file(&"tests/buffers/config_account.so");
    program_test.add_account(
        config_pda().0,
        Account {
            lamports: Rent::default().minimum_balance(data.len()).max(1),
            data,
            owner: ore_api::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
}

// ============================================================================
// Convenience Helpers
// ============================================================================

/// Sets up common ORE accounts needed for deploy tests
/// Returns the board for slot reference
pub fn setup_deploy_test_accounts(
    program_test: &mut ProgramTest,
    round_id: u64,
    current_slot: u64,
    slots_until_end: u64,
) -> Board {
    let end_slot = current_slot + slots_until_end;
    
    // Board with specified timing
    let board = add_board_account(program_test, round_id, current_slot, end_slot);
    
    // Round with some existing deployments (so EV calc has something to work with)
    let mut deployed = [0u64; 25];
    deployed[0] = 1_000_000_000; // 1 SOL on square 0
    deployed[5] = 500_000_000;   // 0.5 SOL on square 5
    add_round_account(program_test, round_id, deployed, 1_500_000_000, end_slot + 1000);
    
    // Entropy var
    add_entropy_var_account(program_test, board_pda().0, end_slot);
    
    // Other required accounts
    add_treasury_account(program_test);
    add_mint_account(program_test);
    add_treasury_ata_account(program_test);
    add_config_account(program_test);
    
    board
}

// ============================================================================
// Tests
// ============================================================================

mod create_manager {
    use super::*;

    #[tokio::test]
    async fn test_success() {
        let program_test = setup_programs();
        let context = program_test.start_with_context().await;
        
        let miner = Keypair::new();
        let manager_keypair = Keypair::new();
        
        // Fund the miner
        let ix = system_instruction::transfer(
            &context.payer.pubkey(),
            &miner.pubkey(),
            1_000_000_000,
        );
        let blockhash = context.banks_client.get_latest_blockhash().await.unwrap();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&context.payer.pubkey()),
            &[&context.payer],
            blockhash,
        );
        context.banks_client.process_transaction(tx).await.unwrap();
        
        // Create manager
        let ix = evore::instruction::create_manager(miner.pubkey(), manager_keypair.pubkey());
        let blockhash = context.banks_client.get_latest_blockhash().await.unwrap();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&miner.pubkey()),
            &[&miner, &manager_keypair],
            blockhash,
        );
        context.banks_client.process_transaction(tx).await.expect("create_manager should succeed");
        
        // Verify
        let manager_account = context.banks_client
            .get_account(manager_keypair.pubkey())
            .await
            .unwrap()
            .unwrap();
        let manager = Manager::try_from_bytes(&manager_account.data).unwrap();
        assert_eq!(manager.authority, miner.pubkey());
    }
    
    #[tokio::test]
    async fn test_already_initialized() {
        let mut program_test = setup_programs();
        
        let miner = Keypair::new();
        let manager_keypair = Keypair::new();
        
        // Pre-create the manager account
        add_manager_account(&mut program_test, manager_keypair.pubkey(), miner.pubkey());
        
        let context = program_test.start_with_context().await;
        
        // Fund the miner
        let ix = system_instruction::transfer(
            &context.payer.pubkey(),
            &miner.pubkey(),
            1_000_000_000,
        );
        let blockhash = context.banks_client.get_latest_blockhash().await.unwrap();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&context.payer.pubkey()),
            &[&context.payer],
            blockhash,
        );
        context.banks_client.process_transaction(tx).await.unwrap();
        
        // Try to create manager again - should fail
        let ix = evore::instruction::create_manager(miner.pubkey(), manager_keypair.pubkey());
        let blockhash = context.banks_client.get_latest_blockhash().await.unwrap();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&miner.pubkey()),
            &[&miner, &manager_keypair],
            blockhash,
        );
        let result = context.banks_client.process_transaction(tx).await;
        assert!(result.is_err(), "should fail when manager already exists");
    }
}

mod ev_deploy {
    use super::*;

    #[tokio::test]
    async fn test_success() {
        let mut program_test = setup_programs();
        
        let miner = Keypair::new();
        let manager_keypair = Keypair::new();
        let manager_address = manager_keypair.pubkey();
        let auth_id = 1u64;
        let managed_miner_auth = managed_miner_auth_pda(manager_address, auth_id);
        
        // Setup accounts - round ending in 5 slots
        let current_slot = 1000;
        let _board = setup_deploy_test_accounts(&mut program_test, TEST_ROUND_ID, current_slot, 5);
        
        // Add ore miner for our managed auth
        add_ore_miner_account(
            &mut program_test,
            managed_miner_auth.0,
            [0u64; 25],
            0, 0,
            TEST_ROUND_ID - 1,
            TEST_ROUND_ID - 1,
        );
        
        let mut context = program_test.start_with_context().await;
        let _ = context.warp_to_slot(current_slot + 3); // 2 slots left
        
        // Fund accounts
        let ix0 = system_instruction::transfer(&context.payer.pubkey(), &miner.pubkey(), 2_000_000_000);
        let ix1 = system_instruction::transfer(&context.payer.pubkey(), &managed_miner_auth.0, 1_000_000_000);
        let ix2 = system_instruction::transfer(&context.payer.pubkey(), &FEE_COLLECTOR, 1_000_000);
        let blockhash = context.banks_client.get_latest_blockhash().await.unwrap();
        let tx = Transaction::new_signed_with_payer(
            &[ix0, ix1, ix2],
            Some(&context.payer.pubkey()),
            &[&context.payer],
            blockhash,
        );
        context.banks_client.process_transaction(tx).await.unwrap();
        
        // Create manager and deploy
        let cu_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(1_400_000);
        let ix1 = evore::instruction::create_manager(miner.pubkey(), manager_address);
        let ix2 = evore::instruction::ev_deploy(
            miner.pubkey(),
            manager_address,
            auth_id,
            TEST_ROUND_ID,
            300_000_000,  // bankroll
            100_000_000,  // max_per_square
            10_000,       // min_bet
            800_000_000,  // ore_value
            2,            // slots_left threshold
        );
        
        let blockhash = context.banks_client.get_latest_blockhash().await.unwrap();
        let tx = Transaction::new_signed_with_payer(
            &[cu_limit_ix, ix1, ix2],
            Some(&miner.pubkey()),
            &[&miner, &manager_keypair],
            blockhash,
        );
        context.banks_client.process_transaction(tx).await.expect("deploy should succeed");
        
        // Verify manager was created
        let manager = context.banks_client.get_account(manager_address).await.unwrap().unwrap();
        let manager = Manager::try_from_bytes(&manager.data).unwrap();
        assert_eq!(manager.authority, miner.pubkey());
    }

    #[tokio::test]
    async fn test_too_many_slots_left() {
        let mut program_test = setup_programs();
        
        let miner = Keypair::new();
        let manager_keypair = Keypair::new();
        let manager_address = manager_keypair.pubkey();
        let auth_id = 1u64;
        let managed_miner_auth = managed_miner_auth_pda(manager_address, auth_id);
        
        // Setup accounts - round ending in 100 slots (too many)
        let current_slot = 1000;
        let _board = setup_deploy_test_accounts(&mut program_test, TEST_ROUND_ID, current_slot, 100);
        
        add_ore_miner_account(
            &mut program_test,
            managed_miner_auth.0,
            [0u64; 25], 0, 0,
            TEST_ROUND_ID - 1, TEST_ROUND_ID - 1,
        );
        
        let mut context = program_test.start_with_context().await;
        let _ = context.warp_to_slot(current_slot + 10); // Still 90 slots left
        
        // Fund
        let ix0 = system_instruction::transfer(&context.payer.pubkey(), &miner.pubkey(), 2_000_000_000);
        let ix1 = system_instruction::transfer(&context.payer.pubkey(), &managed_miner_auth.0, 1_000_000_000);
        let blockhash = context.banks_client.get_latest_blockhash().await.unwrap();
        let tx = Transaction::new_signed_with_payer(&[ix0, ix1], Some(&context.payer.pubkey()), &[&context.payer], blockhash);
        context.banks_client.process_transaction(tx).await.unwrap();
        
        // Try to deploy with slots_left=2 when there are 90 slots left
        let ix1 = evore::instruction::create_manager(miner.pubkey(), manager_address);
        let ix2 = evore::instruction::ev_deploy(
            miner.pubkey(), manager_address, auth_id, TEST_ROUND_ID,
            300_000_000, 100_000_000, 10_000, 800_000_000,
            2,  // slots_left threshold - but there are 90 slots left!
        );
        
        let blockhash = context.banks_client.get_latest_blockhash().await.unwrap();
        let tx = Transaction::new_signed_with_payer(&[ix1, ix2], Some(&miner.pubkey()), &[&miner, &manager_keypair], blockhash);
        let result = context.banks_client.process_transaction(tx).await;
        assert!(result.is_err(), "should fail when too many slots left");
    }
    
    #[tokio::test]
    async fn test_wrong_authority() {
        let mut program_test = setup_programs();
        
        let miner = Keypair::new();
        let wrong_signer = Keypair::new();
        let manager_keypair = Keypair::new();
        let manager_address = manager_keypair.pubkey();
        let auth_id = 1u64;
        let managed_miner_auth = managed_miner_auth_pda(manager_address, auth_id);
        
        // Pre-create manager with miner as authority
        add_manager_account(&mut program_test, manager_address, miner.pubkey());
        
        let current_slot = 1000;
        let _board = setup_deploy_test_accounts(&mut program_test, TEST_ROUND_ID, current_slot, 5);
        add_ore_miner_account(&mut program_test, managed_miner_auth.0, [0u64; 25], 0, 0, TEST_ROUND_ID - 1, TEST_ROUND_ID - 1);
        
        let mut context = program_test.start_with_context().await;
        let _ = context.warp_to_slot(current_slot + 3);
        
        // Fund wrong_signer
        let ix = system_instruction::transfer(&context.payer.pubkey(), &wrong_signer.pubkey(), 2_000_000_000);
        let blockhash = context.banks_client.get_latest_blockhash().await.unwrap();
        let tx = Transaction::new_signed_with_payer(&[ix], Some(&context.payer.pubkey()), &[&context.payer], blockhash);
        context.banks_client.process_transaction(tx).await.unwrap();
        
        // Try to deploy with wrong signer (not the manager authority)
        let ix = evore::instruction::ev_deploy(
            wrong_signer.pubkey(), manager_address, auth_id, TEST_ROUND_ID,
            300_000_000, 100_000_000, 10_000, 800_000_000, 2,
        );
        
        let blockhash = context.banks_client.get_latest_blockhash().await.unwrap();
        let tx = Transaction::new_signed_with_payer(&[ix], Some(&wrong_signer.pubkey()), &[&wrong_signer], blockhash);
        let result = context.banks_client.process_transaction(tx).await;
        assert!(result.is_err(), "should fail with wrong authority");
    }
}

mod checkpoint {
    use super::*;

    #[tokio::test]
    async fn test_wrong_authority() {
        let mut program_test = setup_programs();
        
        let miner = Keypair::new();
        let wrong_signer = Keypair::new();
        let manager_keypair = Keypair::new();
        let manager_address = manager_keypair.pubkey();
        let auth_id = 1u64;
        let managed_miner_auth = managed_miner_auth_pda(manager_address, auth_id);
        
        // Pre-create manager with miner as authority
        add_manager_account(&mut program_test, manager_address, miner.pubkey());
        
        // Setup minimal accounts
        let current_slot = 1000;
        add_board_account(&mut program_test, TEST_ROUND_ID, current_slot, current_slot + 100);
        add_round_account(&mut program_test, TEST_ROUND_ID, [0u64; 25], 0, current_slot + 1000);
        add_treasury_account(&mut program_test);
        add_ore_miner_account(&mut program_test, managed_miner_auth.0, [0u64; 25], 0, 0, TEST_ROUND_ID - 1, TEST_ROUND_ID - 1);
        
        let context = program_test.start_with_context().await;
        
        // Fund wrong_signer
        let ix = system_instruction::transfer(&context.payer.pubkey(), &wrong_signer.pubkey(), 1_000_000_000);
        let blockhash = context.banks_client.get_latest_blockhash().await.unwrap();
        let tx = Transaction::new_signed_with_payer(&[ix], Some(&context.payer.pubkey()), &[&context.payer], blockhash);
        context.banks_client.process_transaction(tx).await.unwrap();
        
        // Try checkpoint with wrong authority
        let ix = evore::instruction::mm_checkpoint(wrong_signer.pubkey(), manager_address, TEST_ROUND_ID, auth_id);
        let blockhash = context.banks_client.get_latest_blockhash().await.unwrap();
        let tx = Transaction::new_signed_with_payer(&[ix], Some(&wrong_signer.pubkey()), &[&wrong_signer], blockhash);
        let result = context.banks_client.process_transaction(tx).await;
        assert!(result.is_err(), "should fail with wrong authority");
    }
}

mod claim_sol {
    use super::*;

    #[tokio::test]
    async fn test_wrong_authority() {
        let mut program_test = setup_programs();
        
        let miner = Keypair::new();
        let wrong_signer = Keypair::new();
        let manager_keypair = Keypair::new();
        let manager_address = manager_keypair.pubkey();
        let auth_id = 1u64;
        let managed_miner_auth = managed_miner_auth_pda(manager_address, auth_id);
        
        // Pre-create manager with miner as authority
        add_manager_account(&mut program_test, manager_address, miner.pubkey());
        add_ore_miner_account(&mut program_test, managed_miner_auth.0, [0u64; 25], 1_000_000_000, 0, TEST_ROUND_ID - 1, TEST_ROUND_ID - 1);
        
        let context = program_test.start_with_context().await;
        
        // Fund wrong_signer
        let ix = system_instruction::transfer(&context.payer.pubkey(), &wrong_signer.pubkey(), 1_000_000_000);
        let blockhash = context.banks_client.get_latest_blockhash().await.unwrap();
        let tx = Transaction::new_signed_with_payer(&[ix], Some(&context.payer.pubkey()), &[&context.payer], blockhash);
        context.banks_client.process_transaction(tx).await.unwrap();
        
        // Try claim_sol with wrong authority
        let ix = evore::instruction::mm_claim_sol(wrong_signer.pubkey(), manager_address, auth_id);
        let blockhash = context.banks_client.get_latest_blockhash().await.unwrap();
        let tx = Transaction::new_signed_with_payer(&[ix], Some(&wrong_signer.pubkey()), &[&wrong_signer], blockhash);
        let result = context.banks_client.process_transaction(tx).await;
        assert!(result.is_err(), "should fail with wrong authority");
    }
}

mod claim_ore {
    use super::*;

    #[tokio::test]
    async fn test_wrong_authority() {
        let mut program_test = setup_programs();
        
        let miner = Keypair::new();
        let wrong_signer = Keypair::new();
        let manager_keypair = Keypair::new();
        let manager_address = manager_keypair.pubkey();
        let auth_id = 1u64;
        let managed_miner_auth = managed_miner_auth_pda(manager_address, auth_id);
        
        // Pre-create manager with miner as authority
        add_manager_account(&mut program_test, manager_address, miner.pubkey());
        add_ore_miner_account(&mut program_test, managed_miner_auth.0, [0u64; 25], 0, 1_000_000_000, TEST_ROUND_ID - 1, TEST_ROUND_ID - 1);
        add_treasury_account(&mut program_test);
        add_mint_account(&mut program_test);
        add_treasury_ata_account(&mut program_test);
        
        let context = program_test.start_with_context().await;
        
        // Fund wrong_signer
        let ix = system_instruction::transfer(&context.payer.pubkey(), &wrong_signer.pubkey(), 1_000_000_000);
        let blockhash = context.banks_client.get_latest_blockhash().await.unwrap();
        let tx = Transaction::new_signed_with_payer(&[ix], Some(&context.payer.pubkey()), &[&context.payer], blockhash);
        context.banks_client.process_transaction(tx).await.unwrap();
        
        // Try claim_ore with wrong authority
        let ix = evore::instruction::mm_claim_ore(wrong_signer.pubkey(), manager_address, auth_id);
        let blockhash = context.banks_client.get_latest_blockhash().await.unwrap();
        let tx = Transaction::new_signed_with_payer(&[ix], Some(&wrong_signer.pubkey()), &[&wrong_signer], blockhash);
        let result = context.banks_client.process_transaction(tx).await;
        assert!(result.is_err(), "should fail with wrong authority");
    }
}
