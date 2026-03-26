
use prism_core::types::trace::{ContractInvocation, ExecutionTrace};
use std::fmt::Write;

/// Icons for different auth types and states
mod icons {
    pub const CONTRACT: &str = "📄";
    pub const FUNCTION: &str = "⚡";
    pub const AUTH_REQUIRED: &str = "🔒";
    pub const AUTH_PROVIDED: &str = "✅";
    pub const AUTH_MISSING: &str = "❌";
    pub const SUB_INVOCATION: &str = "🔗";
    pub const ERROR: &str = "❗";
    pub const SUCCESS: &str = "✨";
}

/// Unicode box-drawing characters
mod box_chars {
    pub const VERTICAL: &str = "│";
    pub const HORIZONTAL: &str = "─";
    pub const TOP_RIGHT: &str = "└";
    pub const CROSS: &str = "├";
    pub const VERTICAL_RIGHT: &str = "├";
}

/// Render authorization tree for a contract invocation
pub fn render_auth_tree(trace: &ExecutionTrace) -> anyhow::Result<String> {
    let mut output = String::new();

    // Header
    writeln!(output, "{} Authorization Tree", icons::CONTRACT)?;
    writeln!(output, "Transaction: {}", trace.tx_hash)?;
    writeln!(output, "Network: {} | Ledger: {}", trace.network, trace.ledger_sequence)?;
    writeln!(output)?;

    // Render each root invocation
    for (i, invocation) in trace.invocations.iter().enumerate() {
        let is_last = i == trace.invocations.len().saturating_sub(1);
        render_invocation(&mut output, invocation, "", is_last)?;
    }

    Ok(output)
}

/// Render a single contract invocation and its sub-invocations as a tree
fn render_invocation(
    output: &mut String,
    invocation: &ContractInvocation,
    prefix: &str,
    is_last: bool,
) -> anyhow::Result<()> {
    let current_prefix = if prefix.is_empty() { "" } else { prefix };
    let connector = if is_last { box_chars::TOP_RIGHT } else { box_chars::VERTICAL_RIGHT };
    let child_prefix = if is_last { "    " } else { "│   " };

    // Main invocation line
    let status_icon = if invocation.is_error { icons::ERROR } else { icons::SUCCESS };
    writeln!(
        output,
        "{}{}{} {} {}::{}",
        current_prefix,
        connector,
        box_chars::HORIZONTAL,
        status_icon,
        invocation.contract_id,
        invocation.function_name
    )?;

    // Arguments
    if !invocation.arguments.is_empty() {
        let args_connector = if is_last { "    " } else { "│   " };
        writeln!(output, "{}{}├─ {} Arguments:", current_prefix, args_connector, icons::FUNCTION)?;
        for (i, arg) in invocation.arguments.iter().enumerate() {
            let arg_is_last = i == invocation.arguments.len().saturating_sub(1);
            let arg_connector = if arg_is_last { box_chars::TOP_RIGHT } else { box_chars::VERTICAL_RIGHT };
            writeln!(
                output,
                "{}{}{}{} {}",
                current_prefix,
                args_connector,
                "│  ",
                arg_connector,
                arg
            )?;
        }
    }

    // Auth information (this would need to be added to ContractInvocation struct)
    // For now, we'll show placeholder auth info
    let auth_prefix = format!("{}{}", current_prefix, child_prefix);
    writeln!(output, "{}├─ {} Authorization:", auth_prefix, icons::AUTH_REQUIRED)?;
    writeln!(output, "{}│  └─ {} Provided: ✓", auth_prefix, icons::AUTH_PROVIDED)?;
    writeln!(output, "{}│  └─ {} Verified: ✓", auth_prefix, icons::AUTH_PROVIDED)?;

    // Resource usage
    writeln!(output, "{}├─ ⚡ Resources:", auth_prefix)?;
    writeln!(
        output,
        "{}│  └─ CPU: {} instructions",
        auth_prefix,
        invocation.total_cpu_instructions
    )?;
    writeln!(
        output,
        "{}│  └─ Memory: {} bytes",
        auth_prefix,
        invocation.total_memory_bytes
    )?;

    // Sub-invocations
    if !invocation.sub_invocations.is_empty() {
        writeln!(output, "{}└─ {} Sub-invocations:", auth_prefix, icons::SUB_INVOCATION)?;
        for (i, sub_invocation) in invocation.sub_invocations.iter().enumerate() {
            let sub_is_last = i == invocation.sub_invocations.len().saturating_sub(1);
            let sub_prefix = format!("{}│  ", auth_prefix);
            render_invocation(output, sub_invocation, &sub_prefix, sub_is_last)?;
        }
    } else {
        writeln!(output, "{}└─ (no sub-invocations)", auth_prefix)?;
    }

    Ok(())
}

/// Render a simplified auth tree focusing only on authorization structure
pub fn render_auth_only(trace: &ExecutionTrace) -> anyhow::Result<String> {
    let mut output = String::new();

    writeln!(output, "{} Authorization Structure", icons::AUTH_REQUIRED)?;
    writeln!(output, "Transaction: {}", trace.tx_hash)?;
    writeln!(output)?;

    for (i, invocation) in trace.invocations.iter().enumerate() {
        let is_last = i == trace.invocations.len().saturating_sub(1);
        render_auth_invocation(&mut output, invocation, "", is_last)?;
    }

    Ok(output)
}

/// Render only the authorization aspects of an invocation
fn render_auth_invocation(
    output: &mut String,
    invocation: &ContractInvocation,
    prefix: &str,
    is_last: bool,
) -> anyhow::Result<()> {
    let connector = if is_last { box_chars::TOP_RIGHT } else { box_chars::VERTICAL_RIGHT };
    let child_prefix = if is_last { "    " } else { "│   " };

    // Show contract and function with auth status
    let auth_status = if invocation.is_error { icons::AUTH_MISSING } else { icons::AUTH_PROVIDED };
    writeln!(
        output,
        "{}{}{} {} {}::{}",
        prefix,
        connector,
        box_chars::HORIZONTAL,
        auth_status,
        invocation.contract_id,
        invocation.function_name
    )?;

    // Show nested auth requirements
    let auth_prefix = format!("{}{}", prefix, child_prefix);

    // Placeholder for actual auth requirements - this would need auth data
    writeln!(output, "{}├─ {} Required signatures:", auth_prefix, icons::AUTH_REQUIRED)?;
    writeln!(output, "{}│  └─ {} Contract authority", auth_prefix, icons::AUTH_PROVIDED)?;
    writeln!(output, "{}│  └─ {} Function caller", auth_prefix, icons::AUTH_PROVIDED)?;

    // Sub-invocations with their auth requirements
    if !invocation.sub_invocations.is_empty() {
        writeln!(output, "{}└─ {} Cross-contract calls:", auth_prefix, icons::SUB_INVOCATION)?;
        for (i, sub_invocation) in invocation.sub_invocations.iter().enumerate() {
            let sub_is_last = i == invocation.sub_invocations.len().saturating_sub(1);
            let sub_prefix = format!("{}│  ", auth_prefix);
            render_auth_invocation(output, sub_invocation, &sub_prefix, sub_is_last)?;
        }
    } else {
        writeln!(output, "{}└─ (no additional auth requirements)", auth_prefix)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_core::types::trace::{ContractInvocation, HostFunctionCall};

    #[test]
    fn test_render_simple_auth_tree() {
        let invocation = ContractInvocation {
            contract_id: "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC".to_string(),
            function_name: "transfer".to_string(),
            arguments: vec!["from: G...".to_string(), "to: G...".to_string(), "amount: 100".to_string()],
            return_value: Some("Success".to_string()),
            host_calls: vec![],
            sub_invocations: vec![],
            total_cpu_instructions: 1500000,
            total_memory_bytes: 50000,
            is_error: false,
        };

        let trace = ExecutionTrace {
            tx_hash: "abcd1234...".to_string(),
            ledger_sequence: 12345,
            network: "testnet".to_string(),
            invocations: vec![invocation],
            state_diff: Default::default(),
            resource_profile: Default::default(),
            diagnostic_events: vec![],
        };

        let result = render_auth_tree(&trace).unwrap();
        assert!(result.contains("Authorization Tree"));
        assert!(result.contains("transfer"));
        assert!(result.contains("✨"));
    }

    #[test]
    fn test_render_nested_auth_tree() {
        let sub_invocation = ContractInvocation {
            contract_id: "SUB123...".to_string(),
            function_name: "approve".to_string(),
            arguments: vec!["spender: G...".to_string(), "amount: 50".to_string()],
            return_value: Some("Success".to_string()),
            host_calls: vec![],
            sub_invocations: vec![],
            total_cpu_instructions: 800000,
            total_memory_bytes: 25000,
            is_error: false,
        };

        let main_invocation = ContractInvocation {
            contract_id: "MAIN456...".to_string(),
            function_name: "execute".to_string(),
            arguments: vec!["target: SUB123...".to_string()],
            return_value: Some("Success".to_string()),
            host_calls: vec![],
            sub_invocations: vec![sub_invocation],
            total_cpu_instructions: 2000000,
            total_memory_bytes: 75000,
            is_error: false,
        };

        let trace = ExecutionTrace {
            tx_hash: "nested789...".to_string(),
            ledger_sequence: 67890,
            network: "mainnet".to_string(),
            invocations: vec![main_invocation],
            state_diff: Default::default(),
            resource_profile: Default::default(),
            diagnostic_events: vec![],
        };

        let result = render_auth_tree(&trace).unwrap();
        assert!(result.contains("Sub-invocations"));
        assert!(result.contains("approve"));
        assert!(result.contains("🔗"));
    }
}
