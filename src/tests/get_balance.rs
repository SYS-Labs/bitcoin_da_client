use crate::client::{SyscoinClient, MockRpcClient};

#[test]
fn test_get_balance() {
    let mock_client = MockRpcClient;
    let syscoin_client = SyscoinClient::new(mock_client, "http://mocked_poda_url/");

    let balance = syscoin_client.get_balance().unwrap();
    assert_eq!(balance, 100.0, "Balance should match mocked value");
}
