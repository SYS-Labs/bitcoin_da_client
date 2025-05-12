use bitcoin_da_client::SyscoinClient;

type Error = Box<dyn std::error::Error + Send + Sync>;

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Create a real RPC client
    println!("Starting");
    let rpc_url = "http://127.0.0.1:8370";
    let rpc_user = "u";
    let rpc_password = "p";
    let poda_url = "http://poda.tanenbaum.io/vh/";
    let timeout = Some(std::time::Duration::from_secs(30));

    // Initialize the client
    let client = SyscoinClient::new(
        rpc_url,
        rpc_user,
        rpc_password,
        poda_url,
        timeout,
    )?;
    
    // Create/load wallet
    println!("Loading wallet");
    client.create_or_load_wallet("wallet12").await?;

    println!("Checking balance");
    let balance = client.get_balance().await?;
    println!("Balance: {balance}");

    println!("Uploading blob data");
    let blob_hash = client.create_blob(&[1, 2, 3, 4]).await?;
    println!("Created blob: {blob_hash}");

    println!("Fetching it back (RPC → cloud fallback)");
    let blob_data = client.get_blob(&blob_hash).await?;
    println!("Blob data: {blob_data:?}");

    Ok(())
}