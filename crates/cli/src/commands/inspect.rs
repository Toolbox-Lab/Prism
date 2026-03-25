//! `prism inspect` — Full transaction context inspection.

use clap::Args;
use prism_core::types::config::NetworkConfig;

#[derive(Args)]
pub struct InspectArgs {
    /// Transaction hash to inspect.
    pub tx_hash: String,

    /// Show detailed fee breakdown including bid vs charged values.
    #[arg(long)]
    pub fee_stats: bool,
}

pub async fn run(
    args: InspectArgs,
    network: &NetworkConfig,
    output_format: &str,
) -> anyhow::Result<()> {
    let spinner = indicatif::ProgressBar::new_spinner();
    spinner.set_message("Fetching and decoding transaction...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let report = prism_core::decode::decode_transaction(&args.tx_hash, network).await?;

    spinner.finish_and_clear();

    // Inspect shows the full context including decoded args, auth, resources, fees
    match output_format {
        "json" => crate::output::json::print_report(&report)?,
        _ => {
            crate::output::human::print_report(&report)?;
            if args.fee_stats {
                let fee_context = report.transaction_context.as_ref().map(|ctx| &ctx.fee);

                // Bid fee is not currently exposed in the decoded report pipeline.
                let bid_fee: Option<i64> = None;
                let resource_fee = fee_context.map(|fee| fee.resource_fee);
                let total_charged_fee = fee_context
                    .and_then(|fee| fee.inclusion_fee.checked_add(fee.resource_fee));
                let inclusion_fee = match (total_charged_fee, resource_fee) {
                    (Some(charged), Some(resource)) => charged.checked_sub(resource),
                    _ => None,
                };
                let surge = match (total_charged_fee, bid_fee) {
                    (Some(charged), Some(bid)) => Some(charged > bid),
                    _ => None,
                };

                let format_fee = |value: Option<i64>| match value {
                    Some(v) => format!("{v} stroops"),
                    None => "N/A".to_string(),
                };
                let format_surge = |value: Option<bool>| match value {
                    Some(true) => "Yes",
                    Some(false) => "No",
                    None => "N/A",
                };

                println!();
                println!("FEE BREAKDOWN");
                println!("Bid Fee: {}", format_fee(bid_fee));
                println!("Total Charged Fee: {}", format_fee(total_charged_fee));
                println!("Resource Fee: {}", format_fee(resource_fee));
                println!("Inclusion Fee: {}", format_fee(inclusion_fee));
                println!("Surge: {}", format_surge(surge));
            }
        }
    }

    Ok(())
}
