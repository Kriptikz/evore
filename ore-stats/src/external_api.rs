use reqwest::Client;
use serde::Deserialize;
use solana_program::pubkey::Pubkey;

use crate::app_state::AppRound;

#[derive(Deserialize, Debug)]
struct ResetRoundMeta {
    disc: u8,
    round_id: u64,
    start_slot: u64,
    end_slot: u64,
    winning_square: u64,
    top_miner: [u8; 32],
    num_winners: u64,
    motherlode: u64,
    total_deployed: u64,
    total_vaulted: u64,
    total_winnings: u64,
    total_minted: u64,
    ts: u64,
}

// Each item: [ [u8;64], { ...ResetRoundMeta... } ]
type ResetRoundEntry = (Vec<u8>, ResetRoundMeta);

fn top_miner_to_string(bytes: [u8; 32]) -> String {
    let pk = Pubkey::new_from_array(bytes);
    pk.to_string()
}

pub async fn get_ore_supply_rounds(page: u64) -> Vec<AppRound> {
    let url = format!("https://ore-bsm.onrender.com/events/reset?page={}", page);

    let client = Client::new();

    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("get_ore_supply_rounds: HTTP request failed: {e}");
            return Vec::new();
        }
    };

    if !resp.status().is_success() {
        eprintln!(
            "get_ore_supply_rounds: non-success status {} for URL {}",
            resp.status(),
            url
        );
        return Vec::new();
    }

    let entries: Vec<ResetRoundEntry> = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("get_ore_supply_rounds: failed to parse JSON: {e}");
            return Vec::new();
        }
    };

    entries
        .into_iter()
        .map(|(_sig_bytes, meta)| AppRound {
            round_id: meta.round_id as i64,
            winning_square: meta.winning_square as i64,
            motherlode: meta.motherlode as i64,
            top_miner: top_miner_to_string(meta.top_miner),
            total_deployed: meta.total_deployed as i64,
            total_vaulted: meta.total_vaulted as i64,
            total_winnings: meta.total_winnings as i64,
            created_at: meta.ts as i64,
        })
        .collect()
}
