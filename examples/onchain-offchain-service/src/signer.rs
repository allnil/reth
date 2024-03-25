use alloy_consensus::{TxLegacy, SignableTransaction, TxEip1559};
use alloy_primitives::{U256, address, bytes, Sign};
use alloy_signer::{Signer, SignerSync, Signature};
use alloy_network::{TxSignerSync};
use reth::primitives::Transaction;

pub fn sign(tx: Transaction) -> eyre::Result<(Signature)> {
    // Instantiate a signer.
    let signer = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80" // anvil test account(0) 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
        .parse::<alloy_signer_wallet::LocalWallet>()?;

    // Sign it.
    let signature = signer.sign_transaction_sync(&mut tx)?; // trait bounds HMM

    Ok(signature)
}
