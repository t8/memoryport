use crate::types::{SignedDataItem, TurboUploadResponse};
use thiserror::Error;
use tracing::{debug, warn};

#[derive(Debug, Error)]
pub enum TurboError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("upload rejected with status {status}: {body}")]
    UploadRejected { status: u16, body: String },
}

/// Client for the ar.io Turbo upload service.
#[derive(Debug, Clone)]
pub struct TurboClient {
    http: reqwest::Client,
    endpoint: String,
}

impl TurboClient {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            endpoint: endpoint.into(),
        }
    }

    /// Upload a signed ANS-104 data item to ar.io Turbo.
    ///
    /// Sends raw binary bytes to `POST /v1/tx` with `application/octet-stream`.
    pub async fn upload(&self, data_item: &SignedDataItem) -> Result<TurboUploadResponse, TurboError> {
        let url = format!("{}/v1/tx", self.endpoint);

        debug!(
            id = %data_item.id,
            size = data_item.bytes.len(),
            "uploading data item to Turbo"
        );

        let response = self
            .http
            .post(&url)
            .header("Content-Type", "application/octet-stream")
            .body(data_item.bytes.clone())
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            warn!(status = %status, body = %body, "Turbo upload rejected");
            return Err(TurboError::UploadRejected {
                status: status.as_u16(),
                body,
            });
        }

        let upload_response: TurboUploadResponse = response.json().await?;
        debug!(
            id = %upload_response.id,
            "data item uploaded successfully"
        );

        Ok(upload_response)
    }

    /// Check if the Turbo endpoint is reachable.
    pub async fn health_check(&self) -> Result<bool, TurboError> {
        let url = format!("{}/v1/health", self.endpoint);
        match self.http.get(&url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }
}
