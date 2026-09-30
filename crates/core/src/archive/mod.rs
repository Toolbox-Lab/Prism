use crate::error::{ArchiveErrorKind, PrismResult};
use crate::network::NetworkConfig;

pub struct ArchiveClient {
    #[allow(dead_code)]
    client: reqwest::Client,

    #[allow(dead_code)]
    archive_urls: Vec<String>,
}

#[derive(Debug)]
pub struct ArchiveCheckpoint {
    pub ledger_sequence: u32,

    pub ledger_header: Vec<u8>,

    pub transaction_set: Vec<u8>,

    pub transaction_results: Vec<u8>,
}

impl ArchiveClient {
    pub fn new(config: &NetworkConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            archive_urls: config.archive_urls.clone(),
        }
    }

    pub async fn fetch_checkpoint(&self, ledger_sequence: u32) -> PrismResult<ArchiveCheckpoint> {
        let checkpoint_seq = (ledger_sequence / 64) * 64;
        let _path = format_checkpoint_path(checkpoint_seq);
        let archive_count = self.archive_urls.len();
        let _ = &self.client;

        tracing::info!(
            archive_count,
            "Fetching archive checkpoint for ledger {checkpoint_seq}"
        );

        Err(ArchiveErrorKind::FetchFailed {
            file: format!("checkpoint-{checkpoint_seq}"),
            reason: "Archive fetch not yet implemented".to_string(),
        }
        .into())
    }

    pub async fn fetch_ledger_entry(
        &self,
        _ledger_sequence: u32,
        _key: &str,
    ) -> PrismResult<Vec<u8>> {
        Err(ArchiveErrorKind::FetchFailed {
            file: format!("ledger-entry-{_ledger_sequence}-{_key}"),
            reason: "Ledger entry fetch not yet implemented".to_string(),
        }
        .into())
    }
}

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
