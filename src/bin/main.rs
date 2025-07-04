use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, debug, span, Level};
use tracing::{Instrument};
use tracing_subscriber::fmt;
use bitcoin_da_client::SyscoinClient;

type Error = Box<dyn std::error::Error + Send + Sync>;

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Initialize tracing: compact output without file/line info, max DEBUG level
    fmt()
        .with_max_level(Level::DEBUG)
        .with_file(false)
        .with_line_number(false)
        .with_target(false)
        .compact()
        .init();

    info!("Starting Syscoin client application");

    // Configuration parameters
    let rpc_url = "http://127.0.0.1:8370";
    let rpc_user = "u";
    let rpc_password = "p";
    let poda_url = "https://poda.syscoin.org/vh/";
    let timeout = Some(Duration::from_secs(30));
    let wallet = "wallet200999";
    debug!(rpc_url, rpc_user, poda_url, timeout = ?timeout, wallet, "Config loaded");

    // Initialize the Syscoin RPC client
    let client = SyscoinClient::new(
        rpc_url,
        rpc_user,
        rpc_password,
        poda_url,
        timeout,
        wallet,
    )?;
    info!("SyscoinClient initialized successfully");

    // Create or load the wallet
    info!(wallet, "Loading or creating wallet");
    client
        .create_or_load_wallet(wallet)
        .instrument(span!(Level::DEBUG, "create_or_load_wallet", wallet = wallet))
        .await?;

    // Fetch the current balance
    let mut balance = client
        .get_balance()
        .instrument(span!(Level::DEBUG, "get_balance_start"))
        .await?;
    debug!(balance, "Balance fetched");

    // Funding flow if balance is zero
    if balance <= 0.0 {
        info!("Balance empty, initiating funding flow");
        let address = match client
            .fetch_address_by_label("podalabel")
            .instrument(span!(Level::DEBUG, "fetch_address_by_label", label = "podalabel"))
            .await?
        {
            Some(addr) => {
                info!(address = %addr, "Found existing funding address");
                addr
            }
            None => {
                info!("No existing address found, creating new one");
                let addr = client
                    .get_new_address("podalabel")
                    .instrument(span!(Level::DEBUG, "get_new_address", label = "podalabel"))
                    .await?;
                info!(address = %addr, "Created new funding address");
                addr
            }
        };

        info!(address = %address, "Please fund your wallet with SYS at this address");

        // Poll until funds arrive
        while balance <= 0.0 {
            debug!("Sleeping for 10 seconds before next balance check");
            sleep(Duration::from_secs(10)).await;
            balance = client.get_balance().await?;
            info!(address = %address, balance, "Current balance at address");
        }
        info!("Funding detected, proceeding...");
    }

    // Blob upload/retrieval flow
    info!("Uploading blob data [1,2,3,4]");
    let blob_hash = client
        .create_blob(&[1, 2, 3, 4])
        .instrument(span!(Level::DEBUG, "create_blob", data = "[1,2,3,4]"))
        .await?;
    info!(hash = %blob_hash, "Blob created successfully");

    info!(hash = %blob_hash, "Fetching blob data back");
    let blob_data = client
        .get_blob(&blob_hash)
        .instrument(span!(Level::DEBUG, "get_blob", hash = %blob_hash))
        .await?;
    info!(data = ?blob_data, "Blob data retrieved");

    info!("Syscoin client flow complete");
    Ok(())
}
