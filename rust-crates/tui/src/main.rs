use color_eyre::eyre::{
    Result,
    eyre,
};
use strapped_contract::deployment;

mod client;
mod indexer_client;
mod ui;
mod wallets;

fn print_usage_and_exit() -> ! {
    println!(
        "Usage: strapped-contract [--fake-vrf] [--devnet | --testnet | --local] [--rpc-url <url>]\n\
         [--wallet <name>] [--wallet-dir <path>]\n\
         [--indexer-url <url>]\n\
         \n\
         Flags:\n\
           --fake-vrf          Use the fake VRF contract instead of pseudo VRF\n\
           --devnet            Connect to Fuel devnet (default RPC {})\n\
           --testnet           Connect to Fuel testnet (default RPC {})\n\
           --local             Connect to a local Fuel node (default RPC {})\n\
           --rpc-url <url>     Override the RPC URL for the selected network\n\
           --wallet <name>     forc-wallet profile to use for playing\n\
           --wallet-dir <path> Override forc-wallet directory (defaults to ~/.fuel/wallets)\n\
           --indexer-url <url> Point the client at a running indexer HTTP endpoint",
        client::DEFAULT_DEVNET_RPC_URL,
        client::DEFAULT_TESTNET_RPC_URL,
        client::DEFAULT_LOCAL_RPC_URL,
    );
    std::process::exit(0);
}

fn parse_cli_args() -> Result<client::AppConfig> {
    #[derive(Clone, Copy)]
    enum NetworkFlag {
        Devnet,
        Testnet,
        Local,
    }

    let mut args = std::env::args().skip(1);
    let mut vrf_mode = client::VrfMode::Pseudo;
    let mut network_flag: Option<NetworkFlag> = None;
    let mut custom_url: Option<String> = None;
    let mut wallet_dir: Option<String> = None;
    let mut wallet_name: Option<String> = None;
    let mut indexer_url: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fake-vrf" => vrf_mode = client::VrfMode::Fake,
            "--devnet" => {
                if network_flag.is_some() {
                    return Err(eyre!(
                        "Multiple network flags provided; choose one of --devnet/--testnet/--local"
                    ));
                }
                network_flag = Some(NetworkFlag::Devnet);
            }
            "--testnet" => {
                if network_flag.is_some() {
                    return Err(eyre!(
                        "Multiple network flags provided; choose one of --devnet/--testnet/--local"
                    ));
                }
                network_flag = Some(NetworkFlag::Testnet);
            }
            "--local" => {
                if network_flag.is_some() {
                    return Err(eyre!(
                        "Multiple network flags provided; choose one of --devnet/--testnet/--local"
                    ));
                }
                network_flag = Some(NetworkFlag::Local);
            }
            "--rpc-url" => {
                let url = args
                    .next()
                    .ok_or_else(|| eyre!("--rpc-url requires a URL argument"))?;
                if custom_url.is_some() {
                    return Err(eyre!("--rpc-url may only be specified once"));
                }
                if network_flag.is_none() {
                    return Err(eyre!(
                        "--rpc-url must follow a network flag (--devnet/--testnet/--local)"
                    ));
                }
                custom_url = Some(url);
            }
            "--wallet-dir" => {
                let dir = args
                    .next()
                    .ok_or_else(|| eyre!("--wallet-dir requires a path argument"))?;
                if wallet_dir.is_some() {
                    return Err(eyre!("--wallet-dir may only be specified once"));
                }
                wallet_dir = Some(dir);
            }
            "--wallet" => {
                let name = args
                    .next()
                    .ok_or_else(|| eyre!("--wallet requires a wallet name"))?;
                if wallet_name.is_some() {
                    return Err(eyre!("--wallet may only be specified once"));
                }
                wallet_name = Some(name);
            }
            "--indexer-url" => {
                let url = args
                    .next()
                    .ok_or_else(|| eyre!("--indexer-url requires a URL argument"))?;
                if indexer_url.is_some() {
                    return Err(eyre!("--indexer-url may only be specified once"));
                }
                indexer_url = Some(url);
            }
            "--help" | "-h" => print_usage_and_exit(),
            other => return Err(eyre!("Unknown argument: {other}")),
        }
    }

    let network = match network_flag {
        None => {
            return Err(eyre!(
                "Select a network with --devnet, --testnet, or --local"
            ));
        }
        Some(NetworkFlag::Devnet) => client::NetworkTarget::Devnet {
            url: custom_url.unwrap_or_else(|| client::DEFAULT_DEVNET_RPC_URL.to_string()),
        },
        Some(NetworkFlag::Testnet) => client::NetworkTarget::Testnet {
            url: custom_url
                .unwrap_or_else(|| client::DEFAULT_TESTNET_RPC_URL.to_string()),
        },
        Some(NetworkFlag::Local) => client::NetworkTarget::LocalNode {
            url: custom_url.unwrap_or_else(|| client::DEFAULT_LOCAL_RPC_URL.to_string()),
        },
    };

    let wallet = wallet_name.ok_or_else(|| {
        eyre!("Specify --wallet <name> to select a forc-wallet profile")
    })?;
    let dir = wallets::resolve_wallet_dir(wallet_dir.as_deref())?;
    let wallets = client::WalletConfig::ForcKeystore {
        owner: wallet.clone(),
        dir,
    };

    Ok(client::AppConfig {
        vrf_mode,
        network,
        wallets,
        indexer_url,
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    // let _ = tracing_subscriber::fmt()
    //     .with_max_level(tracing::Level::INFO)
    //     .try_init();
    tracing::info!("starting strapped-contract client");
    color_eyre::install()?;
    deployment::ensure_structure().map_err(|e| eyre!(e))?;
    let app_config = parse_cli_args()?;
    client::run_app(app_config).await
}
