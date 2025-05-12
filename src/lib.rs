use std::error::Error;
use std::time::Duration;
use async_trait::async_trait;
use reqwest::{Client, ClientBuilder};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::warn;

// Default timeout in seconds if none is specified
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Maximum payload accepted by the Syscoin PoDA endpoint (2 MiB).
pub const MAX_BLOB_SIZE: usize = 2 * 1024 * 1024;

/// Response structure for JSON-RPC calls
#[derive(Deserialize, Debug)]
struct JsonRpcResponse<T> {
    result: Option<T>,
    error: Option<Value>,
}

/// Common trait for RPC clients to enable easy mocking
#[async_trait]
pub trait RpcClient {
    /// Make a generic RPC call with any method and parameters
    async fn call(&self, method: &str, params: &[Value]) -> Result<Value, Box<dyn Error>>;

    /// Get wallet balance with optional account and watchonly parameters
    async fn get_balance(&self, account: Option<&str>, include_watchonly: Option<bool>) -> Result<f64, Box<dyn Error>>;

    /// Make an HTTP GET request to the specified URL
    async fn http_get(&self, url: &str) -> Result<Vec<u8>, Box<dyn Error>>;
}

/// Production implementation of the RPC client for Syscoin
pub struct RealRpcClient {
    rpc_url: String,
    rpc_user: String,
    rpc_password: String,
    http_client: Client,
    timeout: Duration,
}

impl RealRpcClient {
    /// Create a new RPC client with default timeout
    pub fn new(rpc_url: &str, rpc_user: &str, rpc_password: &str, timeout: Option<Duration>) -> Result<Self, Box<dyn Error>> {
        Self::new_with_timeout(rpc_url, rpc_user, rpc_password, timeout)
    }

    /// Create a new RPC client with custom timeout
    pub fn new_with_timeout(
        rpc_url: &str,
        rpc_user: &str,
        rpc_password: &str,
        timeout: Option<Duration>,
    ) -> Result<Self, Box<dyn Error>> {
        let timeout = timeout.unwrap_or_else(|| Duration::from_secs(DEFAULT_TIMEOUT_SECS));

        let http_client = ClientBuilder::new()
            .timeout(timeout)
            .build()?;

        Ok(Self {
            rpc_url: rpc_url.to_string(),
            rpc_user: rpc_user.to_string(),
            rpc_password: rpc_password.to_string(),
            http_client,
            timeout,
        })
    }

    /// Send a JSON-RPC request to the Syscoin node
    async fn rpc_request(&self, method: &str, params: &[Value]) -> Result<Value, Box<dyn Error>> {
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        let response = self.http_client
            .post(&self.rpc_url)
            .basic_auth(&self.rpc_user, Some(&self.rpc_password))
            .json(&request_body)
            .timeout(self.timeout)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("HTTP error: {}", response.status()).into());
        }

        let response_body: JsonRpcResponse<Value> = response.json().await?;

        match (response_body.result, response_body.error) {
            (Some(result), _) => Ok(result),
            (_, Some(error)) => Err(format!("RPC error: {}", error).into()),
            _ => Err("Invalid RPC response format".into()),
        }
    }

    /// Create or load a wallet by name
    pub async fn create_or_load_wallet(&self, wallet_name: &str) -> Result<(), Box<dyn Error>> {
        // First try to load the wallet
        match self.call("loadwallet", &[json!(wallet_name)]).await {
            Ok(_) => return Ok(()),
            Err(_) => {
                // If loading fails, try to create a new wallet
                self.call("createwallet", &[json!(wallet_name)]).await?;
                Ok(())
            }
        }
    }
}

#[async_trait]
impl RpcClient for RealRpcClient {
    async fn call(&self, method: &str, params: &[Value]) -> Result<Value, Box<dyn Error>> {
        self.rpc_request(method, params).await
    }

    async fn get_balance(&self, account: Option<&str>, include_watchonly: Option<bool>) -> Result<f64, Box<dyn Error>> {
        let mut params = Vec::new();

        if let Some(acct) = account {
            params.push(json!(acct));

            if let Some(watch) = include_watchonly {
                params.push(json!(watch));
            }
        }

        let result = self.call("getbalance", &params).await?;
        let balance = result.as_f64().ok_or("Invalid balance format")?;

        Ok(balance)
    }

    async fn http_get(&self, url: &str) -> Result<Vec<u8>, Box<dyn Error>> {
        let response = self.http_client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(format!("HTTP GET error: {}", response.status()).into());
        }

        Ok(response.bytes().await?.to_vec())
    }
}

pub struct SyscoinClient {
    rpc_client: RealRpcClient,
    poda_url: String,
}

impl SyscoinClient {
    /// Create a new Syscoin client
    pub fn new(
        rpc_url: &str,
        rpc_user: &str,
        rpc_password: &str,
        poda_url: &str,
        timeout: Option<Duration>,
    ) -> Result<Self, Box<dyn Error>> {
        let rpc_client = RealRpcClient::new_with_timeout(rpc_url, rpc_user, rpc_password, timeout)?;

        Ok(Self {
            rpc_client,
            poda_url: poda_url.to_string(),
        })
    }

    /// Create a blob in BitcoinDA(FKA Poda) storage
    pub async fn create_blob(&self, data: &[u8]) -> Result<String, Box<dyn Error>> {
        if data.len() > MAX_BLOB_SIZE {
            return Err(format!(
                "blob size ({}) exceeds maximum allowed ({})",
                data.len(),
                MAX_BLOB_SIZE
            ).into());
        }

        // Use named parameter format as required by Syscoin
        let data_hex = hex::encode(data);
        let params = vec![json!({ "data": data_hex })];
        
        let response = self.rpc_client.call("syscoincreatenevmblob", &params).await?;
        let hash = response
            .get("versionhash")
            .and_then(|v| v.as_str())
            .ok_or("Missing versionhash")?;

        Ok(hash.to_string())
    }

    /// Get wallet balance
    pub async fn get_balance(&self) -> Result<f64, Box<dyn Error>> {
        self.rpc_client.get_balance(None, None).await
    }

    /// Fetch a blob; tries RPC first, then falls back to PoDA cloud
    pub async fn get_blob(&self, blob_id: &str) -> Result<Vec<u8>, Box<dyn Error>> {
        match self.get_blob_from_rpc(blob_id).await {
            Ok(data) => Ok(data),
            Err(e) => {
                warn!("get_blob_from_rpc failed ({e}); falling back to cloud");
                self.get_blob_from_cloud(blob_id).await
            }
        }
    }

    /// Retrieve blob data from RPC node
    async fn get_blob_from_rpc(&self, blob_id: &str) -> Result<Vec<u8>, Box<dyn Error>> {
        // Strip any 0x prefix
        let actual_blob_id = if let Some(stripped) = blob_id.strip_prefix("0x") {
            stripped
        } else {
            blob_id
        };

        // Use named parameters as required
        let params = vec![json!({
            "versionhash_or_txid": actual_blob_id,
            "getdata": true
        })];

        let response = self.rpc_client.call("getnevmblobdata", &params).await?;
        
        let hex_data = response
            .get("data")
            .and_then(|v| v.as_str())
            .ok_or("Missing data in getnevmblobdata response")?;

        // Strip any 0x prefix from result data
        let data_to_decode = if let Some(stripped) = hex_data.strip_prefix("0x") {
            stripped
        } else {
            hex_data
        };

        Ok(hex::decode(data_to_decode)?)
    }

    /// Retrieve blob data from PODA cloud storage
    pub async fn get_blob_from_cloud(&self, version_hash: &str) -> Result<Vec<u8>, Box<dyn Error>> {
        let url = format!("{}/blob/{}", self.poda_url, version_hash);
        self.rpc_client.http_get(&url).await
    }

    /// Create or load a wallet by name
    pub async fn create_or_load_wallet(&self, wallet_name: &str) -> Result<(), Box<dyn Error>> {
        self.rpc_client.create_or_load_wallet(wallet_name).await
    }
}

/// Mock implementation for testing
#[cfg(test)]
pub struct MockRpcClient {
    // Add any fields needed for test state
}

#[cfg(test)]
#[async_trait]
impl RpcClient for MockRpcClient {
    async fn call(&self, method: &str, _params: &[Value]) -> Result<Value, Box<dyn Error>> {
        // Return mock responses based on the method
        match method {
            "getbalance" => Ok(json!(10.5)),
            "syscoincreatenevmblob" => Ok(json!({ "versionhash": "mock_blob_hash" })),
            "getnevmblobdata" => Ok(json!({ "data": hex::encode(b"mock_data") })),
            "loadwallet" => Ok(json!(null)),
            "createwallet" => Ok(json!(null)),
            _ => Err("Unimplemented mock method".into()),
        }
    }

    async fn get_balance(&self, _account: Option<&str>, _include_watchonly: Option<bool>) -> Result<f64, Box<dyn Error>> {
        Ok(10.5)
    }

    async fn http_get(&self, _url: &str) -> Result<Vec<u8>, Box<dyn Error>> {
        Ok(b"mock_data".to_vec())
    }
}