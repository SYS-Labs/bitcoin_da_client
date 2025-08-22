use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, debug, span, Level};
use tracing::{Instrument};
use tracing_subscriber::fmt;
use bitcoin_da_client::SyscoinClient;

type Error = Box<dyn std::error::Error + Send + Sync>;

#[tokio::main]
async fn main() -> Result<(), Error> {
    // 🎛️ Initialize tracing: compact output, max DEBUG, no file/line info
    fmt()
        .with_max_level(Level::INFO)
        .with_file(false)
        .with_line_number(false)
        .with_target(false)
        .compact()
        .init();

    info!("🚀 Starting Syscoin client application");

    // 🔧 Configuration parameters
    let rpc_url    = "http://127.0.0.1:8370";
    let rpc_user   = "u";
    let rpc_password = "p";
    let poda_url   = "https://poda.syscoin.org/vh/";
    let timeout    = Some(Duration::from_secs(30));
    let wallet     = "wallet200999";
    debug!(rpc_url, rpc_user, poda_url, timeout = ?timeout, wallet, "🔍 Config loaded");

    // 🔌 Initialize the Syscoin RPC client
    info!("🔌 Connecting to Syscoin node…");
    let client = SyscoinClient::new(
        rpc_url,
        rpc_user,
        rpc_password,
        poda_url,
        timeout,
        wallet,
    )?;
    info!("✅ SyscoinClient initialized successfully");

    // 💼 Create or load the wallet and ensure a stable funding address
    info!("🆕 Loading or creating wallet “{}”", wallet);
    client
        .create_or_load_wallet(wallet)
        .instrument(span!(Level::DEBUG, "create_or_load_wallet", wallet = wallet))
        .await?;
    let funding_label = "da_funding";
    let funding_address = client
        .ensure_address_by_label(funding_label)
        .instrument(span!(Level::DEBUG, "ensure_address_by_label", label = funding_label))
        .await?;
    info!("🏷️ Funding label '{}' is bound to address: {}", funding_label, funding_address);

    // 📥 Fetch the current balance
    let mut balance = client
        .get_balance()
        .instrument(span!(Level::INFO, "get_balance_start"))
        .await?;
    debug!("📥 Balance fetched: {} SYS", balance);

    // 💸 Funding flow if balance is zero
    if balance <= 0.0 {
        info!("⚠️ Balance empty, let's top you up!");
        let address = match client
            .fetch_address_by_label("podalabel")
            .instrument(span!(Level::DEBUG, "fetch_address_by_label", label = "podalabel"))
            .await?
        {
            Some(addr) => {
                info!("📍 Found existing funding address: {}", addr);
                addr
            }
            None => {
                info!("✨ No address yet—creating a fresh one…");
                let addr = client
                    .get_new_address("podalabel")
                    .instrument(span!(Level::DEBUG, "get_new_address", label = "podalabel"))
                    .await?;
                info!("📍 New funding address: {}", addr);
                addr
            }
        };

        info!("💌 Please send some SYS to: {}", address);

        // 🔄 Poll until funds arrive
        while balance <= 0.0 {
            debug!("⏳ Waiting 10 seconds before checking balance again…");
            sleep(Duration::from_secs(10)).await;
            balance = client.get_balance().await?;
            info!("🔄 Checking… current balance: {} SYS", balance);
        }
        info!("🎉 Funds detected! Continuing…");
    }

    // 📤 Blob upload/retrieval flow
    let data_to_upload = [1, 2, 3, 4];
    info!("📤 Uploading blob data: {:?}", data_to_upload);
    let blob_hash = client
        .create_blob(&data_to_upload)
        .instrument(span!(Level::DEBUG, "create_blob", data = ?data_to_upload))
        .await?;
    info!("✅ Blob uploaded! Got hash: {}", blob_hash);

    // ✅ Check finality (chainlock) once
    let is_final = client
        .check_blob_finality(&blob_hash)
        .instrument(span!(Level::INFO, "check_blob_finality", hash = %blob_hash))
        .await?;
    if is_final {
        info!("🔒 Blob is FINAL (chainlocked)");
    } else {
        info!("⌛ Blob not final yet (no chainlock)");
    }

    info!("📥 Fetching blob back by hash…");
    let blob_data = client
        .get_blob(&blob_hash)
        .instrument(span!(Level::DEBUG, "get_blob", hash = %blob_hash))
        .await?;
    info!("🗒️ Blob data retrieved: {:?}", blob_data);

    // 🔗 Log the data availability (DA) link
    let da_link = format!("{}{}", poda_url, blob_hash);
    info!("🔗 Access your data here: {}", da_link);


    info!("🏁 Syscoin client flow complete—have a great day!");
    Ok(())
}
