use clap::Args;
use grat_core::network::NetworkConfig;

/// Arguments for `grat replay`.
#[derive(Args, Debug)]
pub struct ReplayArgs {
    /// Transaction hash whose recorded invocation tree is replayed.
    #[arg(index = 1, value_name = "TX_HASH")]
    pub tx_hash: String,

    /// Open the full-terminal debugger instead of printing a status hint.
    #[arg(long, short = 'i')]
    pub interactive: bool,
}

pub async fn run(
    args: ReplayArgs,
    network: &NetworkConfig,
    output_format: &str,
    quiet: &bool,
) -> anyhow::Result<()> {
    let json_output = matches!(
        crate::output::OutputFormat::parse(output_format),
        crate::output::OutputFormat::Json
    );

    if args.interactive {
        if json_output {
            print_json(&status_payload(&args.tx_hash, true))?;
            return Ok(());
        }

        if !*quiet {
            println!("Launching interactive TUI debugger for {}...", args.tx_hash);
        }
        crate::tui::app::launch(&args.tx_hash, network).await?;
    } else {
        if json_output {
            print_json(&status_payload(&args.tx_hash, false))?;
            return Ok(());
        }

        if !*quiet {
            println!("Use --interactive / -i to launch the TUI debugger.");
            println!(
                "Or use `grat trace {}` for non-interactive trace output.",
                args.tx_hash
            );
        }
    }

    Ok(())
}

fn status_payload(tx_hash: &str, interactive: bool) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "tx_hash": tx_hash,
        "interactive": interactive,
    });
    if interactive {
        payload["status"] = serde_json::json!("launching_tui");
    } else {
        payload["status"] = serde_json::json!("interactive_mode_required");
        payload["hint"] = serde_json::json!("Use --interactive / -i to launch the TUI debugger.");
    }
    payload
}

fn print_json(payload: &serde_json::Value) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(payload)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct ReplayOnly {
        #[command(flatten)]
        replay: ReplayArgs,
    }

    #[test]
    fn replay_accepts_a_positional_hash_and_short_interactive_flag() {
        let parsed = ReplayOnly::try_parse_from(["grat", "abc123", "-i"]).unwrap();
        assert_eq!(parsed.replay.tx_hash, "abc123");
        assert!(parsed.replay.interactive);
    }

    #[test]
    fn json_status_hints_describe_each_mode() {
        let interactive = status_payload("abc123", true);
        assert_eq!(interactive["tx_hash"], "abc123");
        assert_eq!(interactive["interactive"], true);
        assert_eq!(interactive["status"], "launching_tui");

        let hint = status_payload("abc123", false);
        assert_eq!(hint["status"], "interactive_mode_required");
        assert!(hint["hint"]
            .as_str()
            .is_some_and(|hint| hint.contains("--interactive / -i")));
    }
}
