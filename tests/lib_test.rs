#[cfg(test)]
mod tests {
    use mockito::Server;
    use serde_json::json;
    use tokio;
    use bitcoin_da_client::{SyscoinClient, MAX_BLOB_SIZE};
    use hex;


    #[tokio::test]
    async fn test_syscoin_client_creation() {
        let timeout = Some(std::time::Duration::from_secs(30));
        let result = SyscoinClient::new(
            "http://localhost:8888",
            "user",
            "password",
            "http://poda.example.com",
            timeout,
        );
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_balance() {
        // Create the mock server in a separate thread
        let mut mock_server = std::thread::spawn(|| {
            Server::new()
        }).join().expect("Failed to create mock server");


        let expected_balance = 100.5;

        let mock_response = json!({
            "result": expected_balance,
            "error": null
        });

        // Set up mock response
        let _m = mock_server
            .mock("POST", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(mock_response.to_string())
            .create();

        let client = SyscoinClient::new(
            &mock_server.url(),
            "user",
            "password",
            "http://poda.example.com",
            None,
        )
            .unwrap();

        let balance = client.get_balance().await;

        assert!(balance.is_ok());
        assert_eq!(balance.unwrap(), expected_balance);
    }

    #[tokio::test]
    async fn test_create_blob() {
        // Create the mock server in a separate thread
        let mut mock_server = std::thread::spawn(|| {
            Server::new()
        }).join().expect("Failed to create mock server");
        let expected_hash = "deadbeef";

        // Mock RPC response
        let mock_response = json!({
            "result": {
                "versionhash": expected_hash
            },
            "error": null,
            "id": 1
        });

        let _m = mock_server
            .mock("POST", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(mock_response.to_string())
            .create();

        let client = SyscoinClient::new(
            &mock_server.url(),
            "user",
            "password",
            "http://poda.example.com",
            None,
        )
            .unwrap();

        let result = client.create_blob(&[1, 2, 3, 4]).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), expected_hash);
    }

    #[tokio::test]
    async fn test_get_blob_from_cloud() {
        // Create the mock server in a separate thread
        let mut mock_server = std::thread::spawn(|| {
            Server::new()
        }).join().expect("Failed to create mock server");
        
        let expected_data = b"retrieved data".to_vec();
        let version_hash = "deadbeef";

        // Mock HTTP GET response
        let _m = mock_server
            .mock("GET", format!("/blob/{}", version_hash).as_str())
            .with_status(200)
            .with_body(&expected_data)
            .create();

        let client = SyscoinClient::new(
            "http://localhost:8888", // RPC URL (won't be used)
            "user",                   // Username
            "password",               // Password
            &mock_server.url(),       // PODA cloud URL
            None                      // Timeout
        ).unwrap();

        // Use get_blob with a non-existent RPC server to force fallback to cloud
        // First make sure RPC will fail by mocking it to return an error
        mock_server
            .mock("POST", "/")
            .with_status(500)
            .with_body("RPC error")
            .create();
        
        // Then call get_blob which should fall back to the cloud endpoint
        let result = client.get_blob(version_hash).await;
        
        assert!(result.is_ok(), "Error: {:?}", result.err());
        assert_eq!(result.unwrap(), expected_data);
    }

    #[tokio::test]
    async fn test_create_or_load_wallet() {
        // Create the mock server in a separate thread
        let mut mock_server = std::thread::spawn(|| {
            Server::new()
        }).join().expect("Failed to create mock server");
        let wallet_name = "test_wallet";

        // Mock successful wallet creation response
        let mock_response = json!({
            "result": {},
            "error": null,
            "id": 1
        });

        let _m = mock_server
            .mock("POST", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(mock_response.to_string())
            .create();

        let client = SyscoinClient::new(
            &mock_server.url(),
            "user",
            "password",
            "http://poda.example.com",
            None,
        )
            .unwrap();

        let result = client.create_or_load_wallet(wallet_name).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_error_handling() {
        // Create the mock server in a separate thread
        let mut mock_server = std::thread::spawn(|| {
            Server::new()
        }).join().expect("Failed to create mock server");

        // Mock error response
        let mock_response = json!({
            "result": {},
            "error": {
                "code": -32601,
                "message": "Method not found"
            }
        });

        let _m = mock_server
            .mock("POST", "/")
            .with_status(400)
            .with_header("content-type", "application/json")
            .with_body(mock_response.to_string())
            .create();

        let client = SyscoinClient::new(
            &mock_server.url(),
            "user",
            "password",
            "http://poda.example.com",
            None,
        )
            .unwrap();

        let result = client.get_balance().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rpc_request_invalid_json() {
        // Create the mock server in a separate thread
        let mut mock_server = std::thread::spawn(|| {
            Server::new()
        }).join().expect("Failed to create mock server");

        let _m = mock_server
            .mock("POST", "/")
            .with_status(200)
            .with_body("Not a JSON")
            .create();

        let client = SyscoinClient::new(
            &mock_server.url(),
            "user",
            "password",
            "http://poda.example.com",
            None,
        )
            .unwrap();
        let result = client.create_blob(&[1, 2, 3, 4]).await;
        println!("Result: {:?}", result);
        // Expect an error because the response body is not valid JSON.
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_blob() {
        use hex::encode;
        
        let mut mock_server = std::thread::spawn(|| {
            Server::new()
        }).join().expect("Failed to create mock server");
        
        let expected_data = b"hello world blob data".to_vec();
        let hex_data = encode(&expected_data);
        let blob_id = "deadbeef123";
        
        // Mock the RPC endpoint
        let mock_response = json!({
            "result": {
                "data": hex_data
            },
            "error": null,
            "id": 1
        });
        
        // Mock the JSON-RPC POST request 
        mock_server
            .mock("POST", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(mock_response.to_string())
            .create();
            
        // ALSO mock the fallback cloud GET endpoint
        // The url format should match what's in get_blob_from_cloud
        mock_server
            .mock("GET", format!("/{}", blob_id).as_str())
            .with_status(200)
            .with_body(&expected_data)
            .create();
        
        let client = SyscoinClient::new(
            &mock_server.url(),
            "user",
            "password",
            &mock_server.url(), // Same server for both
            None
        ).unwrap();
        
        // Add very detailed debug info
        println!("Server URL: {}", &mock_server.url());
        println!("Blob ID: {}", blob_id);
        
        let result = client.get_blob(blob_id).await;
        assert!(result.is_ok(), "Error: {:?}", result.err());
        assert_eq!(result.unwrap(), expected_data);
    }

    #[tokio::test]
    async fn test_max_blob_size() {
        // Create a client
        let client = SyscoinClient::new(
            "http://dummy-url.com",
            "user",
            "password",
            "http://dummy-poda.com",
            None
        ).unwrap();
        
        // Verify it returns the correct size constant
        assert_eq!(client.max_blob_size(), MAX_BLOB_SIZE);
        
        // Verify it's reasonable (2 MiB)
        assert_eq!(client.max_blob_size(), 2 * 1024 * 1024);
    }

}

