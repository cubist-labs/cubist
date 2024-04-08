use ethers::{
    middleware::transformer::{Transformer, TransformerError},
    types::{transaction::eip2718::TypedTransaction, TransactionRequest, U256},
};

/// Transformer that converts all transactions into legacy transactions
#[derive(Debug, Clone)]
pub struct LegacyTransformer {
    /// Gas price to set when converting to [`TypedTransaction::Legacy`]
    pub gas_price: U256,
}

impl Transformer for LegacyTransformer {
    fn transform(&self, tx: &mut TypedTransaction) -> Result<(), TransformerError> {
        // Convert the typed transaction into a legacy transaction and back into a typed
        // transaction.
        let tx_req: TransactionRequest = tx.clone().into();
        let tx_req = tx_req.gas_price(self.gas_price);
        *tx = tx_req.into();
        Ok(())
    }
}
