use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "evore-bot")]
#[command(about = "Evore deployment bot for ORE v3")]
struct Args {
    /// RPC URL
    #[arg(long, default_value = "https://api.mainnet-beta.solana.com")]
    rpc_url: String,

    /// Path to keypair file
    #[arg(long)]
    keypair: Option<String>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    
    println!("Evore Bot");
    println!("RPC: {}", args.rpc_url);
    
    // TODO: Implement bot logic
}

