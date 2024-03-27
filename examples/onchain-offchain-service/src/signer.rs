use alloy_eips;
use alloy_signer::Signature;
use alloy_signer_wallet::LocalWallet;

use alloy_network::TxSignerSync;
pub struct SignerService {
    wallet: LocalWallet,
}

impl SignerService {
    pub fn new() -> SignerService {
        // TODO: Get private key from cli

        // Instantiate a signer.
        let wallet = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80" // anvil account 0
            .parse::<alloy_signer_wallet::LocalWallet>()
            .unwrap();
        Self { wallet }
    }

    pub fn sign_signable(
        self,
        tx: &mut dyn alloy_consensus::SignableTransaction<alloy_signer::Signature>,
    ) -> eyre::Result<(Signature)> {
        let signature = self.wallet.sign_transaction_sync(tx)?;
        Ok(signature)
    }

    pub fn sign_tx_eip4844(self, mut tx: alloy_consensus::TxEip4844) -> eyre::Result<(Signature)> {
        let signature = self.wallet.sign_transaction_sync(&mut tx)?;

        Ok(signature)
    }

    pub fn sign_tx_eip1559(self, mut tx: alloy_consensus::TxEip1559) -> eyre::Result<(Signature)> {
        let signature = self.wallet.sign_transaction_sync(&mut tx)?;

        Ok(signature)
    }

    pub fn sign_tx_eip2930(self, mut tx: alloy_consensus::TxEip2930) -> eyre::Result<(Signature)> {
        let signature = self.wallet.sign_transaction_sync(&mut tx)?;

        Ok(signature)
    }
}
