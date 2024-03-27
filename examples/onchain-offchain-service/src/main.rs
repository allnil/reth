//!
//! ```not_rust
//! cargo run -p onchain-offchain-service -- node --dev
//! ```
mod service;
pub mod signer;

use alloy_consensus::{SignableTransaction, TxEnvelope};
use alloy_eips::eip2718::Encodable2718;
use alloy_network::TxSignerSync;
use alloy_rlp::Encodable;
use futures::StreamExt;
use reth::{
    builder::NodeHandle,
    cli::Cli,
    primitives::{FromRecoveredTransaction, TransactionSigned},
    providers::{CanonStateSubscriptions, TransactionsProvider},
    revm::interpreter::gas::ZERO,
    transaction_pool::{PoolTransaction, TransactionOrigin, TransactionPool},
};

use reth_node_ethereum::EthereumNode;
use std::hash::Hash;
use reth::primitives::{BlockId, BlockNumberOrTag, Address, U64}; // TODO: add from_str for alloy
use reth::rpc::api::EthApiClient;
use std::str::FromStr;

use crate::signer::SignerService;
use reth_rpc_types::BlockHashOrNumber;
use reth_transaction_pool::EthPooledTransaction;

fn main() {
    Cli::parse_args()
        .run(|builder, _args| async move {
            // launch the node
            let NodeHandle { node, node_exit_future } = builder
                .node(EthereumNode::default())
                .on_component_initialized(|_ctx| {
                    println!("hi from initialized component!");
                    Ok(())
                })
                .launch()
                .await?;

            // let mut local_wallet = SignerService::new();

            println!("Spawning trace task!");
            // Spawn an async block to listen for transactions.
            let node_clone = node.clone();
            node.task_executor.spawn(Box::pin(async move {
                let new_headers_stream =
                    node.provider.canonical_state_stream().flat_map(|new_chain| {
                        let headers = new_chain
                            .committed()
                            .map(|chain| chain.headers().collect::<Vec<_>>())
                            .unwrap_or_default();
                        futures::stream::iter(headers)
                    });

                let mut block_stream = new_headers_stream.map(Box::new);

                while let Some(new_block) = block_stream.next().await {
                    println!("Block received: {new_block:?}");
                    if let Some(mut txs) = node
                        .provider
                        .transactions_by_block(BlockHashOrNumber::Number(new_block.number))
                        .unwrap()
                    {
                        for tx in txs {
                            let mut local_wallet = SignerService::new();

                            println!("do something fancy with tx: {tx:?}");

                            let nonce = node_clone.rpc_server_handle().http_client().unwrap().transaction_count(
                                Address::from_str("0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80").unwrap(),
                                Some(BlockId::from(new_block.number))).await.unwrap();
                            let new_nonce = (nonce.to::<u64>()) + 1;

                            let mut my_tx = alloy_consensus::TxEip1559 {
                                chain_id: tx.chain_id().unwrap(),
                                nonce: new_nonce,
                                gas_limit: tx.gas_limit() * 2,
                                to: alloy_primitives::TxKind::Call(tx.to().unwrap()),
                                value: tx.value(),
                                input: tx.input().clone(),
                                max_fee_per_gas: tx.max_fee_per_gas() * 2,
                                max_priority_fee_per_gas: tx.max_priority_fee_per_gas().unwrap() *
                                    2,
                                access_list: alloy_eips::eip2930::AccessList::default(),
                            };

                            let mut encoded = Vec::new();
                            my_tx.encode_for_signing(&mut encoded);

                            let signature = local_wallet.sign_signable(&mut my_tx.clone()).unwrap();
                            let signed_tx = my_tx.clone().into_signed(signature);

                            let enveloped_tx = TxEnvelope::from(signed_tx).encoded_2718();

                            let decoded_tx = TransactionSigned::decode_enveloped_typed_transaction(
                                &mut enveloped_tx.as_ref(),
                            )
                            .unwrap();
                            let pool_tx = EthPooledTransaction::from_recovered_transaction(
                                decoded_tx.clone().into_ecrecovered().unwrap(),
                            );
                            println!("get decoded tx: {decoded_tx:?}");

                            let res = node
                                .pool
                                .add_transaction(TransactionOrigin::Private, pool_tx)
                                .await
                                .unwrap();

                            println!("PUSHED tx to the POOL");
                        }
                    };
                }
            }));

            node_exit_future.await
        })
        .unwrap();
}
