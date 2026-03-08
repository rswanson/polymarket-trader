use alloy::primitives::{Address, B256, ChainId};
use alloy::signers::Signer;
use alloy::signers::aws::AwsSigner;
use alloy::signers::local::PrivateKeySigner;
use async_trait::async_trait;
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

pub fn create_local_signer(private_key: &str) -> anyhow::Result<PrivateKeySigner> {
    let key = private_key.strip_prefix("0x").unwrap_or(private_key);
    let mut signer: PrivateKeySigner = key
        .parse()
        .map_err(|e| anyhow::anyhow!("Failed to parse private key: {e}"))?;
    signer.set_chain_id(Some(POLYGON));
    Ok(signer)
}

pub enum AnySigner {
    Local(PrivateKeySigner),
    Kms(AwsSigner),
}

#[async_trait]
impl Signer for AnySigner {
    async fn sign_hash(&self, hash: &B256) -> alloy::signers::Result<alloy::primitives::Signature> {
        match self {
            Self::Local(s) => s.sign_hash(hash).await,
            Self::Kms(s) => s.sign_hash(hash).await,
        }
    }

    fn address(&self) -> Address {
        match self {
            Self::Local(s) => s.address(),
            Self::Kms(s) => s.address(),
        }
    }

    fn chain_id(&self) -> Option<ChainId> {
        match self {
            Self::Local(s) => s.chain_id(),
            Self::Kms(s) => s.chain_id(),
        }
    }

    fn set_chain_id(&mut self, chain_id: Option<ChainId>) {
        match self {
            Self::Local(s) => s.set_chain_id(chain_id),
            Self::Kms(s) => s.set_chain_id(chain_id),
        }
    }
}
