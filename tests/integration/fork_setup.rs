//! Anvil fork setup utilities for integration tests.
//!
//! Provides helpers to spawn an Anvil instance forking Polygon mainnet,
//! fund test accounts with USDC.e, and approve the Polymarket exchange contracts.

use alloy::{
    network::EthereumWallet,
    node_bindings::{Anvil, AnvilInstance},
    primitives::{Address, B256, U256, address, keccak256},
    providers::{ProviderBuilder, ext::AnvilApi},
    signers::local::PrivateKeySigner,
    sol,
};

// ── Polygon mainnet contract addresses ──

/// Polymarket CTF Exchange on Polygon.
pub const CTF_EXCHANGE: Address = address!("0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E");

/// Polymarket Neg Risk CTF Exchange on Polygon.
pub const NEG_RISK_CTF_EXCHANGE: Address = address!("0xC5d563A36AE78145C45a50134d48A1215220f80a");

/// Gnosis Conditional Tokens on Polygon.
pub const CONDITIONAL_TOKENS: Address = address!("0x4D97DCd97eC945f40cF65F87097ACe5EA0476045");

/// USDC.e (bridged USDC) on Polygon.
pub const USDC: Address = address!("0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174");

// ── Minimal ERC20 interface ──

sol! {
    #[sol(rpc)]
    interface IERC20 {
        function balanceOf(address account) external view returns (uint256);
        function approve(address spender, uint256 amount) external returns (bool);
        function allowance(address owner, address spender) external view returns (uint256);
    }
}

// ── Fork environment ──

/// Holds the Anvil instance, RPC URL, and test signer.
///
/// The Anvil process is killed when this struct is dropped.
pub struct ForkEnv {
    /// The running Anvil instance. Kept alive for the duration of the test.
    pub anvil: AnvilInstance,
    /// HTTP RPC URL for the forked Anvil node.
    pub rpc_url: String,
    /// Pre-funded test signer derived from Anvil's first dev account.
    pub signer: PrivateKeySigner,
}

/// Spawn an Anvil instance forking Polygon mainnet.
///
/// Requires `POLYGON_RPC_URL` environment variable to be set (e.g.
/// `https://polygon-rpc.com` or an Alchemy/Infura endpoint).
///
/// # Panics
///
/// Panics if `POLYGON_RPC_URL` is not set or if Anvil fails to start.
pub async fn spawn_fork() -> ForkEnv {
    let polygon_rpc = std::env::var("POLYGON_RPC_URL").expect(
        "POLYGON_RPC_URL must be set to a Polygon mainnet RPC endpoint \
         (e.g. https://polygon-rpc.com). Integration tests fork from this URL.",
    );

    let anvil = Anvil::new().fork(&polygon_rpc).spawn();

    let rpc_url = anvil.endpoint();

    // Derive a PrivateKeySigner from the first Anvil dev key.
    let signer: PrivateKeySigner =
        PrivateKeySigner::from_signing_key(anvil.first_key().clone().into());

    ForkEnv {
        anvil,
        rpc_url,
        signer,
    }
}

/// Fund a target address with USDC.e by directly manipulating storage slots.
///
/// ERC-20 contracts typically store balances in a `mapping(address => uint256)`.
/// The slot is `keccak256(abi.encode(address, mappingSlot))`. Since the exact
/// mapping slot varies between contracts, we try common slots (0, 2, 9) until
/// one works.
///
/// # Panics
///
/// Panics if no known slot successfully sets the balance.
pub async fn fund_usdc(fork: &ForkEnv, target: Address, amount: U256) {
    let provider = ProviderBuilder::new().connect_http(fork.anvil.endpoint_url());

    // Common mapping slots for ERC-20 balanceOf storage.
    let candidate_slots: &[u64] = &[0, 2, 9];

    for &slot in candidate_slots {
        // Compute storage key: keccak256(abi.encode(address, slot))
        let mut buf = [0u8; 64];
        buf[12..32].copy_from_slice(target.as_slice());
        buf[32..64].copy_from_slice(&U256::from(slot).to_be_bytes::<32>());
        let storage_key = keccak256(buf);

        // Encode the amount as a 32-byte value.
        let value = B256::from(amount.to_be_bytes::<32>());

        // Set the storage slot via Anvil cheatcode.
        let _ = provider
            .anvil_set_storage_at(USDC, U256::from_be_bytes(storage_key.0), value)
            .await;

        // Verify by calling balanceOf.
        let usdc = IERC20::new(USDC, &provider);
        let result = usdc.balanceOf(target).call().await;

        if let Ok(bal) = result
            && bal >= amount
        {
            return;
        }
    }

    panic!(
        "fund_usdc: could not set USDC balance for {target}. \
         Tried mapping slots 0, 2, 9 — none matched the contract's storage layout."
    );
}

/// Approve both CTF Exchange and Neg Risk CTF Exchange to spend the maximum
/// amount of USDC.e on behalf of the test wallet.
///
/// # Panics
///
/// Panics if either approval transaction fails.
pub async fn approve_exchange(fork: &ForkEnv) {
    let wallet = EthereumWallet::from(fork.signer.clone());
    let provider = ProviderBuilder::new()
        .wallet(wallet)
        .connect_http(fork.anvil.endpoint_url());

    let usdc = IERC20::new(USDC, &provider);

    // Approve CTF Exchange.
    let receipt = usdc
        .approve(CTF_EXCHANGE, U256::MAX)
        .send()
        .await
        .expect("failed to send CTF Exchange approval tx")
        .get_receipt()
        .await
        .expect("failed to get CTF Exchange approval receipt");
    assert!(receipt.status(), "CTF Exchange USDC approval reverted");

    // Approve Neg Risk CTF Exchange.
    let receipt = usdc
        .approve(NEG_RISK_CTF_EXCHANGE, U256::MAX)
        .send()
        .await
        .expect("failed to send Neg Risk CTF Exchange approval tx")
        .get_receipt()
        .await
        .expect("failed to get Neg Risk CTF Exchange approval receipt");
    assert!(
        receipt.status(),
        "Neg Risk CTF Exchange USDC approval reverted"
    );
}
