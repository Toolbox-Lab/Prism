use crate::ui::error_summary::ErrorSummaryList;
use clap::Args;
use grat_core::types::config::NetworkConfig;

#[derive(Args)]
pub struct BatchArgs {
    #[arg(required = true, num_args = 1..)]
    pub tx_hashes: Vec<String>,
}

pub async fn run(args: BatchArgs, network: &NetworkConfig) -> anyhow::Result<()> {
    let mut all_reports = Vec::new();

    let spinner = indicatif::ProgressBar::new_spinner();
    spinner.set_message(format!("Decoding {} transactions...", args.tx_hashes.len()));
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    for hash in &args.tx_hashes {
        if let Ok(reports) =
            grat_core::decode::decode_transaction_with_op_filter(hash, network, None).await
        {
            all_reports.extend(reports);
        }
    }

    spinner.finish_and_clear();

    ErrorSummaryList::render(&all_reports);

    Ok(())
}
