use crate::client::{SyscoinClient, MockRpcClient};

#[tokio::test]
async fn test_create_blob() {
    let mock_client = MockRpcClient;
    let syscoin_client = SyscoinClient::new(mock_client, "http://mocked_poda_url/");

    let data = vec![1, 2, 3];
    let blob_hash = syscoin_client.create_blob(&data).await.unwrap();
    assert_eq!(blob_hash, "mocked_hash", "Blob hash should match mocked value");
}
