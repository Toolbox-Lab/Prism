use crate::ui::error_summary::ErrorSummaryList;
use clap::Args;
use futures_util::StreamExt;
use grat_core::types::config::NetworkConfig;

#[derive(Args)]
pub struct BatchArgs {
    #[arg(required = true, num_args = 1..)]
    pub tx_hashes: Vec<String>,
}

pub async fn run(args: BatchArgs, network: &NetworkConfig) -> anyhow::Result<()> {
    let mut completed_results = Vec::new();

    let spinner = indicatif::ProgressBar::new_spinner();
    spinner.set_message(format!("Decoding {} transactions...", args.tx_hashes.len()));
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let mut decodes = futures_util::stream::iter(args.tx_hashes.iter().enumerate().map(
        |(index, hash)| async move {
            let result =
                grat_core::decode::decode_transaction_with_op_filter(hash, network, None).await;
            (index, hash.clone(), result)
        },
    ))
    .buffer_unordered(args.tx_hashes.len().max(1));

    let mut completed = 0;
    while let Some((index, hash, result)) = decodes.next().await {
        completed += 1;
        spinner.set_message(format!(
            "Decoding transactions... ({}/{})",
            completed,
            args.tx_hashes.len()
        ));

        match result {
            Ok(reports) => completed_results.push((index, hash, reports)),
            Err(error) => eprintln!("Failed to decode transaction {hash}: {error}"),
        }
    }

    spinner.finish_and_clear();

    completed_results.sort_by_key(|(index, _, _)| *index);
    let batch_results = completed_results
        .into_iter()
        .map(|(_, hash, reports)| (hash, reports))
        .collect::<Vec<_>>();

    ErrorSummaryList::render(&batch_results);

    Ok(())
}
