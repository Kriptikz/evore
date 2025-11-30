use evore::{entropy_api::{self, var_pda, Var}, ore_api::{self, board_pda, config_pda, miner_pda, round_pda, treasury_tokens_address, Board, Miner, INTERMISSION_SLOTS, MINT_ADDRESS, TREASURY_ADDRESS}, state::{managed_miner_auth_pda, Manager}};
use solana_program::{rent::Rent, system_instruction};
use solana_program_test::{processor, read_file, ProgramTest, ProgramTestContext};
use solana_sdk::{
    account::Account, compute_budget::ComputeBudgetInstruction, pubkey, signature::Keypair, signer::Signer, transaction::Transaction
};
use steel::{AccountDeserialize, Discriminator, Numeric};

const TEST_ROUND_ID: u64 = 70149;

#[tokio::test]
pub async fn test_init() {

    let round_pda = round_pda(TEST_ROUND_ID).0;
    println!("ROUND PDA: {}", round_pda.to_string());
    let config_pda = config_pda();
    println!("CONFIG PDA: {}", config_pda.0.to_string());

    println!("TREASURY ADDRESS: {}", TREASURY_ADDRESS.to_string());

    println!("TREASURY ATA {}", treasury_tokens_address());


    println!("TOKEN PROGRAM: {}", spl_token::id().to_string());
    init_program().await;
}

#[tokio::test]
pub async fn test_deploy() {
    let (mut context, miner, manager_keypair) = init_program().await;

    // Send miner sol
    let ix0 = system_instruction::transfer(&context.payer.pubkey(), &miner.pubkey(), 1000000000);

    let blockhash = context
        .banks_client
        .get_latest_blockhash()
        .await
        .expect("should get latest blockhash");

    let mut tx = Transaction::new_with_payer(&[ix0], Some(&context.payer.pubkey()));
    tx.sign(&[&context.payer], blockhash);

    context
        .banks_client
        .process_transaction(tx)
        .await
        .expect("process_transaction should be ok");

    let manager_account = manager_keypair.pubkey();
    let managed_miner_auth_account = managed_miner_auth_pda(manager_account, 1);

    let ore_miner_account = miner_pda(managed_miner_auth_account.0);

    // Send Managed Miner SOL
    let cu_limit = 1_400_000;

    let cu_limit_ix =
        ComputeBudgetInstruction::set_compute_unit_limit(cu_limit);
 
    let bankroll: u64 = 300_000_000; // 0.3SOL
    let min_bet: u64 = 10_000;   // 0.00001 SOL
    // 0.1 SOL cap per square
    let max_per_square: u64 = 100_000_000;

    // >>> Tune ore value here:
    // 1 ORE ~= 2 SOL   -> 2_000_000_000
    // 1 ORE ~= 1 SOL   -> 1_000_000_000
    // 1 ORE ~= 0.1 SOL ->   100_000_000
    // 1 ORE ~= 0 SOL   -> 0 (pure SOL EV)
    // 1 ORE ~= 0.8 SOL
    let ore_value_lamports: u64 = 800_000_000;

    let slots_left = 2;

    let auth_id = 1;
    let ix0 = system_instruction::transfer(&context.payer.pubkey(), &managed_miner_auth_account.0, 1_000_000_000);
    let ix1 = evore::instruction::create_manager(miner.pubkey(), manager_account);
    let ix2 = evore::instruction::ev_deploy(
        miner.pubkey(),
        manager_account,
        auth_id,
        TEST_ROUND_ID,
        bankroll,
        max_per_square,
        min_bet,
        ore_value_lamports,
        slots_left
    );

    let mut tx = Transaction::new_with_payer(&[cu_limit_ix, ix0, ix1, ix2], Some(&miner.pubkey()));

    let blockhash = context
        .banks_client
        .get_latest_blockhash()
        .await
        .expect("should get latest blockhash");

    tx.sign(&[&miner, &manager_keypair, &context.payer], blockhash);

    context
        .banks_client
        .process_transaction(tx)
        .await
        .expect("process_transaction should be ok");

    // Verify evore::MangedMiner data
    let manager = context
        .banks_client
        .get_account(manager_account)
        .await
        .unwrap()
        .unwrap();
    let manager = Manager::try_from_bytes(&manager.data).unwrap();
    assert_eq!(manager.authority, miner.pubkey());

    // Verify ore::Miner data
    let ore_miner = context
        .banks_client
        .get_account(ore_miner_account.0)
        .await
        .unwrap()
        .unwrap();
    let ore_miner = ore_api::Miner::try_from_bytes(&ore_miner.data).unwrap();

    let board = context
        .banks_client
        .get_account(board_pda().0)
        .await
        .unwrap()
        .unwrap();
    let board = ore_api::Board::try_from_bytes(&board.data).unwrap();

    let _ = context.warp_to_slot(board.end_slot + INTERMISSION_SLOTS + 10);

    let config_fee_collect = pubkey!("DyB4Kv6V613gp2LWQTq1dwDYHGKuUEoDHnCouGUtxFiX");

    let ix0 = ore_api::reset(context.payer.pubkey(), config_fee_collect, TEST_ROUND_ID, managed_miner_auth_account.0);

    let blockhash = context
        .banks_client
        .get_latest_blockhash()
        .await
        .expect("should get latest blockhash for reset");

    let mut tx = Transaction::new_with_payer(&[ix0], Some(&context.payer.pubkey()));
    tx.sign(&[&context.payer], blockhash);

    context
        .banks_client
        .process_transaction(tx)
        .await
        .expect("process_transaction reset should be ok");

    let _ = context.warp_to_slot(board.end_slot + INTERMISSION_SLOTS + 10 + 20);
    let round_address = round_pda(TEST_ROUND_ID).0;
    let ix0 = system_instruction::transfer(&context.payer.pubkey(), &round_address, 100_000_000_000);
    let ix1 = system_instruction::transfer(&context.payer.pubkey(), &ore_miner_account.0, 10_000_000_000);
    let ix2 = evore::instruction::mm_checkpoint(
        miner.pubkey(),
        manager_account,
        TEST_ROUND_ID,
        auth_id,
    );
    let ix3 = evore::instruction::mm_claim_sol(
        miner.pubkey(),
        manager_account,
        auth_id,
    );
    let ix4 = evore::instruction::mm_claim_ore(
        miner.pubkey(),
        manager_account,
        auth_id,
    );

    let mut tx = Transaction::new_with_payer(&[ix0, ix1, ix2, ix3, ix4], Some(&miner.pubkey()));

    let blockhash = context
        .banks_client
        .get_latest_blockhash()
        .await
        .expect("should get latest blockhash");

    tx.sign(&[&miner, &context.payer], blockhash);

    context
        .banks_client
        .process_transaction(tx)
        .await
        .expect("process_transaction should be ok");
}


pub async fn init_program() -> (ProgramTestContext, Keypair, Keypair) {
    let mut program_test = ProgramTest::new(
        "evore",
        evore::id(),
        processor!(evore::process_instruction),
    );

    // Add Ore Program account
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

    // Treasury Account
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

    // Mint Account
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

    // Treasury AT Account
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

    // Add Entropy Program account
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


    let c_var = Var {
        authority: pubkey!("BrcSxdp1nXFzou1YyDnQJcPNBNHgoypZmTsyKBSLLXzi"),
        id: 0,
        provider: pubkey!("AKBXJ7jQ2DiqLQKzgPn791r1ZVNvLchTFH6kpesPAAWF"),
        commit: [255, 8, 38, 129, 68, 179, 50, 246, 181, 212, 33, 196, 78, 70, 219, 148, 201, 87, 24, 84, 153, 232, 51, 229, 213, 243, 65, 3, 78, 115, 50, 42],
        seed: [0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        slot_hash: [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        value: [0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        samples: 969807,
        is_auto: 0,
        start_at: 383419391,
        end_at: 383419541
    };

    let mut data = Vec::new();
    let discr = (Var::discriminator() as u64).to_le_bytes();
    for b in discr {
        data.push(b);
    }
    for b in c_var.to_bytes() {
        data.push(*b);
    }
    program_test.add_account(
        var_pda(board_pda().0, 0).0,
        Account {
            lamports: Rent::default().minimum_balance(data.len()).max(1),
            data,
            owner: entropy_api::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    // Create initial Board Account
    let data = read_file(&"tests/buffers/board_account.so");
    let board_data = data.clone();
    let board = Board::try_from_bytes(&board_data[..]).unwrap();
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

    // Create initial Round Account
    let data = read_file(&"tests/buffers/round_account.so");
    program_test.add_account(
        round_pda(TEST_ROUND_ID).0,
        Account {
            lamports: Rent::default().minimum_balance(data.len()).max(1),
            data: data,
            owner: ore_api::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    // Create initial Config Account
    let data = read_file(&"tests/buffers/config_account.so");
    program_test.add_account(
        config_pda().0,
        Account {
            lamports: Rent::default().minimum_balance(data.len()).max(1),
            data: data,
            owner: ore_api::id(),
            executable: false,
            rent_epoch: 0,
        },
    );

    let miner = Keypair::new();
    let manager_keypair = Keypair::new();
    // Create miner account
    let manager_address = manager_keypair.pubkey();
    let managed_miner_auth_address = managed_miner_auth_pda(manager_address, 1).0;

    let new_miner = Miner {
        authority: managed_miner_auth_address,
        deployed: [0; 25],
        cumulative: [0; 25],
        checkpoint_fee: 10000,
        checkpoint_id: TEST_ROUND_ID - 1,
        last_claim_ore_at: 0,
        last_claim_sol_at: 0,
        rewards_factor: Numeric::ZERO,
        rewards_sol: 1_000_000_000,
        rewards_ore: 1_000_000_000_000, // 10
        refined_ore: 100_000_000_000, // 1
        round_id: TEST_ROUND_ID - 1,
        lifetime_rewards_sol: 0,
        lifetime_rewards_ore: 0,
    };

    let mut data = Vec::new();
    let discr = (Miner::discriminator() as u64).to_le_bytes();
    for b in discr {
        data.push(b);
    }
    for b in new_miner.to_bytes() {
        data.push(*b);
    }

    let ore_miner_address = miner_pda(managed_miner_auth_address).0;

    program_test.add_account(
        ore_miner_address,
        Account {
            lamports: Rent::default().minimum_balance(data.len()).max(1),
            data: data,
            owner: ore_api::id(),
            executable: false,
            rent_epoch: 0,
        },
    );
    let mut context = program_test.start_with_context().await;
    let _ = context.warp_to_slot(board.end_slot - 2);

    (context, miner, manager_keypair)
}
