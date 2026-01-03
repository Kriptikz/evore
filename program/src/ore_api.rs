use solana_program::pubkey;
use serde::{Serialize, Deserialize};
use spl_associated_token_account::get_associated_token_address;
use steel::*;

use crate::entropy_api;

pub const PROGRAM_ID: Pubkey = pubkey!("oreV3EG1i9BEgiAJ8b177Z2S2rMarzak4NMv1kULvWv");

/// The seed of the board account PDA.
pub const BOARD: &[u8] = b"board";

/// The seed of the miner account PDA.
pub const MINER: &[u8] = b"miner";

/// The seed of the round account PDA.
pub const ROUND: &[u8] = b"round";

/// The seed of the config account PDA.
pub const CONFIG: &[u8] = b"config";

/// The seed of the automation account PDA.
pub const AUTOMATION: &[u8] = b"automation";

/// The seed of the treasury account PDA.
pub const TREASURY: &[u8] = b"treasury";

/// The seed of the stake account PDA.
pub const STAKE: &[u8] = b"stake";

/// The address of the treasury account.
pub const TREASURY_ADDRESS: Pubkey = pubkey!("45db2FSR4mcXdSVVZbKbwojU6uYDpMyhpEi7cC8nHaWG");

pub const INTERMISSION_SLOTS: u64 = 35;

/// The checkpoint fee that miners must hold (in lamports)
/// This is required by ORE v3 for the checkpoint operation
pub const CHECKPOINT_FEE: u64 = 10_000; // 0.00001 SOL

/// The address of the mint account.
pub const MINT_ADDRESS: Pubkey = pubkey!("oreoU2P8bN6jkk3jbaiVxYnG1dCXcYxwhwyK9jSybcp");

pub fn id() -> Pubkey {
    PROGRAM_ID
}

pub fn board_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[BOARD], &PROGRAM_ID)
}

pub fn config_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[CONFIG], &PROGRAM_ID)
}

pub fn miner_pda(authority: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[MINER, &authority.to_bytes()], &PROGRAM_ID)
}

pub fn round_pda(id: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[ROUND, &id.to_le_bytes()], &PROGRAM_ID)
}

pub fn automation_pda(authority: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[AUTOMATION, &authority.to_bytes()], &PROGRAM_ID)
}

pub fn treasury_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[TREASURY], &PROGRAM_ID)
}

pub fn stake_pda(authority: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[STAKE, &authority.to_bytes()], &PROGRAM_ID)
}

pub fn treasury_tokens_address() -> Pubkey {
    spl_associated_token_account::get_associated_token_address(&TREASURY_ADDRESS, &MINT_ADDRESS)
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoPrimitive, TryFromPrimitive)]
pub enum OreAccount {
    Automation = 100,
    Config = 101,
    Miner = 103,
    Treasury = 104,
    Board = 105,
    Stake = 108,
    Round = 109,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct Automation {
    /// The amount of SOL to deploy on each territory per round.
    pub amount: u64,

    /// The authority of this automation account.
    pub authority: Pubkey,

    /// The amount of SOL this automation has left.
    pub balance: u64,

    /// The executor of this automation account.
    pub executor: Pubkey,

    /// The amount of SOL the executor should receive in fees.
    pub fee: u64,

    /// The strategy this automation uses.
    pub strategy: u64,

    /// The mask of squares this automation should deploy to if preferred strategy.
    /// If strategy is Random, first bit is used to determine how many squares to deploy to.
    pub mask: u64,

    /// Whether or not to auto-reload SOL winnings into the automation balance.
    pub reload: u64,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, IntoPrimitive, TryFromPrimitive)]
pub enum AutomationStrategy {
    Random = 0,
    Preferred = 1,
    Discretionary = 2,
}

impl AutomationStrategy {
    pub fn from_u64(value: u64) -> Self {
        Self::try_from(value as u8).unwrap()
    }
}

impl Automation {
    pub fn pda(&self) -> (Pubkey, u8) {
        miner_pda(self.authority)
    }
}

account!(OreAccount, Automation);

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct Board {
    /// The current round number.
    pub round_id: u64,

    /// The slot at which the current round starts mining.
    pub start_slot: u64,

    /// The slot at which the current round ends mining.
    pub end_slot: u64,

    /// The current epoch id.
    pub epoch_id: u64,
}

impl Board {
    pub fn pda(&self) -> (Pubkey, u8) {
        board_pda()
    }
}

account!(OreAccount, Board);

/// Treasury is a singleton account which is the mint authority for the ORE token and the authority of
/// Treasury is a singleton account which is the mint authority for the ORE token and the authority of
/// the program's global token account.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct Treasury {
    // The amount of SOL collected for buy-bury operations.
    pub balance: u64,

    /// Buffer a (placeholder)
    pub buffer_a: u64,

    /// The amount of ORE in the motherlode rewards pool.
    pub motherlode: u64,

    /// The cumulative ORE distributed to miners, divided by the total unclaimed ORE at the time of distribution.
    pub miner_rewards_factor: Numeric,

    /// The cumulative ORE distributed to stakers, divided by the total stake at the time of distribution.
    pub stake_rewards_factor: Numeric,

    /// Buffer b (placeholder)
    pub buffer_b: u64,

    /// The current total amount of refined ORE mining rewards.
    pub total_refined: u64,

    /// The current total amount of ORE staking deposits.
    pub total_staked: u64,

    /// The current total amount of unclaimed ORE mining rewards.
    pub total_unclaimed: u64,
}

account!(OreAccount, Treasury);


#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct Round {
    /// The round number.
    pub id: u64,

    /// The amount of SOL deployed in each square.
    pub deployed: [u64; 25],

    /// The hash of the end slot, provided by solana, used for random number generation.
    pub slot_hash: [u8; 32],

    /// The count of miners on each square.
    pub count: [u64; 25],

    /// The slot at which claims for this round account end.
    pub expires_at: u64,

    /// The amount of ORE in the motherlode.
    pub motherlode: u64,

    /// The account to which rent should be returned when this account is closed.
    pub rent_payer: Pubkey,

    /// The top miner of the round.
    pub top_miner: Pubkey,

    /// The amount of ORE to distribute to the top miner.
    pub top_miner_reward: u64,

    /// The total amount of SOL deployed in the round.
    pub total_deployed: u64,

    /// The total number of unique miners that played in the round.
    pub total_miners: u64,

    /// The total amount of SOL put in the ORE vault.
    pub total_vaulted: u64,

    /// The total amount of SOL won by miners for the round.
    pub total_winnings: u64,
}

account!(OreAccount, Round);

impl Round {
  pub fn pda(&self) -> (Pubkey, u8) {
      round_pda(self.id)
  }

  pub fn rng(&self) -> Option<u64> {
      if self.slot_hash == [0; 32] || self.slot_hash == [u8::MAX; 32] {
          return None;
      }
      let r1 = u64::from_le_bytes(self.slot_hash[0..8].try_into().unwrap());
      let r2 = u64::from_le_bytes(self.slot_hash[8..16].try_into().unwrap());
      let r3 = u64::from_le_bytes(self.slot_hash[16..24].try_into().unwrap());
      let r4 = u64::from_le_bytes(self.slot_hash[24..32].try_into().unwrap());
      let r = r1 ^ r2 ^ r3 ^ r4;
      Some(r)
  }

  pub fn winning_square(&self, rng: u64) -> usize {
      (rng % 25) as usize
  }

  pub fn top_miner_sample(&self, rng: u64, winning_square: usize) -> u64 {
      if self.deployed[winning_square] == 0 {
          return 0;
      }
      rng.reverse_bits() % self.deployed[winning_square]
  }

  pub fn calculate_total_winnings(&self, winning_square: usize) -> u64 {
      let mut total_winnings = 0;
      for (i, &deployed) in self.deployed.iter().enumerate() {
          if i != winning_square {
              total_winnings += deployed;
          }
      }
      total_winnings
  }

  pub fn is_split_reward(&self, rng: u64) -> bool {
      // One out of four rounds get split rewards.
      let rng = rng.reverse_bits().to_le_bytes();
      let r1 = u16::from_le_bytes(rng[0..2].try_into().unwrap());
      let r2 = u16::from_le_bytes(rng[2..4].try_into().unwrap());
      let r3 = u16::from_le_bytes(rng[4..6].try_into().unwrap());
      let r4 = u16::from_le_bytes(rng[6..8].try_into().unwrap());
      let r = r1 ^ r2 ^ r3 ^ r4;
      r % 2 == 0
  }

  pub fn did_hit_motherlode(&self, rng: u64) -> bool {
      rng.reverse_bits() % 625 == 0
  }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct Miner {
    /// The authority of this miner account.
    pub authority: Pubkey,

    /// The miner's prospects in the current round.
    pub deployed: [u64; 25],

    /// The cumulative amount of SOL deployed on each square prior to this miner's move.
    pub cumulative: [u64; 25],

    /// SOL witheld in reserve to pay for checkpointing.
    pub checkpoint_fee: u64,

    /// The last round that this miner checkpointed.
    pub checkpoint_id: u64,

    /// The last time this miner claimed ORE rewards.
    pub last_claim_ore_at: i64,

    /// The last time this miner claimed SOL rewards.
    pub last_claim_sol_at: i64,

    /// The rewards factor last time rewards were updated on this miner account.
    pub rewards_factor: Numeric,

    /// The amount of SOL this miner can claim.
    pub rewards_sol: u64,

    /// The amount of ORE this miner can claim.
    pub rewards_ore: u64,

    /// The amount of ORE this miner has earned from claim fees.
    pub refined_ore: u64,

    /// The ID of the round this miner last played in.
    pub round_id: u64,

    /// The total amount of SOL this miner has mined across all blocks.
    pub lifetime_rewards_sol: u64,

    /// The total amount of ORE this miner has mined across all blocks.
    pub lifetime_rewards_ore: u64,

    /// The total amount of ORE this miner has deployed across all rounds.
    pub lifetime_deployed: u64,
}

account!(OreAccount, Miner);

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct Config {
    /// The address that can update the config.
    pub admin: Pubkey,

    /// The adress with authority to call wrap and bury.
    pub bury_authority: Pubkey,

    /// The address that receives admin fees.
    pub fee_collector: Pubkey,

    /// The program to be used for protocol swaps.
    pub swap_program: Pubkey,

    /// The address of the entropy var account.
    pub var_address: Pubkey,

    /// Amount to pay to fee collector (bps)
    pub admin_fee: u64,
}

impl Config {
    pub fn pda() -> (Pubkey, u8) {
        config_pda()
    }
}

account!(OreAccount, Config);

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct Stake {
    /// The authority of this miner account.
    pub authority: Pubkey,

    /// The balance of this stake account.
    pub balance: u64,

    /// Buffer a (placeholder)
    pub buffer_a: u64,

    /// Buffer b (placeholder)
    pub buffer_b: u64,

    /// Buffer c (placeholder)
    pub buffer_c: u64,

    /// Buffer d (placeholder)
    pub buffer_d: u64,

    /// The lamport reserve to pay fees for auto-compounding bots.
    pub compound_fee_reserve: u64,

    /// The timestamp of last claim.
    pub last_claim_at: i64,

    /// The timestamp the last time this staker deposited.
    pub last_deposit_at: i64,

    /// The timestamp the last time this staker withdrew.
    pub last_withdraw_at: i64,

    /// The rewards factor last time rewards were updated on this stake account.
    pub rewards_factor: Numeric,

    /// The amount of ORE this staker can claim.
    pub rewards: u64,

    /// The total amount of ORE this staker has earned over its lifetime.
    pub lifetime_rewards: u64,

    /// Buffer f (placeholder)
    pub buffer_f: u64,
}

impl Stake {
    pub fn pda(&self) -> (Pubkey, u8) {
        stake_pda(self.authority)
    }

    pub fn claim(&mut self, amount: u64, clock: &Clock, treasury: &Treasury) -> u64 {
        self.update_rewards(treasury);
        let amount = self.rewards.min(amount);
        self.rewards -= amount;
        self.last_claim_at = clock.unix_timestamp;
        amount
    }

    pub fn deposit(
        &mut self,
        amount: u64,
        clock: &Clock,
        treasury: &mut Treasury,
        sender: &TokenAccount,
    ) -> u64 {
        self.update_rewards(treasury);
        let amount = sender.amount().min(amount);
        self.balance += amount;
        self.last_deposit_at = clock.unix_timestamp;
        treasury.total_staked += amount;
        amount
    }

    pub fn withdraw(&mut self, amount: u64, clock: &Clock, treasury: &mut Treasury) -> u64 {
        self.update_rewards(treasury);
        let amount = self.balance.min(amount);
        self.balance -= amount;
        self.last_withdraw_at = clock.unix_timestamp;
        treasury.total_staked -= amount;
        amount
    }

    pub fn update_rewards(&mut self, treasury: &Treasury) {
        // Accumulate rewards, weighted by stake balance.
        if treasury.stake_rewards_factor > self.rewards_factor {
            let accumulated_rewards = treasury.stake_rewards_factor - self.rewards_factor;
            if accumulated_rewards < Numeric::ZERO {
                panic!("Accumulated rewards is negative");
            }
            let personal_rewards = accumulated_rewards * Numeric::from_u64(self.balance);
            self.rewards += personal_rewards.to_u64();
            self.lifetime_rewards += personal_rewards.to_u64();
        }

        // Update this stake account's last seen rewards factor.
        self.rewards_factor = treasury.stake_rewards_factor;
    }
}

account!(OreAccount, Stake);

pub fn deploy(
    signer: Pubkey,
    authority: Pubkey,
    amount: u64,
    round_id: u64,
    squares: [bool; 25],
) -> Instruction {
    let automation_address = automation_pda(authority).0;
    let board_address = board_pda().0;
    let miner_address = miner_pda(authority).0;
    let round_address = round_pda(round_id).0;
    let config_address = config_pda().0;
    let entropy_var_address = entropy_api::var_pda(board_address, 0).0;

    // Convert array of 25 booleans into a 32-bit mask where each bit represents whether
    // that square index is selected (1) or not (0)
    let mut mask: u32 = 0;
    for (i, &square) in squares.iter().enumerate() {
        if square {
            mask |= 1 << i;
        }
    }

    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(authority, false),
            AccountMeta::new(automation_address, false),
            AccountMeta::new(board_address, false),
            AccountMeta::new(config_address, false),
            AccountMeta::new(miner_address, false),
            AccountMeta::new(round_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(PROGRAM_ID, false),
            // Entropy accounts.
            AccountMeta::new(entropy_var_address, false),
            AccountMeta::new_readonly(entropy_api::id(), false),
        ],
        data: Deploy {
            amount: amount.to_le_bytes(),
            squares: mask.to_le_bytes(),
        }
        .to_bytes(),
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, TryFromPrimitive)]
pub enum OreInstruction {
    // Miner
    Automate = 0,
    Checkpoint = 2,
    ClaimSOL = 3,
    ClaimORE = 4,
    Close = 5,
    Deploy = 6,
    Log = 8,
    Reset = 9,
    ReloadSOL = 21,

    // Staker
    Deposit = 10,
    Withdraw = 11,
    ClaimYield = 12,

    // Admin
    Bury = 13,
    Wrap = 14,
    SetAdmin = 15,
    SetFeeCollector = 16,
    SetSwapProgram = 17,
    SetVarAddress = 18,
    NewVar = 19,
    SetAdminFee = 20,
    MigrateAutomation = 22,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Automate {
    pub amount: [u8; 8],
    pub deposit: [u8; 8],
    pub fee: [u8; 8],
    pub mask: [u8; 8],
    pub strategy: u8,
    pub reload: [u8; 8],
}

instruction!(OreInstruction, Automate);

pub fn automate(
  signer: Pubkey,
  amount: u64,
  deposit: u64,
  executor: Pubkey,
  fee: u64,
  mask: u64,
  strategy: u8,
  reload: bool,
) -> Instruction {
  let automation_address = automation_pda(signer).0;
  let miner_address = miner_pda(signer).0;
  Instruction {
      program_id: PROGRAM_ID,
      accounts: vec![
          AccountMeta::new(signer, true),
          AccountMeta::new(automation_address, false),
          AccountMeta::new(executor, false),
          AccountMeta::new(miner_address, false),
          AccountMeta::new_readonly(system_program::ID, false),
      ],
      data: Automate {
          amount: amount.to_le_bytes(),
          deposit: deposit.to_le_bytes(),
          fee: fee.to_le_bytes(),
          mask: mask.to_le_bytes(),
          strategy: strategy as u8,
          reload: (reload as u64).to_le_bytes(),
      }
      .to_bytes(),
  }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Deploy {
    pub amount: [u8; 8],
    pub squares: [u8; 4],
}

instruction!(OreInstruction, Deploy);

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Checkpoint {}
instruction!(OreInstruction, Checkpoint);

pub fn checkpoint(signer: Pubkey, authority: Pubkey, round_id: u64) -> Instruction {
    let miner_address = miner_pda(authority).0;
    let board_address = board_pda().0;
    let round_address = round_pda(round_id).0;
    let treasury_address = TREASURY_ADDRESS;
    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(board_address, false),
            AccountMeta::new(miner_address, false),
            AccountMeta::new(round_address, false),
            AccountMeta::new(treasury_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: Checkpoint {}.to_bytes(),
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ClaimSOL {}
instruction!(OreInstruction, ClaimSOL);

pub fn claim_sol(signer: Pubkey) -> Instruction {
    let miner_address = miner_pda(signer).0;
    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(miner_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: ClaimSOL {}.to_bytes(),
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ClaimORE {}
instruction!(OreInstruction, ClaimORE);
pub fn claim_ore(signer: Pubkey) -> Instruction {
    let miner_address = miner_pda(signer).0;
    let treasury_address = treasury_pda().0;
    let treasury_tokens_address = get_associated_token_address(&treasury_address, &MINT_ADDRESS);
    let recipient_address = get_associated_token_address(&signer, &MINT_ADDRESS);

    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(miner_address, false),
            AccountMeta::new(MINT_ADDRESS, false),
            AccountMeta::new(recipient_address, false),
            AccountMeta::new(treasury_address, false),
            AccountMeta::new(treasury_tokens_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(spl_token::ID, false),
            AccountMeta::new_readonly(spl_associated_token_account::ID, false),
        ],
        data: ClaimORE {}.to_bytes(),
    }
}






#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Reset {}
instruction!(OreInstruction, Reset);

pub fn reset(
    signer: Pubkey,
    fee_collector: Pubkey,
    round_id: u64,
    top_miner: Pubkey,
) -> Instruction {
    let board_address = board_pda().0;
    let config_address = config_pda().0;
    let mint_address = MINT_ADDRESS;
    let round_address = round_pda(round_id).0;
    let round_next_address = round_pda(round_id + 1).0;
    let top_miner_address = miner_pda(top_miner).0;
    let treasury_address = TREASURY_ADDRESS;
    let treasury_tokens_address = treasury_tokens_address();
    let entropy_var_address = entropy_api::var_pda(board_address, 0).0;
    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(board_address, false),
            AccountMeta::new(config_address, false),
            AccountMeta::new(fee_collector, false),
            AccountMeta::new(mint_address, false),
            AccountMeta::new(round_address, false),
            AccountMeta::new(round_next_address, false),
            AccountMeta::new(top_miner_address, false),
            AccountMeta::new(treasury_address, false),
            AccountMeta::new(treasury_tokens_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(spl_token::ID, false),
            AccountMeta::new_readonly(PROGRAM_ID, false),
            AccountMeta::new_readonly(sysvar::slot_hashes::ID, false),
            // Entropy accounts.
            AccountMeta::new(entropy_var_address, false),
            AccountMeta::new_readonly(entropy_api::id(), false),
        ],
        data: Reset {}.to_bytes(),
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Deposit {
    pub amount: [u8; 8],
    pub compound_fee: [u8; 8],
}
instruction!(OreInstruction, Deposit);

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Withdraw {
    pub amount: [u8; 8],
}
instruction!(OreInstruction, Withdraw);

pub fn deposit(signer: Pubkey, payer: Pubkey, amount: u64, compound_fee: u64) -> Instruction {
  let mint_address = MINT_ADDRESS;
  let stake_address = stake_pda(signer).0;
  let stake_tokens_address = get_associated_token_address(&stake_address, &MINT_ADDRESS);
  let sender_address = get_associated_token_address(&signer, &MINT_ADDRESS);
  let treasury_address = TREASURY_ADDRESS;
  Instruction {
      program_id: crate::ID,
      accounts: vec![
          AccountMeta::new(signer, true),
          AccountMeta::new(payer, true),
          AccountMeta::new(mint_address, false),
          AccountMeta::new(sender_address, false),
          AccountMeta::new(stake_address, false),
          AccountMeta::new(stake_tokens_address, false),
          AccountMeta::new(treasury_address, false),
          AccountMeta::new_readonly(system_program::ID, false),
          AccountMeta::new_readonly(spl_token::ID, false),
          AccountMeta::new_readonly(spl_associated_token_account::ID, false),
      ],
      data: Deposit {
          amount: amount.to_le_bytes(),
          compound_fee: compound_fee.to_le_bytes(),
      }
      .to_bytes(),
  }
}


pub fn withdraw(signer: Pubkey, amount: u64) -> Instruction {
  let stake_address = stake_pda(signer).0;
  let stake_tokens_address = get_associated_token_address(&stake_address, &MINT_ADDRESS);
  let mint_address = MINT_ADDRESS;
  let recipient_address = get_associated_token_address(&signer, &MINT_ADDRESS);
  let treasury_address = TREASURY_ADDRESS;
  Instruction {
      program_id: crate::ID,
      accounts: vec![
          AccountMeta::new(signer, true),
          AccountMeta::new(mint_address, false),
          AccountMeta::new(recipient_address, false),
          AccountMeta::new(stake_address, false),
          AccountMeta::new(stake_tokens_address, false),
          AccountMeta::new(treasury_address, false),
          AccountMeta::new_readonly(system_program::ID, false),
          AccountMeta::new_readonly(spl_token::ID, false),
          AccountMeta::new_readonly(spl_associated_token_account::ID, false),
      ],
      data: Withdraw {
          amount: amount.to_le_bytes(),
      }
      .to_bytes(),
  }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ReloadSOL {}
instruction!(OreInstruction, ReloadSOL);

/// Build a reload_sol instruction that reloads SOL winnings into automation balance.
/// This should be called by the executor after a deploy.
pub fn reload_sol(signer: Pubkey, authority: Pubkey) -> Instruction {
    let automation_address = automation_pda(authority).0;
    let miner_address = miner_pda(authority).0;
    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(automation_address, false),
            AccountMeta::new(miner_address, false),
        ],
        data: ReloadSOL {}.to_bytes(),
    }
}



