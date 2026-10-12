use integration_tests::load_test_env;
use std::env;

#[tokio::test]
async fn test_livekit_connection() {
    load_test_env();
    let url = env::var("LIVEKIT_URL").unwrap_or_default();
    let key = env::var("LIVEKIT_API_KEY").unwrap_or_default();
    
    assert!(
        url.starts_with("ws://") || url.starts_with("wss://"),
        "LIVEKIT_URL must be a valid websocket url"
    );
    assert!(!key.is_empty(), "LIVEKIT_API_KEY must be configured in environment");
    
    // TODO: Initialize livekit::Room::connect() and test room join
}
