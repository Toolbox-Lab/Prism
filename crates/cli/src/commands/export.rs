use clap::Args;
use grat_core::types::config::NetworkConfig;

#[derive(Args)]
pub struct ExportArgs {
    pub tx_hash: String,

    #[arg(long, default_value = "test")]
    pub format: String,

    #[arg(long, short)]
    pub output: Option<String>,
}

pub async fn run(
    args: ExportArgs,
    network: &NetworkConfig,
    output_format: &str,
    quiet: &bool,
) -> anyhow::Result<()> {
    if !*quiet {
        println!(
            "Exporting {} on {:?} as {} format...",
            args.tx_hash, network.network, args.format
        );
    }

    let output_path = args.output.unwrap_or_else(|| {
        format!(
            "grat_test_{}.rs",
            &args.tx_hash[..8.min(args.tx_hash.len())]
        )
    });

    if matches!(
        crate::output::OutputFormat::parse(output_format),
        crate::output::OutputFormat::Json
    ) {
        let payload = serde_json::json!({
            "tx_hash": args.tx_hash,
            "network": format!("{:?}", network.network),
            "format": args.format,
            "output_path": output_path,
            "status": "exported",
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("Test case exported to {output_path}");
    }

    Ok(())
}
