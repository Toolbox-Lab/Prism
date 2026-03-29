//! Stellar History Archive client.
//!
//! Fetches and decompresses history archive files for cold-path state reconstruction.
//! Supports S3/GCS/HTTP backends. Used only for older transactions outside the RPC hot path.

use crate::types::config::NetworkConfig;
use crate::types::error::{PrismError, PrismResult};

/// Client for accessing Stellar History Archives.
pub struct ArchiveClient {
    /// HTTP client.
    #[allow(dead_code)]
    client: reqwest::Client,
    /// Archive base URLs.
    #[allow(dead_code)]
    archive_urls: Vec<String>,
}

/// A fetched history archive checkpoint.
#[derive(Debug)]
pub struct ArchiveCheckpoint {
    /// The ledger sequence number of the checkpoint.
    pub ledger_sequence: u32,
    /// Raw ledger header data.
    pub ledger_header: Vec<u8>,
    /// Raw transaction set data.
    pub transaction_set: Vec<u8>,
    /// Raw transaction result data.
    pub transaction_results: Vec<u8>,
}

impl ArchiveClient {
    /// Create a new archive client from network configuration.
    pub fn new(config: &NetworkConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            archive_urls: config.archive_urls.clone(),
        }
    }

    /// Fetch the history archive checkpoint containing the given ledger sequence.
    ///
    /// Checkpoints are stored every 64 ledgers. This method computes the correct
    /// checkpoint sequence and fetches the corresponding archive files.
    pub async fn fetch_checkpoint(&self, ledger_sequence: u32) -> PrismResult<ArchiveCheckpoint> {
        let checkpoint_seq = (ledger_sequence / 64) * 64;
        let _path = format_checkpoint_path(checkpoint_seq);
        let archive_count = self.archive_urls.len();
        let _client = &self.client;

        // TODO: Fetch and decompress archive files (gzip/xz)
        // TODO: Try each archive URL in order until one succeeds

        tracing::info!(
            archive_count,
            "Fetching archive checkpoint for ledger {checkpoint_seq}"
        );

        Err(PrismError::ArchiveError(
            "Archive fetch not yet implemented".to_string(),
        ))
    }

    /// Fetch a specific ledger entry from the history archives.
    pub async fn fetch_ledger_entry(
        &self,
        _ledger_sequence: u32,
        _key: &str,
    ) -> PrismResult<Vec<u8>> {
        Err(PrismError::ArchiveError(
            "Ledger entry fetch not yet implemented".to_string(),
        ))
    }
}

/// Format the checkpoint file path for the history archive directory structure.
fn format_checkpoint_path(checkpoint_seq: u32) -> String {
    let hex = format!("{checkpoint_seq:08x}");
    format!(
        "{}/{}/{}/ledger-{}.xdr.gz",
        &hex[0..2],
        &hex[2..4],
        &hex[4..6],
        hex
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_path_format() {
        let path = format_checkpoint_path(64);
        assert!(path.contains("ledger-"));
        assert!(path.ends_with(".xdr.gz"));
    }
}
