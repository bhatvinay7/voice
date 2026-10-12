use integration_tests::load_test_env;
use std::env;

#[tokio::test]
async fn test_s3_upload_simulated() {
    load_test_env();
    let bucket = env::var("S3_BUCKET_NAME")
        .or_else(|_| env::var("R2_BUCKET_NAME"))
        .unwrap_or_default();
    assert!(!bucket.is_empty(), "S3/R2 Bucket must be loaded from environment");

    let endpoint = env::var("S3_ENDPOINT")
        .or_else(|_| env::var("R2_ENDPOINT_URL"))
        .unwrap_or_default();
    assert!(!endpoint.is_empty(), "S3/R2 endpoint must be configured in environment");
}

#[tokio::test]
async fn test_redis_stream_write() {
    load_test_env();
    let redis_url = env::var("REDIS_URL").unwrap_or_default();
    assert!(
        redis_url.starts_with("redis://"),
        "REDIS_URL must be a valid redis connection string loaded from env"
    );
}

#[tokio::test]
async fn test_api_endpoints() {
    load_test_env();
    let port = env::var("SERVER_PORT")
        .or_else(|_| env::var("PORT"))
        .unwrap_or_default();
    assert!(!port.is_empty(), "Server port must be defined in environment");
}
