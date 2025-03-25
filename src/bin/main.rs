use bitcoin_da_client::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a real RPC client
    println!("Starting")  ;
    let rpc_url = "http://127.0.0.1:8370";
    let rpc_user = "u";
    let rpc_password = "p";
    let poda_url = "http://poda.tanenbaum.io/vh/";
    let timeout = Some(std::time::Duration::from_secs(30));


    let rpc_client = RealRpcClient::new(rpc_url, rpc_user, rpc_password, timeout)?;
    println!("Loading wallet") ;
    rpc_client.create_or_load_wallet("wallet12").await?;


    println!("Initializing client") ;
    let syscoin_client = SyscoinClient::new(
        "http://127.0.0.1:8370/wallet/wallet12",
        "u",
        "p",
        poda_url,
        timeout,
    )?;

    println!("checking balance");
    let balance = syscoin_client.get_balance().await?;

    println!("Balance: {}", balance);

    println!("Sending blob data");
    let blob_hash = syscoin_client.create_blob(&[1, 2, 3, 4]).await?;
    println!("Created Blob Hash: {}", blob_hash);

    let blob_data = syscoin_client.get_blob_from_cloud(&blob_hash).await?;

    // TODO: script to get blob from rpc?
    // TODO: test for resilioence, how long can it be running?
    println!("Blob Data: {:?}", blob_data);

    Ok(())
}