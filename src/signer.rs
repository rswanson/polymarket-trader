use alloy::signers::aws::AwsSigner;
use aws_config::BehaviorVersion;
use polymarket_client_sdk::POLYGON;

pub async fn create_kms_signer(key_id: &str) -> anyhow::Result<AwsSigner> {
    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let kms_client = aws_sdk_kms::Client::new(&config);

    let signer = AwsSigner::new(kms_client, key_id.to_owned(), Some(POLYGON))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create KMS signer: {e}"))?;

    Ok(signer)
}
