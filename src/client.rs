use alloy::signers::Signer;
use polymarket_client_sdk::auth::Normal;
use polymarket_client_sdk::auth::state::{Authenticated, Unauthenticated};
use polymarket_client_sdk::clob::{Client, Config};

pub fn create_unauthenticated_client(host: &str) -> anyhow::Result<Client<Unauthenticated>> {
    let client = Client::new(host, Config::default())?;
    Ok(client)
}

pub async fn create_authenticated_client<S: Signer>(
    host: &str,
    signer: &S,
) -> anyhow::Result<Client<Authenticated<Normal>>> {
    let config = Config::builder().use_server_time(true).build();

    let client = Client::new(host, config)?
        .authentication_builder(signer)
        .authenticate()
        .await?;

    Ok(client)
}
