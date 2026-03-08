# Integration Testing with Forked Chain + Mock CLOB

## Problem

The CLI's on-chain code paths (auth, order signing, balance queries) can only be tested against Polymarket's production API today. Polymarket offers no testnet or staging environment. We need a way to verify these flows in CI without spending real money.

## Approach

Hybrid environment: Anvil fork of Polygon mainnet (real contract state) + in-process mock CLOB server (validates signatures, settles against the fork). Tests run the actual CLI binary against this environment.

## Design

### 1. Local Private Key Signer

Add `--private-key` / `POLYMARKET_PRIVATE_KEY` as a first-class CLI option, mutually exclusive with `--kms-key-id`. Uses `alloy::signers::local::PrivateKeySigner`. Requires checking whether `polymarket-client-sdk` accepts generic `impl Signer` or only `AwsSigner` — if the latter, add a thin wrapper enum dispatching to either backend.

Useful beyond testing: enables local dev without AWS KMS.

### 2. Mock CLOB Server

An `axum` HTTP server started in-process on a random port. Implements the minimum endpoints the CLI hits:

- `POST /auth/l1` and `POST /auth/l2` — auth flow, returns API credentials
- `POST /order` — accepts signed order, validates EIP-712 signature, submits to CTF Exchange contract on the Anvil fork
- `GET /balance-allowance` — reads USDC/CTF balances from the fork

Shared state via `Arc<MockClobState>` holding the Anvil provider and contract handles.

When the mock receives a signed order it:
1. Validates the EIP-712 signature matches order parameters
2. Recovers the signer address
3. Calls Polymarket's CTF Exchange contract on the fork to verify the order would be accepted
4. Optionally executes the order using Anvil account impersonation

### 3. Contract ABIs

ABIs needed for fork interaction:
- **CTF Exchange** — order settlement
- **USDC (ERC20)** — balance checks and approvals
- **Conditional Tokens Framework (CTF)** — position token balances

Sourced from Polygonscan / Polymarket deployment docs. Checked into `tests/abi/`.

### 4. Test Account Setup

Per-test, using Anvil cheatcodes:
- `anvil_setBalance` to give the test wallet USDC
- Approve CTF Exchange to spend USDC
- Simulates a funded, ready-to-trade account

### 5. Test Structure

```
tests/
  cli.rs                    (existing CLI tests)
  integration/
    mod.rs
    mock_clob.rs            (axum mock server)
    fork_setup.rs           (Anvil fork + account funding)
    test_orders.rs          (order placement/cancel tests)
    test_account.rs         (balance/allowance tests)
    test_auth.rs            (authentication flow tests)
```

Feature-gated behind `integration-tests` Cargo feature. Dev-dependencies: `axum`, `tokio`, `alloy` test utilities.

### 6. CI Job

New job in `ci.yml`:
- Runs after check/clippy/fmt
- Installs Foundry via `foundry-rs/foundry-toolchain` GitHub Action
- Polygon RPC URL stored as GitHub Actions secret (Alchemy/Infura)
- Runs `cargo test --features integration-tests`
- Fork pinned to a known block number for determinism

### Test Cases

- **Auth flow** — CLI authenticates against mock CLOB
- **Place order** — CLI signs and posts a limit order, mock validates and settles on fork
- **Query balance** — CLI reads balance from mock backed by fork state
- **Cancel order** — CLI cancels a previously placed order
- **Error cases** — insufficient balance, invalid parameters, rejected signatures

## What This Validates

- EIP-712 signatures produced by the CLI are valid on-chain
- Order structs match what Polymarket's contracts expect
- Auth flow works end-to-end
- Balance/allowance queries return correct data

## What This Does Not Validate

- Real CLOB order matching and fill behavior
- API rate limiting, latency, or production-specific error modes
- KMS signing specifically (tests use local keys — same `Signer` trait)

## Risks

- **RPC dependency in CI**: If the Polygon RPC provider is down, tests fail. Mitigate with generous timeouts and allowing manual re-runs.
- **Contract ABI drift**: If Polymarket upgrades contracts. Low risk for established contracts; fork is pinned to a known block.
