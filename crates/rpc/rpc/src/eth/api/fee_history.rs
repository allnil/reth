//! Consist of types adjacent to the fee history cache and its configs

use crate::eth::{cache::EthStateCache, error::EthApiError};

use futures::{Stream, StreamExt};
use metrics::atomics::AtomicU64;
use reth_interfaces::RethResult;
use reth_primitives::{Receipt, SealedBlock, TransactionSigned, B256, U256};
use reth_provider::{BlockReaderIdExt, CanonStateNotification, ChainSpecProvider};
use reth_rpc_types::TxGasAndReward;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt::Debug,
    sync::{atomic::Ordering::SeqCst, Arc},
};

/// Settings for the [FeeHistoryCache].
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeHistoryCacheConfig {
    /// Max number of blocks in cache.
    ///
    /// Default is 1024.
    pub max_blocks: u64,
    /// Percentile approximation resolution
    ///
    /// Default is 4 which means 0.25
    pub resolution: u64,
}

impl Default for FeeHistoryCacheConfig {
    fn default() -> Self {
        FeeHistoryCacheConfig { max_blocks: 1024, resolution: 4 }
    }
}

/// Wrapper struct for BTreeMap
#[derive(Debug, Clone)]
pub struct FeeHistoryCache {
    /// Stores the lower bound of the cache
    lower_bound: Arc<AtomicU64>,
    upper_bound: Arc<AtomicU64>,
    /// Config for FeeHistoryCache, consists of resolution for percentile approximation
    /// and max number of blocks
    config: FeeHistoryCacheConfig,
    entries: Arc<tokio::sync::RwLock<BTreeMap<u64, FeeHistoryEntry>>>,
    eth_cache: EthStateCache,
}

impl FeeHistoryCache {
    /// Creates new FeeHistoryCache instance, initialize it with the mose recent data, set bounds
    pub fn new(eth_cache: EthStateCache, config: FeeHistoryCacheConfig) -> Self {
        let init_tree_map = BTreeMap::new();

        let entries = Arc::new(tokio::sync::RwLock::new(init_tree_map));

        let upper_bound = Arc::new(AtomicU64::new(0));
        let lower_bound = Arc::new(AtomicU64::new(0));

        FeeHistoryCache { config, entries, upper_bound, lower_bound, eth_cache }
    }

    /// How the cache is configured.
    pub fn config(&self) -> &FeeHistoryCacheConfig {
        &self.config
    }

    /// Returns the configured resolution for percentile approximation.
    #[inline]
    pub fn resolution(&self) -> u64 {
        self.config.resolution
    }

    /// Processing of the arriving blocks
    pub async fn on_new_blocks<'a, I>(&self, blocks: I)
    where
        I: Iterator<Item = &'a SealedBlock>,
    {
        let mut entries = self.entries.write().await;

        for block in blocks {
            let mut fee_history_entry = FeeHistoryEntry::new(block);
            let percentiles = self.predefined_percentiles();

            if let Ok(Some((transactions, receipts))) =
                self.eth_cache.get_transactions_and_receipts(fee_history_entry.header_hash).await
            {
                fee_history_entry.rewards = calculate_reward_percentiles_for_block(
                    &percentiles,
                    fee_history_entry.gas_used,
                    fee_history_entry.base_fee_per_gas,
                    transactions,
                    receipts,
                )
                .unwrap_or_default();

                entries.insert(block.number, fee_history_entry);
            } else {
                break
            }
        }

        while entries.len() > self.config.max_blocks as usize {
            entries.pop_first();
        }

        if entries.len() == 0 {
            self.upper_bound.store(0, SeqCst);
            self.lower_bound.store(0, SeqCst);
            return
        }

        let upper_bound = *entries.last_entry().expect("Contains at least one entry").key();
        let lower_bound = *entries.first_entry().expect("Contains at least one entry").key();
        self.upper_bound.store(upper_bound, SeqCst);
        self.lower_bound.store(lower_bound, SeqCst);
    }

    /// Get UpperBound value for FeeHistoryCache
    pub fn upper_bound(&self) -> u64 {
        self.upper_bound.load(SeqCst)
    }

    /// Get LowerBound value for FeeHistoryCache
    pub fn lower_bound(&self) -> u64 {
        self.lower_bound.load(SeqCst)
    }

    /// Collect fee history for given range.
    /// This function retrieves fee history entries from the cache for the specified range.
    /// If the requested range (star_block to end_block) is within the cache bounds,
    /// it returns the corresponding entries.
    /// Otherwise it returns None.
    pub async fn get_history(
        &self,
        start_block: u64,
        end_block: u64,
    ) -> RethResult<Option<Vec<FeeHistoryEntry>>> {
        let lower_bound = self.lower_bound();
        let upper_bound = self.upper_bound();
        if start_block >= lower_bound && end_block <= upper_bound {
            let entries = self.entries.read().await;
            let result = entries
                .range(start_block..=end_block + 1)
                .map(|(_, fee_entry)| fee_entry.clone())
                .collect();
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    /// Generates predefined set of percentiles
    ///
    /// This returns 100 * resolution points
    pub fn predefined_percentiles(&self) -> Vec<f64> {
        let res = self.resolution() as f64;
        (0..=100 * self.resolution()).map(|p| p as f64 / res).collect()
    }
}

/// Awaits for new chain events and directly inserts them into the cache so they're available
/// immediately before they need to be fetched from disk.
pub async fn fee_history_cache_new_blocks_task<St, Provider>(
    fee_history_cache: FeeHistoryCache,
    mut events: St,
    provider: Provider,
) where
    St: Stream<Item = CanonStateNotification> + Unpin + 'static,
    Provider: BlockReaderIdExt + ChainSpecProvider + 'static,
{
    // Init default state
    if fee_history_cache.upper_bound() == 0 {
        let last_block_number = provider.last_block_number().unwrap_or(0);

        let start_block = if last_block_number > fee_history_cache.config.max_blocks {
            last_block_number - fee_history_cache.config.max_blocks
        } else {
            0
        };

        let blocks = provider.block_range(start_block..=last_block_number).unwrap_or_default();
        let sealed = blocks.into_iter().map(|block| block.seal_slow()).collect::<Vec<_>>();

        fee_history_cache.on_new_blocks(sealed.iter()).await;
    }

    while let Some(event) = events.next().await {
        if let Some(committed) = event.committed() {
            // we're only interested in new committed blocks
            let (blocks, _) = committed.inner();

            let blocks = blocks.iter().map(|(_, v)| v.block.clone()).collect::<Vec<_>>();

            fee_history_cache.on_new_blocks(blocks.iter()).await;
        }
    }
}

/// Calculates reward percentiles for transactions in a block header.
/// Given a list of percentiles and a sealed block header, this function computes
/// the corresponding rewards for the transactions at each percentile.
///
/// The results are returned as a vector of U256 values.
pub(crate) fn calculate_reward_percentiles_for_block(
    percentiles: &[f64],
    gas_used: u64,
    base_fee_per_gas: u64,
    transactions: Vec<TransactionSigned>,
    receipts: Vec<Receipt>,
) -> Result<Vec<U256>, EthApiError> {
    let mut transactions = transactions
        .into_iter()
        .zip(receipts)
        .scan(0, |previous_gas, (tx, receipt)| {
            // Convert the cumulative gas used in the receipts
            // to the gas usage by the transaction
            //
            // While we will sum up the gas again later, it is worth
            // noting that the order of the transactions will be different,
            // so the sum will also be different for each receipt.
            let gas_used = receipt.cumulative_gas_used - *previous_gas;
            *previous_gas = receipt.cumulative_gas_used;

            Some(TxGasAndReward {
                gas_used,
                reward: tx.effective_tip_per_gas(Some(base_fee_per_gas)).unwrap_or_default(),
            })
        })
        .collect::<Vec<_>>();

    // Sort the transactions by their rewards in ascending order
    transactions.sort_by_key(|tx| tx.reward);

    // Find the transaction that corresponds to the given percentile
    //
    // We use a `tx_index` here that is shared across all percentiles, since we know
    // the percentiles are monotonically increasing.
    let mut tx_index = 0;
    let mut cumulative_gas_used = transactions.first().map(|tx| tx.gas_used).unwrap_or_default();
    let mut rewards_in_block = Vec::new();
    for percentile in percentiles {
        // Empty blocks should return in a zero row
        if transactions.is_empty() {
            rewards_in_block.push(U256::ZERO);
            continue
        }

        let threshold = (gas_used as f64 * percentile / 100.) as u64;
        while cumulative_gas_used < threshold && tx_index < transactions.len() - 1 {
            tx_index += 1;
            cumulative_gas_used += transactions[tx_index].gas_used;
        }
        rewards_in_block.push(U256::from(transactions[tx_index].reward));
    }

    Ok(rewards_in_block)
}

#[derive(Debug, Clone)]
pub struct FeeHistoryEntry {
    pub base_fee_per_gas: u64,
    pub gas_used_ratio: f64,
    pub gas_used: u64,
    pub gas_limit: u64,
    pub header_hash: B256,
    pub rewards: Vec<U256>,
}

impl FeeHistoryEntry {
    pub fn new(block: &SealedBlock) -> Self {
        FeeHistoryEntry {
            base_fee_per_gas: block.base_fee_per_gas.unwrap_or_default(),
            gas_used_ratio: block.gas_used as f64 / block.gas_limit as f64,
            gas_used: block.gas_used,
            header_hash: block.hash,
            gas_limit: block.gas_limit,
            rewards: Vec::new(),
        }
    }
}
