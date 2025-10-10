use color_eyre::eyre::{
    Result,
    eyre,
};

mod client;
mod deployment;
mod ui;
mod wallets;

fn print_usage_and_exit() -> ! {
    println!(
        "Usage: strapped-contract [--fake-vrf] [--devnet | --testnet | --local] [--rpc-url <url>]\n\
         [--wallet <name> | --wallet-owner <name> [--wallet-player <name>]] [--wallet-dir <path>] [--deploy]\n\
         \n\
         Flags:\n\
           --fake-vrf          Use the fake VRF contract instead of pseudo VRF\n\
           --devnet            Connect to Fuel devnet (default RPC {})\n\
           --testnet           Connect to Fuel testnet (default RPC {})\n\
           --local             Connect to a local Fuel node (default RPC {})\n\
           --rpc-url <url>     Override the RPC URL for the selected network\n\
           --wallet <name>     Use the same forc-wallet for both owner and player roles\n\
           --wallet-owner <name>  Specify owner wallet name (for remote networks)\n\
           --wallet-player <name> Specify player wallet name (defaults to owner wallet)\n\
           --wallet-dir <path> Override forc-wallet directory (defaults to ~/.fuel/wallets)\n\
           --deploy            Deploy a fresh contract if no compatible deployment exists",
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
    let mut wallet_owner: Option<String> = None;
    let mut wallet_player: Option<String> = None;
    let mut wallet_shared: Option<String> = None;
    let mut deploy_if_missing = false;

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
                if wallet_shared.is_some() {
                    return Err(eyre!("--wallet may only be specified once"));
                }
                wallet_shared = Some(name);
            }
            "--wallet-owner" => {
                let name = args
                    .next()
                    .ok_or_else(|| eyre!("--wallet-owner requires a wallet name"))?;
                if wallet_owner.is_some() {
                    return Err(eyre!("--wallet-owner may only be specified once"));
                }
                wallet_owner = Some(name);
            }
            "--wallet-player" => {
                let name = args
                    .next()
                    .ok_or_else(|| eyre!("--wallet-player requires a wallet name"))?;
                if wallet_player.is_some() {
                    return Err(eyre!("--wallet-player may only be specified once"));
                }
                wallet_player = Some(name);
            }
            "--deploy" => {
                deploy_if_missing = true;
            }
            "--help" | "-h" => print_usage_and_exit(),
            other => return Err(eyre!("Unknown argument: {other}")),
        }
    }

    let network = match network_flag {
        None => client::NetworkTarget::InMemory,
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

    let wallet_flags_present = wallet_dir.is_some()
        || wallet_owner.is_some()
        || wallet_player.is_some()
        || wallet_shared.is_some();

    let wallets = match network_flag {
        None => {
            if wallet_flags_present {
                return Err(eyre!(
                    "Wallet selection flags require a network flag (--devnet/--testnet/--local)"
                ));
            }
            client::WalletConfig::Generated
        }
        Some(_) => {
            let dir = wallets::resolve_wallet_dir(wallet_dir.as_deref())?;
            let owner_name = wallet_owner
                .or_else(|| wallet_shared.clone())
                .ok_or_else(|| {
                    eyre!(
                        "Specify --wallet-owner <name> or --wallet <name> when selecting a remote network"
                    )
                })?;
            let player_name = wallet_player
                .or_else(|| wallet_shared.clone())
                .unwrap_or_else(|| owner_name.clone());

            client::WalletConfig::ForcKeystore {
                owner: owner_name,
                player: player_name,
                dir,
            }
        }
    };

    Ok(client::AppConfig {
        vrf_mode,
        network,
        wallets,
        deploy_if_missing,
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();
    tracing::info!("starting strapped-contract client");
    color_eyre::install()?;
    deployment::ensure_structure()?;
    let app_config = parse_cli_args()?;
    client::run_app(app_config).await
}
