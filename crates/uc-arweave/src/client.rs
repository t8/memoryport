use crate::graphql::{GraphQLClient, GraphQLError, TagFilter};
use crate::transaction::{self, TransactionError};
use crate::turbo::{TurboClient, TurboError};
use crate::types::{GraphQLEdge, Tag, TurboUploadResponse};
use crate::wallet::{Wallet, WalletError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ArweaveError {
    #[error("wallet error: {0}")]
    Wallet(#[from] WalletError),
    #[error("transaction error: {0}")]
    Transaction(#[from] TransactionError),
    #[error("turbo upload error: {0}")]
    Turbo(#[from] TurboError),
    #[error("graphql error: {0}")]
    GraphQL(#[from] GraphQLError),
    #[error("no wallet configured")]
    NoWallet,
}

/// High-level Arweave client combining wallet, Turbo uploads, and GraphQL queries.
pub struct ArweaveClient {
    wallet: Option<Wallet>,
    turbo: TurboClient,
    graphql: GraphQLClient,
}

impl ArweaveClient {
    /// Create a new client with a wallet for signing.
    pub fn new(wallet: Wallet, turbo_endpoint: &str, gateway: &str) -> Self {
        Self {
            wallet: Some(wallet),
            turbo: TurboClient::new(turbo_endpoint),
            graphql: GraphQLClient::new(gateway),
        }
    }

    /// Create a read-only client (no wallet, cannot upload).
    pub fn read_only(turbo_endpoint: &str, gateway: &str) -> Self {
        Self {
            wallet: None,
            turbo: TurboClient::new(turbo_endpoint),
            graphql: GraphQLClient::new(gateway),
        }
    }

    /// Create, sign, and upload a data item to Arweave via Turbo.
    pub async fn upload(
        &self,
        data: &[u8],
        tags: &[Tag],
    ) -> Result<TurboUploadResponse, ArweaveError> {
        let wallet = self.wallet.as_ref().ok_or(ArweaveError::NoWallet)?;
        let data_item = transaction::create_data_item(wallet, data, tags, None, None)?;
        let response = self.turbo.upload(&data_item).await?;
        Ok(response)
    }

    /// Query transactions by tags.
    pub async fn query_transactions(
        &self,
        tag_filters: &[TagFilter],
        first: usize,
        after: Option<&str>,
    ) -> Result<(Vec<GraphQLEdge>, bool), ArweaveError> {
        let (edges, has_next) = self
            .graphql
            .query_transactions(tag_filters, first, after)
            .await?;
        Ok((edges, has_next))
    }

    /// Query all transactions matching filters (auto-paginated).
    pub async fn query_all_transactions(
        &self,
        tag_filters: &[TagFilter],
    ) -> Result<Vec<GraphQLEdge>, ArweaveError> {
        let edges = self.graphql.query_all_transactions(tag_filters).await?;
        Ok(edges)
    }

    /// Fetch raw transaction data by ID.
    pub async fn fetch_data(&self, tx_id: &str) -> Result<Vec<u8>, ArweaveError> {
        let data = self.graphql.fetch_transaction_data(tx_id).await?;
        Ok(data)
    }

    /// Get the wallet address, if a wallet is loaded.
    pub fn address(&self) -> Option<&str> {
        self.wallet.as_ref().map(|w| w.address.as_str())
    }
}
