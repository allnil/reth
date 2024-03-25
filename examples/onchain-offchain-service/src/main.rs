//!
//! ```not_rust
//! cargo run -p onchain-offchain-service -- node --dev
//! ```
mod service;
pub mod signer;

use alloy_consensus::SignableTransaction;
use alloy_network::TxSignerSync;
use reth::cli::Cli;
use reth_node_ethereum::EthereumNode;
use reth::builder::NodeHandle;
use futures::StreamExt;
use reth::primitives::alloy_primitives::private::rand::thread_rng;
use reth::primitives::{AccessList, hex, Transaction, TransactionKind, TransactionSigned};
use reth::providers::{CanonStateSubscriptions, TransactionsProvider};
use reth::transaction_pool::{PoolTransaction, TransactionOrigin, TransactionPool};
use reth::transaction_pool::test_utils::TransactionGenerator;
use reth_rpc::eth::error::EthApiError;
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

            println!("Spawning trace task!");
            // Spawn an async block to listen for transactions.
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
                    let Some(mut txs)= node.provider.transactions_by_block(BlockHashOrNumber::Number(new_block.number)).unwrap() {
                        for tx in txs {
                            println!("do something fancy with tx: {tx:?}");
                        }
                    };
                }
            }));

            /* TODO:
                1. Get new state via new canon state notification stream
                2. Scan transactions, do some job
                3. Create new transaction, sign it, push it
             */

            // SIGNER draft
            use alloy_consensus::{TxLegacy, SignableTransaction, TxEip1559};
            use alloy_primitives::{U256, address, bytes};

            let mut tx = Transaction::Eip1559( reth::primitives::TxEip1559 {
                chain_id: 1,
                nonce: 0x42,
                gas_limit: 44386,
                to: TransactionKind::Call( hex!("0xa0Ee7A142d267C1f36714E4a8F75612F20a79720").into()),
                value: U256::from(1e18),
                input: bytes!(),
                max_fee_per_gas: 0x4a817c800,
                max_priority_fee_per_gas: 0x3b9aca00,
                access_list: AccessList::default(),
            });

            let signer = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80" // anvil test account(0) 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
                .parse::<alloy_signer_wallet::LocalWallet>()?;

           // println!("push custom signed tx ONCHAIN: {signed_tx:?}");

            node_exit_future.await
        })
        .unwrap();
}
