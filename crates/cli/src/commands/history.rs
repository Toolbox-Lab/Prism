use std::{
    env,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

const HISTORY_DIR: &str = ".grat";
const HISTORY_FILE: &str = "history.json";
const MAX_ENTRIES: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HistoryEntry {
    tx_hash: String,
    timestamp: u64,
}

pub async fn run(_output_format: &str) -> anyhow::Result<()> {
    let entries = load_history()?;
    if entries.is_empty() {
        println!("No transaction history found.");
        return Ok(());
    }

    println!("{:<4} {:<66} {}", "#", "Transaction Hash", "Decoded");
    println!("{}", "-".repeat(100));
    for (i, entry) in entries.iter().enumerate() {
        println!("{:<4} {:<66} {}", i + 1, entry.tx_hash, relative_time(entry.timestamp));
    }
    Ok(())
}

pub fn append_to_history(tx_hash: &str) -> anyhow::Result<()> {
    let mut entries = load_history()?;
    // Remove any existing entry with the same hash to keep unique.
    entries.retain(|e| e.tx_hash != tx_hash);
    entries.insert(
        0,
        HistoryEntry {
            tx_hash: tx_hash.to_string(),
            timestamp: now_unix(),
        },
    );
    entries.truncate(MAX_ENTRIES);
    save_history(&entries)
}

fn history_path() -> anyhow::Result<PathBuf> {
    let home = if let Some(home) = env::var_os("HOME") {
        PathBuf::from(home)
    } else if let Some(profile) = env::var_os("USERPROFILE") {
        PathBuf::from(profile)
    } else {
        PathBuf::from(".")
    };
    Ok(home.join(HISTORY_DIR).join(HISTORY_FILE))
}

fn load_history() -> anyhow::Result<Vec<HistoryEntry>> {
    let path = history_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents = fs::read_to_string(&path)?;
    let entries = serde_json::from_str(&contents)?;
    Ok(entries)
}

fn save_history(entries: &[HistoryEntry]) -> anyhow::Result<()> {
    let path = history_path()?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(entries)?;
    fs::write(path, json)?;
    Ok(())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn relative_time(timestamp: u64) -> String {
    let now = now_unix();
    let diff = now.saturating_sub(timestamp);
    if diff < 60 {
        format!("{diff} seconds ago")
    } else if diff < 3600 {
        format!("{} minutes ago", diff / 60)
    } else if diff < 86400 {
        format!("{} hours ago", diff / 3600)
    } else {
        format!("{} days ago", diff / 86400)
    }
}
