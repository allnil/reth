use reth::primitives::SealedHeader;

// draft of the some OffChain service
pub struct OffChainService {}

impl OffChainService {
    pub fn react_on_new_block(self, new_header: SealedHeader) -> eyre::Result<()> {
        println!("service reacts on new block in some fancy manner");
        return Ok(())
    }

    fn escalate_gas_fee(self) -> eyre::Result<()> {
        return Ok(())
    }
}
