use std::error::Error;
use std::time::Duration;
use async_trait::async_trait;
use reqwest::{Client, ClientBuilder};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::{info, warn};

// Default timeout in seconds if none is specified
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Maximum payload accepted by the Syscoin PoDA endpoint (2 MiB).
pub const MAX_BLOB_SIZE: usize = 2 * 1024 * 1024;

/// Thread-safe error type
pub type SyscoinError = Box<dyn Error + Send + Sync + 'static>;

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
    async fn call(&self, method: &str, params: &[Value]) -> Result<Value, SyscoinError>;

    async fn call_wallet(&self, method: &str, params: &[Value]) -> Result<Value, SyscoinError>;

    /// Get wallet balance with optional account and watchonly parameters
    async fn get_balance(&self, account: Option<&str>, include_watchonly: Option<bool>) -> Result<f64, SyscoinError>;

    /// Make an HTTP GET request to the specified URL
    async fn http_get(&self, url: &str) -> Result<Vec<u8>, SyscoinError>;
}

/// Production implementation of the RPC client for Syscoin
pub struct RealRpcClient {
    rpc_url: String,
    rpc_user: String,
    rpc_password: String,
    http_client: Client,
    timeout: Duration,
    wallet_name: String,
}

impl RealRpcClient {
    /// Create a new RPC client with default timeout
    pub fn new(rpc_url: &str, rpc_user: &str, rpc_password: &str, timeout: Option<Duration>, wallet_name: &str) -> Result<Self, SyscoinError> {
        Self::new_with_timeout(rpc_url, rpc_user, rpc_password, timeout, wallet_name)
    }

    /// Create a new RPC client with custom timeout
    pub fn new_with_timeout(
        rpc_url: &str,
        rpc_user: &str,
        rpc_password: &str,
        timeout: Option<Duration>,
        wallet_name: &str,
    ) -> Result<Self, SyscoinError> {
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
            wallet_name: wallet_name.to_string(),
        })
    }

    /// Send a JSON-RPC request to the Syscoin node
    async fn rpc_request(&self, method: &str, params: &[Value]) -> Result<Value, SyscoinError> {
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        // fire the HTTP call
        let resp = self.http_client
            .post(&self.rpc_url)
            .basic_auth(&self.rpc_user, Some(&self.rpc_password))
            .json(&request_body)
            .timeout(self.timeout)
            .send()
            .await?;

        // pull the entire body into a String
        let status = resp.status();
        let body   = resp.text().await?;

        // log whatever the node actually sent us
        info!("RPC `{}` → HTTP {}:\n{}", method, status, body);

        // if it wasn’t a 200, include the body in our Err
        if !status.is_success() {
            return Err(format!(
                "HTTP error: {} returned body: {}",
                status, body
            ).into());
        }

        // now parse the JSON-RPC envelope from the text
        let jr: JsonRpcResponse<Value> = serde_json::from_str(&body)?;
        if let Some(err) = jr.error {
            // you can pull out err["code"] and err["message"] here too
            return Err(format!("RPC error: {}", err).into());
        }

        jr.result.ok_or_else(|| "missing result in JSON-RPC response".into())
    }

    /// Like `rpc_request`, but points at `/wallet/{wallet_name}` on the node
    async fn wallet_rpc_request(&self, method: &str, params: &[Value]) -> Result<Value, SyscoinError> {
        // build the JSON-RPC envelope
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        // compute the wallet-specific URL
        let base = self.rpc_url.trim_end_matches('/');
        let url  = format!("{}/wallet/{}", base, self.wallet_name);

        // fire the HTTP call
        let resp   = self.http_client
            .post(&url)
            .basic_auth(&self.rpc_user, Some(&self.rpc_password))
            .json(&request_body)
            .timeout(self.timeout)
            .send()
            .await?;

        // pull the entire body into a String
        let status = resp.status();
        let body   = resp.text().await?;

        // log whatever the node actually sent us
        info!("WALLET RPC `{}` → HTTP {}:\n{}", method, status, body);

        // if it wasn’t a 200, include the body in our Err
        if !status.is_success() {
            return Err(format!(
                "HTTP error: {} returned body: {}",
                status, body
            ).into());
        }

        // now parse the JSON-RPC envelope
        let jr: JsonRpcResponse<Value> = serde_json::from_str(&body)?;

        // if the RPC server reported an application-level error, forward it
        if let Some(err) = jr.error {
            return Err(format!("RPC error: {}", err).into());
        }

        // otherwise grab the result or error out if missing
        jr.result.ok_or_else(|| "missing result in JSON-RPC response".into())
    }


    /// Create or load a wallet by name
    pub async fn create_or_load_wallet(&self, wallet_name: &str) -> Result<(), SyscoinError> {
        info!("create_or_load_wallet");
        match self.call("loadwallet", &[json!(wallet_name)]).await {
            Ok(_) => return Ok(()),
            Err(e) => {
                info!("wallet error");
                let s = e.to_string();
                info!(s);
                // -18 = wallet not found → create it
                if s.contains("failed") {
                    info!("wallet not found, creating new one");
                    self.call("createwallet", &[json!(wallet_name)]).await?;
                    return Ok(());
                }
                // -4 = wallet already loaded → ignore
                if s.contains("already loaded") {
                    info!("wallet already loaded, continuing");
                    return Ok(());
                }
                // any other error is fatal
                return Err(e);
            }
        }
    }


}

#[async_trait]
impl RpcClient for RealRpcClient {
    async fn call(&self, method: &str, params: &[Value]) -> Result<Value, SyscoinError> {
        self.rpc_request(method, params).await
    }

    async fn call_wallet(&self, method: &str, params: &[Value]) -> Result<Value, SyscoinError> {
        self.wallet_rpc_request(method, params).await
    }

    async fn get_balance(&self, account: Option<&str>, include_watchonly: Option<bool>) -> Result<f64, SyscoinError> {
        let mut params = Vec::new();
        if let Some(acct) = account {
            params.push(json!(acct));
            if let Some(w) = include_watchonly {
                params.push(json!(w));
            }
        }
        let v = self.wallet_rpc_request("getbalance", &params).await?;
        v.as_f64().ok_or_else(|| "Invalid balance format".into())
    }

    async fn http_get(&self, url: &str) -> Result<Vec<u8>, SyscoinError> {
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
        wallet_name: &str,
    ) -> Result<Self, SyscoinError> {
        info!("Initializing Client");
        let rpc_client = RealRpcClient::new_with_timeout(rpc_url, rpc_user, rpc_password, timeout, wallet_name)?;

        Ok(Self {
            rpc_client,
            poda_url: poda_url.to_string(),
        })
    }

    /// Create a blob in BitcoinDA(FKA Poda) storage
    pub async fn create_blob(&self, data: &[u8]) -> Result<String, SyscoinError> {
        if data.len() > MAX_BLOB_SIZE {
            return Err(format!(
                "blob size ({}) exceeds maximum allowed ({})",
                data.len(),
                MAX_BLOB_SIZE
            ).into());
        }

        let data_hex = hex::encode(data);
        // pass hex string as the first positional param
        let params = vec![ json!(data_hex) ];

        let response = self.rpc_client.call_wallet("syscoincreatenevmblob", &params).await?;
        let hash = response
            .get("versionhash")
            .and_then(|v| v.as_str())
            .ok_or("Missing versionhash")?;
        Ok(hash.to_string())
    }


    /// Get wallet balance
    pub async fn get_balance(&self) -> Result<f64, SyscoinError> {
        self.rpc_client.get_balance(None, None).await
    }

    /// Fetch a blob; tries RPC first, then falls back to PoDA cloud
    pub async fn get_blob(&self, blob_id: &str) -> Result<Vec<u8>, SyscoinError> {
        match self.get_blob_from_rpc(blob_id).await {
            Ok(data) => Ok(data),
            Err(e) => {
                warn!("get_blob_from_rpc failed ({e}); falling back to cloud");
                self.get_blob_from_cloud(blob_id).await
            }
        }
    }

    /// Get a fresh address for a given label
    pub async fn get_new_address(&self, address_label: &str) -> Result<String, SyscoinError> {
        let resp = self
            .rpc_client
            .call_wallet("getnewaddress", &[json!(address_label)])
            .await?;
        resp.as_str()
            .map(|s| s.to_owned())
            .ok_or_else(|| "getnewaddress returned non-string".into())
    }


    /// Fetch an existing address by label, if any
    pub async fn fetch_address_by_label(
        &self,
        address_label: &str,
    ) -> Result<Option<String>, SyscoinError> {
        // — pass the label as a bare string —
        let resp = match self
            .rpc_client
            .call_wallet("getaddressesbylabel", &[json!(address_label)])
            .await
        {
            Ok(v) => v,
            Err(e) => {
                let msg = e.to_string();
                // if it's the "no addresses" error, swallow it as None
                if msg.contains("\"code\":-11") {
                    return Ok(None);
                }
                // otherwise re-propagate
                return Err(e);
            }
        };

        // parse returned map, take the first key if any
        if let Some(map) = resp.as_object() {
            if let Some((addr, _)) = map.iter().next() {
                return Ok(Some(addr.clone()));
            }
        }
        Ok(None)
    }



    /// Retrieve blob data from RPC node
    async fn get_blob_from_rpc(&self, blob_id: &str) -> Result<Vec<u8>, SyscoinError> {
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
    pub async fn get_blob_from_cloud(&self, version_hash: &str) -> Result<Vec<u8>, SyscoinError> {
        let url = format!("{}/blob/{}", self.poda_url, version_hash);
        self.rpc_client.http_get(&url).await
    }

    /// Check if a blob is final
    pub async fn check_blob_finality(&self, blob_id: &str) -> Result<bool, SyscoinError> {
        // Strip any 0x prefix
        let actual_blob_id = if let Some(stripped) = blob_id.strip_prefix("0x") {
            stripped
        } else {
            blob_id
        };

        // Use named parameters but don't request actual data
        let params = vec![json!({
            "versionhash_or_txid": actual_blob_id,
        })];

        let response = self.rpc_client.call("getnevmblobdata", &params).await?;

        // Extract finality status from response
        let is_final = response
            .get("chainlock")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        Ok(is_final)
    }

    /// Create or load a wallet by name
    pub async fn create_or_load_wallet(&self, wallet_name: &str) -> Result<(), SyscoinError> {
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
    async fn call(&self, method: &str, _params: &[Value]) -> Result<Value, SyscoinError> {
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

    async fn call_wallet(&self, method: &str, _params: &[Value]) -> Result<Value, SyscoinError> {
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

    async fn get_balance(&self, _account: Option<&str>, _include_watchonly: Option<bool>) -> Result<f64, SyscoinError> {
        Ok(10.5)
    }

    async fn http_get(&self, _url: &str) -> Result<Vec<u8>, SyscoinError> {
        Ok(b"mock_data".to_vec())
    }
}