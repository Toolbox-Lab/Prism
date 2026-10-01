use crate::error::{ArchiveErrorKind, GratError, GratResult};
use crate::network::NetworkConfig;
use flate2::read::GzDecoder;
use std::io::Read;
use std::path::Path;
use stellar_xdr::curr::{LedgerHeader, LedgerHeaderHistoryEntry, Limits, ReadXdr};

#[derive(Debug)]
pub struct ArchiveClient {
    client: reqwest::Client,
    archive_urls: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveCheckpoint {
    pub ledger_sequence: u32,

    pub ledger_header: Vec<u8>,

    pub transaction_set: Vec<u8>,

    pub transaction_results: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveCategory {
    Ledger,
    Transactions,
    Results,
}

impl ArchiveCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ledger => "ledger",
            Self::Transactions => "transactions",
            Self::Results => "results",
        }
    }
}

pub fn format_archive_path(category: ArchiveCategory, checkpoint_seq: u32) -> String {
    format_archive_path_for(category, checkpoint_seq, CompressionFormat::Gzip)
}

fn format_archive_path_for(
    category: ArchiveCategory,
    checkpoint_seq: u32,
    compression: CompressionFormat,
) -> String {
    let hex = format!("{checkpoint_seq:08x}");
    let sub_dir = format!("{}/{}/{}", &hex[0..2], &hex[2..4], &hex[4..6]);
    let cat = category.as_str();
    format!("{cat}/{sub_dir}/{cat}-{hex}.xdr.{}", compression.extension())
}

fn join_url(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    let path = path.trim_start_matches('/');
    format!("{base}/{path}")
}

/// Compression format of a history archive file, inferred from its file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompressionFormat {
    Gzip,
    Xz,
}

impl CompressionFormat {
    /// Infers the compression format from a URL's file extension.
    ///
    /// Gzip is the historical Stellar history archive convention (`.xdr.gz`), so any URL
    /// that is not explicitly suffixed with `.xz` is treated as gzip.
    fn from_url(url: &str) -> Self {
        let path = url.split(['?', '#']).next().unwrap_or(url);
        match Path::new(path).extension().and_then(|ext| ext.to_str()) {
            Some("xz") => Self::Xz,
            _ => Self::Gzip,
        }
    }

    /// File extension used by this compression format in archive paths
    /// (i.e. `gz` for `ledger-0000003f.xdr.gz`).
    fn extension(self) -> &'static str {
        match self {
            Self::Gzip => "gz",
            Self::Xz => "xz",
        }
    }

    fn decompress(self, bytes: &[u8], file_name: &str) -> Result<Vec<u8>, ArchiveErrorKind> {
        let mut decompressed = Vec::new();

        let result = match self {
            Self::Gzip => GzDecoder::new(bytes).read_to_end(&mut decompressed),
            Self::Xz => xz2::read::XzDecoder::new(bytes).read_to_end(&mut decompressed),
        };

        result.map_err(|e| ArchiveErrorKind::DecompressionFailed {
            file: file_name.to_string(),
            reason: e.to_string(),
        })?;

        Ok(decompressed)
    }
}

async fn fetch_and_decompress(
    client: &reqwest::Client,
    url: &str,
    file_name: &str,
) -> Result<Vec<u8>, ArchiveErrorKind> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| ArchiveErrorKind::FetchFailed {
            file: file_name.to_string(),
            reason: e.to_string(),
        })?;

    if !response.status().is_success() {
        return Err(ArchiveErrorKind::FetchFailed {
            file: file_name.to_string(),
            reason: format!("HTTP status {}", response.status()),
        });
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| ArchiveErrorKind::FetchFailed {
            file: file_name.to_string(),
            reason: format!("failed to read response bytes: {e}"),
        })?;

    CompressionFormat::from_url(url).decompress(&bytes, file_name)
}

impl ArchiveClient {
    pub fn new(config: &NetworkConfig) -> GratResult<Self> {
        if config.request_timeout_secs == 0 {
            return Err(GratError::ConfigError(
                "request timeout must be greater than zero".to_string(),
            ));
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.request_timeout_secs))
            .build()
            .map_err(|e| GratError::ConfigError(format!("failed to build reqwest client: {e}")))?;

        Ok(Self {
            client,
            archive_urls: config.archive_urls.clone(),
        })
    }

    pub async fn fetch_checkpoint(&self, ledger_sequence: u32) -> GratResult<ArchiveCheckpoint> {
        let checkpoint_seq = get_checkpoint_seq(ledger_sequence);

        let mut last_error = None;

        for base_url in &self.archive_urls {
            // Stellar history archives historically serve gzip-compressed files (`.xdr.gz`),
            // but alternative hosts may use xz (`.xdr.xz`). Try each format for the whole
            // checkpoint so a host that only serves `.xz` still works, without mixing
            // compression formats or archive hosts within a single checkpoint.
            for compression in [CompressionFormat::Gzip, CompressionFormat::Xz] {
                let ledger_path =
                    format_archive_path_for(ArchiveCategory::Ledger, checkpoint_seq, compression);
                let tx_path = format_archive_path_for(
                    ArchiveCategory::Transactions,
                    checkpoint_seq,
                    compression,
                );
                let results_path =
                    format_archive_path_for(ArchiveCategory::Results, checkpoint_seq, compression);

                let ledger_url = join_url(base_url, &ledger_path);
                let tx_url = join_url(base_url, &tx_path);
                let results_url = join_url(base_url, &results_path);

                let ledger_header =
                    match fetch_and_decompress(&self.client, &ledger_url, &ledger_path).await {
                        Ok(bytes) => bytes,
                        Err(e) => {
                            last_error = Some(e);
                            continue;
                        }
                    };

                let transaction_set =
                    match fetch_and_decompress(&self.client, &tx_url, &tx_path).await {
                        Ok(bytes) => bytes,
                        Err(e) => {
                            last_error = Some(e);
                            continue;
                        }
                    };

                let transaction_results =
                    match fetch_and_decompress(&self.client, &results_url, &results_path).await {
                        Ok(bytes) => bytes,
                        Err(e) => {
                            last_error = Some(e);
                            continue;
                        }
                    };

                return Ok(ArchiveCheckpoint {
                    ledger_sequence: checkpoint_seq,
                    ledger_header,
                    transaction_set,
                    transaction_results,
                });
            }
        }

        let reason = match last_error {
            Some(err) => err.to_string(),
            None => "no archive URLs configured".to_string(),
        };

        Err(ArchiveErrorKind::FetchFailed {
            file: format!("checkpoint-{checkpoint_seq}"),
            reason,
        }
        .into())
    }

    pub async fn fetch_ledger_entry(
        &self,
        _ledger_sequence: u32,
        _key: &str,
    ) -> GratResult<Vec<u8>> {
        Err(ArchiveErrorKind::FetchFailed {
            file: format!("ledger-entry-{_ledger_sequence}-{_key}"),
            reason: "Ledger entry fetch not yet implemented".to_string(),
        }
        .into())
    }

    pub async fn get_ledger_header(&self, ledger_sequence: u32) -> GratResult<LedgerHeader> {
        let checkpoint = self.fetch_checkpoint(ledger_sequence).await?;
        let checkpoint_seq = get_checkpoint_seq(ledger_sequence);
        let checkpoint_file = format!("checkpoint-{}", checkpoint_seq);
        let entries = parse_ledger_header_stream(&checkpoint.ledger_header, &checkpoint_file)?;

        for entry in entries {
            if entry.header.ledger_seq == ledger_sequence {
                return Ok(entry.header);
            }
        }

        Err(ArchiveErrorKind::FetchFailed {
            file: checkpoint_file,
            reason: format!("ledger sequence {ledger_sequence} not found in checkpoint stream"),
        }
        .into())
    }
}

/// Parses a stream of XDR-encoded `LedgerHeaderHistoryEntry` records using RFC 5531 Record Marking format.
///
/// Each record fragment is preceded by a 4-byte header:
/// - Highest bit (0x8000_0000): 1 if this fragment is the final fragment of a record, 0 otherwise.
/// - Lower 31 bits (0x7FFF_FFFF): length of the fragment payload in bytes.
pub fn parse_ledger_header_stream(
    bytes: &[u8],
    file_name: &str,
) -> GratResult<Vec<LedgerHeaderHistoryEntry>> {
    let mut cursor = bytes;
    let mut entries = Vec::new();
    let mut record_buf = Vec::new();

    while !cursor.is_empty() {
        if cursor.len() < 4 {
            return Err(ArchiveErrorKind::MalformedXdr {
                file: file_name.to_string(),
                reason: format!(
                    "truncated record header: expected 4 bytes, found {}",
                    cursor.len()
                ),
            }
            .into());
        }

        let header = u32::from_be_bytes(cursor[..4].try_into().unwrap());
        cursor = &cursor[4..];

        let is_last = (header & 0x8000_0000) != 0;
        let fragment_len = (header & 0x7FFF_FFFF) as usize;

        if fragment_len > cursor.len() {
            return Err(ArchiveErrorKind::MalformedXdr {
                file: file_name.to_string(),
                reason: format!(
                    "truncated fragment payload: expected {} bytes, found {}",
                    fragment_len,
                    cursor.len()
                ),
            }
            .into());
        }

        let (fragment_bytes, rest) = cursor.split_at(fragment_len);
        cursor = rest;

        if is_last && record_buf.is_empty() {
            let entry = LedgerHeaderHistoryEntry::from_xdr(fragment_bytes, Limits::none())
                .map_err(|e| ArchiveErrorKind::MalformedXdr {
                    file: file_name.to_string(),
                    reason: format!("failed to decode LedgerHeaderHistoryEntry: {e}"),
                })?;
            entries.push(entry);
        } else {
            record_buf.extend_from_slice(fragment_bytes);

            if is_last {
                let entry = LedgerHeaderHistoryEntry::from_xdr(&record_buf, Limits::none())
                    .map_err(|e| ArchiveErrorKind::MalformedXdr {
                        file: file_name.to_string(),
                        reason: format!("failed to decode LedgerHeaderHistoryEntry: {e}"),
                    })?;
                entries.push(entry);
                record_buf.clear();
            }
        }
    }

    if !record_buf.is_empty() {
        return Err(ArchiveErrorKind::MalformedXdr {
            file: file_name.to_string(),
            reason: "unterminated record fragment stream".to_string(),
        }
        .into());
    }

    Ok(entries)
}

fn get_checkpoint_seq(ledger_sequence: u32) -> u32 {
    (ledger_sequence / 64) * 64 + 63
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::GratError;
    use std::sync::{Arc, Mutex};
    use stellar_xdr::curr::{Hash, LedgerHeaderExt, LedgerHeaderHistoryEntryExt, WriteXdr};

    fn make_test_entry(ledger_seq: u32) -> LedgerHeaderHistoryEntry {
        LedgerHeaderHistoryEntry {
            hash: Hash([0; 32]),
            header: LedgerHeader {
                ledger_version: 0,
                previous_ledger_hash: Hash([0; 32]),
                scp_value: stellar_xdr::curr::StellarValue {
                    tx_set_hash: Hash([0; 32]),
                    close_time: stellar_xdr::curr::TimePoint(0),
                    upgrades: vec![].try_into().unwrap(),
                    ext: stellar_xdr::curr::StellarValueExt::Basic,
                },
                tx_set_result_hash: Hash([0; 32]),
                bucket_list_hash: Hash([0; 32]),
                ledger_seq,
                total_coins: 0,
                fee_pool: 0,
                inflation_seq: 0,
                id_pool: 0,
                base_fee: 100,
                base_reserve: 100,
                max_tx_set_size: 100,
                skip_list: [Hash([0; 32]), Hash([0; 32]), Hash([0; 32]), Hash([0; 32])],
                ext: LedgerHeaderExt::V0,
            },
            ext: LedgerHeaderHistoryEntryExt::V0,
        }
    }

    fn frame_record(payload: &[u8], is_last: bool) -> Vec<u8> {
        let mut header_val = payload.len() as u32 & 0x7FFF_FFFF;
        if is_last {
            header_val |= 0x8000_0000;
        }
        let mut out = header_val.to_be_bytes().to_vec();
        out.extend_from_slice(payload);
        out
    }

    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn gzip_compress(data: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    fn xz_compress(data: &[u8]) -> Vec<u8> {
        let mut encoder = xz2::write::XzEncoder::new(Vec::new(), 6);
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    async fn start_recording_mock_server<F>(
        recorded_paths: Arc<Mutex<Vec<String>>>,
        handler: F,
    ) -> (String, tokio::task::JoinHandle<()>)
    where
        F: Fn(&str) -> (u16, Vec<u8>) + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = format!("http://127.0.0.1:{}", port);
        let handler = std::sync::Arc::new(handler);

        let handle = tokio::spawn(async move {
            loop {
                if let Ok((mut socket, _)) = listener.accept().await {
                    let handler = handler.clone();
                    let recorded_paths = recorded_paths.clone();
                    tokio::spawn(async move {
                        let mut buf = [0; 4096];
                        if let Ok(n) = socket.read(&mut buf).await {
                            if n == 0 {
                                return;
                            }
                            let request = String::from_utf8_lossy(&buf[..n]);
                            if let Some(line) = request.lines().next() {
                                if let Some(path) = line.split_whitespace().nth(1) {
                                    recorded_paths.lock().unwrap().push(path.to_string());
                                    let (status_code, body) = handler(path);
                                    let status_text = match status_code {
                                        200 => "200 OK",
                                        404 => "404 Not Found",
                                        500 => "500 Internal Server Error",
                                        _ => "500 Internal Server Error",
                                    };

                                    let response = format!(
                                        "HTTP/1.1 {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                        status_text,
                                        body.len()
                                    );
                                    let _ = socket.write_all(response.as_bytes()).await;
                                    let _ = socket.write_all(&body).await;
                                    let _ = socket.flush().await;
                                }
                            }
                        }
                    });
                }
            }
        });

        (url, handle)
    }

    async fn start_mock_archive_server(
        ledger_data: Vec<u8>,
        tx_data: Vec<u8>,
        res_data: Vec<u8>,
    ) -> (String, tokio::task::JoinHandle<()>) {
        start_recording_mock_server(Arc::new(Mutex::new(Vec::new())), move |path| {
            if path.contains("ledger-") {
                (200, ledger_data.clone())
            } else if path.contains("transactions-") {
                (200, tx_data.clone())
            } else if path.contains("results-") {
                (200, res_data.clone())
            } else {
                (404, Vec::new())
            }
        })
        .await
    }

    #[test]
    fn test_checkpoint_calculation() {
        assert_eq!(get_checkpoint_seq(0), 63);
        assert_eq!(get_checkpoint_seq(1), 63);
        assert_eq!(get_checkpoint_seq(62), 63);
        assert_eq!(get_checkpoint_seq(63), 63);
        assert_eq!(get_checkpoint_seq(64), 127);
        assert_eq!(get_checkpoint_seq(127), 127);
        assert_eq!(get_checkpoint_seq(128), 191);
    }

    #[test]
    fn test_archive_path_formatting() {
        // Test Category: Ledger
        assert_eq!(
            format_archive_path(ArchiveCategory::Ledger, 63),
            "ledger/00/00/00/ledger-0000003f.xdr.gz"
        );
        assert_eq!(
            format_archive_path(ArchiveCategory::Ledger, 0),
            "ledger/00/00/00/ledger-00000000.xdr.gz"
        );
        assert_eq!(
            format_archive_path(ArchiveCategory::Ledger, 64),
            "ledger/00/00/00/ledger-00000040.xdr.gz"
        );
        assert_eq!(
            format_archive_path(ArchiveCategory::Ledger, 127),
            "ledger/00/00/00/ledger-0000007f.xdr.gz"
        );
        // Test a larger sequence
        assert_eq!(
            format_archive_path(ArchiveCategory::Ledger, 65535),
            "ledger/00/00/ff/ledger-0000ffff.xdr.gz"
        );

        // Test other categories
        assert_eq!(
            format_archive_path(ArchiveCategory::Transactions, 63),
            "transactions/00/00/00/transactions-0000003f.xdr.gz"
        );
        assert_eq!(
            format_archive_path(ArchiveCategory::Results, 63),
            "results/00/00/00/results-0000003f.xdr.gz"
        );
    }

    #[test]
    fn test_archive_path_formatting_with_compression() {
        assert_eq!(
            format_archive_path_for(ArchiveCategory::Ledger, 63, CompressionFormat::Xz),
            "ledger/00/00/00/ledger-0000003f.xdr.xz"
        );
        assert_eq!(
            format_archive_path_for(ArchiveCategory::Transactions, 65535, CompressionFormat::Gzip),
            "transactions/00/00/ff/transactions-0000ffff.xdr.gz"
        );
        assert_eq!(
            format_archive_path_for(ArchiveCategory::Results, 127, CompressionFormat::Xz),
            "results/00/00/00/results-0000007f.xdr.xz"
        );
        // The public helper keeps emitting the historical gzip paths.
        assert_eq!(
            format_archive_path(ArchiveCategory::Ledger, 63),
            format_archive_path_for(ArchiveCategory::Ledger, 63, CompressionFormat::Gzip)
        );
    }

    #[test]
    fn test_compression_format_detection() {
        assert_eq!(
            CompressionFormat::from_url("http://host/ledger/00/00/00/ledger-0000003f.xdr.xz"),
            CompressionFormat::Xz
        );
        assert_eq!(
            CompressionFormat::from_url("http://host/ledger/00/00/00/ledger-0000003f.xdr.gz"),
            CompressionFormat::Gzip
        );
        // Query strings and fragments should not confuse extension detection
        assert_eq!(
            CompressionFormat::from_url("http://host/ledger-0000003f.xdr.xz?token=abc#frag"),
            CompressionFormat::Xz
        );
        // Unrecognized / missing extensions default to gzip
        assert_eq!(
            CompressionFormat::from_url("http://host/ledger-0000003f.xdr"),
            CompressionFormat::Gzip
        );
        assert_eq!(
            CompressionFormat::from_url("http://host/ledger-0000003f"),
            CompressionFormat::Gzip
        );
    }

    #[test]
    fn test_decompress_gzip_payload() {
        let data = b"gzip payload";
        let compressed = gzip_compress(data);
        let decompressed = CompressionFormat::Gzip.decompress(&compressed, "test.xdr.gz").unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_decompress_xz_payload() {
        let data = b"xz payload";
        let compressed = xz_compress(data);
        let decompressed = CompressionFormat::Xz.decompress(&compressed, "test.xdr.xz").unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_decompress_invalid_xz_fails() {
        let err = CompressionFormat::Xz
            .decompress(b"definitely not xz", "test.xdr.xz")
            .unwrap_err();
        assert!(matches!(
            err,
            ArchiveErrorKind::DecompressionFailed { .. }
        ));
    }

    #[tokio::test]
    async fn test_fetch_and_decompress_xz_over_http() {
        let data = b"xz over http";
        let compressed = xz_compress(data);
        let (url, _handle) =
            start_recording_mock_server(Arc::new(Mutex::new(Vec::new())), move |_path| {
                (200, compressed.clone())
            })
            .await;

        let client = reqwest::Client::new();
        let url_xz = format!("{}/ledger/00/00/00/ledger-0000003f.xdr.xz", url);
        let decompressed = fetch_and_decompress(&client, &url_xz, "ledger-0000003f.xdr.xz")
            .await
            .unwrap();
        assert_eq!(decompressed, data);
    }

    #[tokio::test]
    async fn test_fetch_checkpoint_xz_fallback() {
        // Host only serves xz payloads; the gzip attempt must fall through to xz.
        let paths = Arc::new(Mutex::new(Vec::new()));
        let ledger_xz = xz_compress(b"xz_ledger");
        let tx_xz = xz_compress(b"xz_tx");
        let res_xz = xz_compress(b"xz_results");

        let (url, _handle) = start_recording_mock_server(paths.clone(), move |path| {
            if !path.ends_with(".xz") {
                return (404, Vec::new());
            }
            if path.contains("ledger-") {
                (200, ledger_xz.clone())
            } else if path.contains("transactions-") {
                (200, tx_xz.clone())
            } else if path.contains("results-") {
                (200, res_xz.clone())
            } else {
                (404, Vec::new())
            }
        })
        .await;

        let config = NetworkConfig::custom("test", "", "").with_archive_urls(vec![url]);
        let client = ArchiveClient::new(&config).unwrap();

        let checkpoint = client.fetch_checkpoint(64).await.unwrap();
        assert_eq!(checkpoint.ledger_sequence, 127);
        assert_eq!(checkpoint.ledger_header, b"xz_ledger");
        assert_eq!(checkpoint.transaction_set, b"xz_tx");
        assert_eq!(checkpoint.transaction_results, b"xz_results");

        // Ensure xz files were actually requested after the gzip 404s.
        let recorded = paths.lock().unwrap();
        assert!(recorded.iter().any(|p| p.ends_with(".xdr.xz")));
    }

    #[tokio::test]
    async fn test_url_joining_normalization() {
        let paths = Arc::new(Mutex::new(Vec::new()));
        let ledger_gz = gzip_compress(b"ledger");
        let tx_gz = gzip_compress(b"tx");
        let results_gz = gzip_compress(b"results");

        let l_gz = ledger_gz.clone();
        let t_gz = tx_gz.clone();
        let r_gz = results_gz.clone();

        let (url_raw, _handle) = start_recording_mock_server(paths.clone(), move |path| {
            if path.contains("ledger-") {
                (200, l_gz.clone())
            } else if path.contains("transactions-") {
                (200, t_gz.clone())
            } else if path.contains("results-") {
                (200, r_gz.clone())
            } else {
                (404, Vec::new())
            }
        })
        .await;

        // Test with trailing slash
        let url_with_slash = format!("{}/", url_raw);
        let config = NetworkConfig::custom("test", "", "").with_archive_urls(vec![url_with_slash]);
        let client = ArchiveClient::new(&config).unwrap();
        let checkpoint = client.fetch_checkpoint(64).await.unwrap();
        assert_eq!(checkpoint.ledger_header, b"ledger");

        // Test without trailing slash
        let config_no_slash =
            NetworkConfig::custom("test", "", "").with_archive_urls(vec![url_raw]);
        let client_no_slash = ArchiveClient::new(&config_no_slash).unwrap();
        let checkpoint_no_slash = client_no_slash.fetch_checkpoint(64).await.unwrap();
        assert_eq!(checkpoint_no_slash.ledger_header, b"ledger");

        // Verify no double slashes in paths
        let recorded = paths.lock().unwrap();
        for path in recorded.iter() {
            assert!(!path.contains("//"), "Path contains double slash: {}", path);
        }
    }

    #[tokio::test]
    async fn test_fetch_checkpoint_success() {
        let ledger_data = b"my_ledger_payload";
        let tx_data = b"my_tx_payload";
        let res_data = b"my_res_payload";

        let ledger_gz = gzip_compress(ledger_data);
        let tx_gz = gzip_compress(tx_data);
        let res_gz = gzip_compress(res_data);

        let (url, _handle) = start_mock_archive_server(ledger_gz, tx_gz, res_gz).await;
        let config = NetworkConfig::custom("test", "", "").with_archive_urls(vec![url]);
        let client = ArchiveClient::new(&config).unwrap();

        let checkpoint = client.fetch_checkpoint(64).await.unwrap();
        assert_eq!(checkpoint.ledger_sequence, 127);
        assert_eq!(checkpoint.ledger_header, ledger_data);
        assert_eq!(checkpoint.transaction_set, tx_data);
        assert_eq!(checkpoint.transaction_results, res_data);
    }

    #[tokio::test]
    async fn test_fetch_checkpoint_failover_non_2xx() {
        let paths = Arc::new(Mutex::new(Vec::new()));
        // First server returns 404 for results
        let (url1, _h1) = start_recording_mock_server(paths.clone(), move |path| {
            if path.contains("ledger-") {
                (200, gzip_compress(b"l1"))
            } else if path.contains("transactions-") {
                (200, gzip_compress(b"tx1"))
            } else {
                (404, Vec::new()) // Fail results
            }
        })
        .await;

        // Second server succeeds
        let (url2, _h2) = start_recording_mock_server(paths.clone(), move |path| {
            if path.contains("ledger-") {
                (200, gzip_compress(b"l2"))
            } else if path.contains("transactions-") {
                (200, gzip_compress(b"tx2"))
            } else if path.contains("results-") {
                (200, gzip_compress(b"res2"))
            } else {
                (404, Vec::new())
            }
        })
        .await;

        let config = NetworkConfig::custom("test", "", "").with_archive_urls(vec![url1, url2]);
        let client = ArchiveClient::new(&config).unwrap();

        let checkpoint = client.fetch_checkpoint(64).await.unwrap();
        // Should succeed using second archive URL
        assert_eq!(checkpoint.ledger_header, b"l2");
        assert_eq!(checkpoint.transaction_set, b"tx2");
        assert_eq!(checkpoint.transaction_results, b"res2");
    }

    #[tokio::test]
    async fn test_fetch_checkpoint_failover_network_failure() {
        // First archive URL is completely dead/unreachable
        let dead_url = "http://127.0.0.1:1".to_string();

        // Second server succeeds
        let (url2, _h2) = start_mock_archive_server(
            gzip_compress(b"l"),
            gzip_compress(b"tx"),
            gzip_compress(b"res"),
        )
        .await;

        let config = NetworkConfig::custom("test", "", "").with_archive_urls(vec![dead_url, url2]);
        let client = ArchiveClient::new(&config).unwrap();

        let checkpoint = client.fetch_checkpoint(64).await.unwrap();
        assert_eq!(checkpoint.ledger_header, b"l");
    }

    #[tokio::test]
    async fn test_fetch_checkpoint_failover_invalid_gzip() {
        // First server returns raw uncompressed bytes (invalid gzip)
        let (url1, _h1) = start_mock_archive_server(
            b"not_gzip_at_all".to_vec(),
            gzip_compress(b"tx1"),
            gzip_compress(b"res1"),
        )
        .await;

        // Second server succeeds
        let (url2, _h2) = start_mock_archive_server(
            gzip_compress(b"l2"),
            gzip_compress(b"tx2"),
            gzip_compress(b"res2"),
        )
        .await;

        let config = NetworkConfig::custom("test", "", "").with_archive_urls(vec![url1, url2]);
        let client = ArchiveClient::new(&config).unwrap();

        let checkpoint = client.fetch_checkpoint(64).await.unwrap();
        assert_eq!(checkpoint.ledger_header, b"l2");
    }

    #[tokio::test]
    async fn test_fetch_checkpoint_all_archives_fail() {
        let (url1, _h1) = start_mock_archive_server(
            gzip_compress(b"l1"),
            gzip_compress(b"tx1"),
            b"invalid".to_vec(), // fail results
        )
        .await;

        let (url2, _h2) = start_mock_archive_server(
            b"invalid".to_vec(), // fail ledger
            gzip_compress(b"tx2"),
            gzip_compress(b"res2"),
        )
        .await;

        let config = NetworkConfig::custom("test", "", "").with_archive_urls(vec![url1, url2]);
        let client = ArchiveClient::new(&config).unwrap();

        let err = client.fetch_checkpoint(64).await.unwrap_err();
        if let GratError::ArchiveError(ArchiveErrorKind::FetchFailed { reason, .. }) = err {
            // Confirm it matches FetchFailed
            assert!(
                reason.contains("DecompressionFailed")
                    || reason.contains("decompression failed")
                    || reason.contains("HTTP status")
                    || reason.contains("failed to fetch")
            );
        } else {
            panic!("Expected FetchFailed archive error, got: {:?}", err);
        }
    }

    #[tokio::test]
    async fn test_fetch_checkpoint_no_partial() {
        // Check that we never combine responses from different archives
        let (url1, _h1) = start_mock_archive_server(
            gzip_compress(b"l1"),
            gzip_compress(b"tx1"),
            b"invalid".to_vec(), // fail results
        )
        .await;

        let (url2, _h2) = start_mock_archive_server(
            b"invalid".to_vec(), // fail ledger
            gzip_compress(b"tx2"),
            gzip_compress(b"res2"),
        )
        .await;

        let config = NetworkConfig::custom("test", "", "").with_archive_urls(vec![url1, url2]);
        let client = ArchiveClient::new(&config).unwrap();

        // Fetch should fail completely rather than returning a partial checkpoint
        let err = client.fetch_checkpoint(64).await.unwrap_err();
        assert!(matches!(
            err,
            GratError::ArchiveError(ArchiveErrorKind::FetchFailed { .. })
        ));
    }

    #[test]
    fn test_single_record() {
        let entry = make_test_entry(100);
        let xdr_bytes = entry.to_xdr(Limits::none()).unwrap();
        let framed = frame_record(&xdr_bytes, true);

        let parsed = parse_ledger_header_stream(&framed, "test.xdr").unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].header.ledger_seq, 100);
    }

    #[test]
    fn test_multiple_records_and_boundaries() {
        let seqs = vec![64, 65, 66, 67, 127];
        let mut stream = Vec::new();
        for &seq in &seqs {
            let entry = make_test_entry(seq);
            let xdr_bytes = entry.to_xdr(Limits::none()).unwrap();
            stream.extend(frame_record(&xdr_bytes, true));
        }

        let parsed = parse_ledger_header_stream(&stream, "test.xdr").unwrap();
        assert_eq!(parsed.len(), 5);

        // First boundary
        assert_eq!(parsed.first().unwrap().header.ledger_seq, 64);
        // Middle entry
        assert_eq!(parsed[2].header.ledger_seq, 66);
        // Last boundary
        assert_eq!(parsed.last().unwrap().header.ledger_seq, 127);
    }

    #[test]
    fn test_multi_fragment_record() {
        let entry = make_test_entry(200);
        let xdr_bytes = entry.to_xdr(Limits::none()).unwrap();
        assert!(xdr_bytes.len() > 4);

        let mid = xdr_bytes.len() / 2;
        let mut stream = Vec::new();
        stream.extend(frame_record(&xdr_bytes[..mid], false));
        stream.extend(frame_record(&xdr_bytes[mid..], true));

        let parsed = parse_ledger_header_stream(&stream, "test.xdr").unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].header.ledger_seq, 200);
    }

    #[test]
    fn test_ledger_not_found_in_stream() {
        let entry = make_test_entry(100);
        let xdr_bytes = entry.to_xdr(Limits::none()).unwrap();
        let framed = frame_record(&xdr_bytes, true);

        let parsed = parse_ledger_header_stream(&framed, "test.xdr").unwrap();
        let found = parsed.iter().find(|e| e.header.ledger_seq == 101);
        assert!(found.is_none());
    }

    #[test]
    fn test_truncated_frame_header() {
        let bytes = vec![0x80, 0x00, 0x00]; // 3 bytes instead of 4
        let err = parse_ledger_header_stream(&bytes, "test.xdr").unwrap_err();

        if let GratError::ArchiveError(ArchiveErrorKind::MalformedXdr { reason, .. }) = err {
            assert!(reason.contains("truncated record header"));
        } else {
            panic!("expected MalformedXdr error, got {:?}", err);
        }
    }

    #[test]
    fn test_truncated_frame_payload() {
        let payload = vec![1, 2, 3];
        // Frame header declares length 10, but only 3 payload bytes provided
        let mut framed = 10u32.to_be_bytes().to_vec();
        framed[0] |= 0x80; // mark as last
        framed.extend_from_slice(&payload);

        let err = parse_ledger_header_stream(&framed, "test.xdr").unwrap_err();
        if let GratError::ArchiveError(ArchiveErrorKind::MalformedXdr { reason, .. }) = err {
            assert!(reason.contains("truncated fragment payload"));
        } else {
            panic!("expected MalformedXdr error, got {:?}", err);
        }
    }

    #[test]
    fn test_unterminated_fragment_stream() {
        let payload = vec![1, 2, 3, 4];
        // Frame header declares length 4, but is_last = false
        let framed = frame_record(&payload, false);

        let err = parse_ledger_header_stream(&framed, "test.xdr").unwrap_err();
        if let GratError::ArchiveError(ArchiveErrorKind::MalformedXdr { reason, .. }) = err {
            assert!(reason.contains("unterminated record fragment stream"));
        } else {
            panic!("expected MalformedXdr error, got {:?}", err);
        }
    }

    #[test]
    fn test_invalid_xdr_payload() {
        let invalid_xdr = vec![0xFF; 20];
        let framed = frame_record(&invalid_xdr, true);

        let err = parse_ledger_header_stream(&framed, "test.xdr").unwrap_err();
        if let GratError::ArchiveError(ArchiveErrorKind::MalformedXdr { reason, .. }) = err {
            assert!(reason.contains("failed to decode LedgerHeaderHistoryEntry"));
        } else {
            panic!("expected MalformedXdr error, got {:?}", err);
        }
    }

    #[tokio::test]
    async fn test_get_ledger_header_integration() {
        let entry = make_test_entry(64);
        let xdr_bytes = entry.to_xdr(Limits::none()).unwrap();
        let framed = frame_record(&xdr_bytes, true);

        let ledger_gz = gzip_compress(&framed);
        let dummy_gz = gzip_compress(&[0; 4]);

        let (url, _handle) = start_mock_archive_server(ledger_gz, dummy_gz.clone(), dummy_gz).await;
        let config = NetworkConfig::custom("test", "", "").with_archive_urls(vec![url]);
        let client = ArchiveClient::new(&config).unwrap();

        let header = client.get_ledger_header(64).await.unwrap();
        assert_eq!(header.ledger_seq, 64);
    }

    #[test]
    fn test_archive_client_zero_timeout_error() {
        let config = NetworkConfig::custom("test", "", "")
            .with_archive_urls(vec!["http://example.com".to_string()]);
        let mut config_zero = config.clone();
        config_zero.request_timeout_secs = 0;
        let err = ArchiveClient::new(&config_zero).unwrap_err();
        assert!(matches!(
            err,
            GratError::ConfigError(ref msg) if msg.contains("timeout")
        ));
    }
}
