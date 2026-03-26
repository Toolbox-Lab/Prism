//! TraceTree terminal renderer for visualizing contract call stacks.

use prism_core::types::trace::{ ContractInvocation, ExecutionTrace };
use std::io::Write;
use termcolor::WriteColor;

/// Unicode tree characters for rendering
mod tree_chars {
    pub const VERTICAL: &str = "│";
    pub const BRANCH: &str = "├──";
    pub const CORNER: &str = "└──";
    pub const SPACE: &str = "  ";
    pub const CONTINUE: &str = "│  ";
}

/// Colors for terminal output
#[allow(dead_code)]
mod colors {
    use termcolor::{ Color, ColorSpec, WriteColor };

    pub fn contract_id<W: WriteColor>(w: &mut W) {
        let mut spec = ColorSpec::new();
        spec.set_fg(Some(Color::Cyan)).set_bold(true);
        w.set_color(&spec).unwrap();
    }

    pub fn function_name<W: WriteColor>(w: &mut W) {
        let mut spec = ColorSpec::new();
        spec.set_fg(Some(Color::Green)).set_bold(true);
        w.set_color(&spec).unwrap();
    }

    pub fn error<W: WriteColor>(w: &mut W) {
        let mut spec = ColorSpec::new();
        spec.set_fg(Some(Color::Red)).set_bold(true);
        w.set_color(&spec).unwrap();
    }

    pub fn success<W: WriteColor>(w: &mut W) {
        let mut spec = ColorSpec::new();
        spec.set_fg(Some(Color::Green));
        w.set_color(&spec).unwrap();
    }

    pub fn warning<W: WriteColor>(w: &mut W) {
        let mut spec = ColorSpec::new();
        spec.set_fg(Some(Color::Yellow));
        w.set_color(&spec).unwrap();
    }

    pub fn reset<W: WriteColor>(w: &mut W) {
        w.reset().unwrap();
    }
}

/// Render a contract invocation tree
pub fn render_contract_tree<W: WriteColor>(
    writer: &mut W,
    invocation: &ContractInvocation,
    prefix: &str,
    is_last: bool,
    depth: usize
) -> std::io::Result<()> {
    let (connector, next_prefix) = if is_last {
        (tree_chars::CORNER, prefix.to_string() + tree_chars::SPACE)
    } else {
        (tree_chars::BRANCH, prefix.to_string() + tree_chars::CONTINUE)
    };

    // Render contract ID and function name
    write!(writer, "{}", prefix)?;
    write!(writer, "{}", connector)?;

    // Contract ID (cyan, bold)
    colors::contract_id(writer);
    write!(writer, "{}", invocation.contract_id)?;

    write!(writer, "::")?;

    // Function name (green, bold)
    colors::function_name(writer);
    write!(writer, "{}", invocation.function_name)?;

    colors::reset(writer);

    // Status indicator
    if invocation.is_error {
        colors::error(writer);
        write!(writer, " [ERROR]")?;
        colors::reset(writer);
    } else {
        colors::success(writer);
        write!(writer, " [OK]")?;
        colors::reset(writer);
    }

    // Resource usage
    write!(
        writer,
        " (CPU: {}, MEM: {})",
        format_cpu_usage(invocation.total_cpu_instructions),
        format_memory_usage(invocation.total_memory_bytes)
    )?;

    writeln!(writer)?;

    // Render arguments if any
    if !invocation.arguments.is_empty() {
        let args_prefix = if is_last {
            prefix.to_string() + tree_chars::SPACE
        } else {
            prefix.to_string() + tree_chars::CONTINUE
        };

        for (i, arg) in invocation.arguments.iter().enumerate() {
            let is_arg_last = i == invocation.arguments.len() - 1;
            let (arg_connector, _) = if is_arg_last {
                (tree_chars::CORNER, args_prefix.clone() + tree_chars::SPACE)
            } else {
                (tree_chars::BRANCH, args_prefix.clone() + tree_chars::CONTINUE)
            };

            write!(writer, "{}{}📝 {}", args_prefix, arg_connector, arg)?;
            writeln!(writer)?;
        }
    }

    // Render host calls if any
    if !invocation.host_calls.is_empty() {
        let host_prefix = if is_last {
            prefix.to_string() + tree_chars::SPACE
        } else {
            prefix.to_string() + tree_chars::CONTINUE
        };

        for (i, host_call) in invocation.host_calls.iter().enumerate() {
            let is_host_last = i == invocation.host_calls.len() - 1;
            render_host_call(writer, host_call, &host_prefix, is_host_last)?;
        }
    }

    // Render sub-invocations recursively
    if !invocation.sub_invocations.is_empty() {
        let sub_prefix = if is_last {
            prefix.to_string() + tree_chars::SPACE
        } else {
            prefix.to_string() + tree_chars::CONTINUE
        };

        for (i, sub_invocation) in invocation.sub_invocations.iter().enumerate() {
            let is_sub_last = i == invocation.sub_invocations.len() - 1;
            render_contract_tree(writer, sub_invocation, &sub_prefix, is_sub_last, depth + 1)?;
        }
    }

    Ok(())
}

/// Render a host function call
fn render_host_call<W: WriteColor>(
    writer: &mut W,
    host_call: &prism_core::types::trace::HostFunctionCall,
    prefix: &str,
    is_last: bool
) -> std::io::Result<()> {
    let (connector, _) = if is_last {
        (tree_chars::CORNER, prefix.to_string() + tree_chars::SPACE)
    } else {
        (tree_chars::BRANCH, prefix.to_string() + tree_chars::CONTINUE)
    };

    write!(writer, "{}", prefix)?;
    write!(writer, "{}", connector)?;

    // Host function name (yellow)
    colors::warning(writer);
    write!(writer, "🔧 {}", host_call.function_name)?;
    colors::reset(writer);

    // Status indicator
    if host_call.is_error {
        colors::error(writer);
        write!(writer, " [ERROR]")?;
        colors::reset(writer);
    }

    // Resource usage
    write!(
        writer,
        " (CPU: {}, MEM: {})",
        format_cpu_usage(host_call.cpu_instructions),
        format_memory_usage(host_call.memory_bytes)
    )?;

    writeln!(writer)?;

    // Render arguments if any
    if !host_call.arguments.is_empty() {
        let args_prefix = if is_last {
            prefix.to_string() + tree_chars::SPACE
        } else {
            prefix.to_string() + tree_chars::CONTINUE
        };

        for (i, arg) in host_call.arguments.iter().enumerate() {
            let is_arg_last = i == host_call.arguments.len() - 1;
            let (arg_connector, _) = if is_arg_last {
                (tree_chars::CORNER, args_prefix.clone() + tree_chars::SPACE)
            } else {
                (tree_chars::BRANCH, args_prefix.clone() + tree_chars::CONTINUE)
            };

            write!(writer, "{}{}📝 {}", args_prefix, arg_connector, arg)?;
            writeln!(writer)?;
        }
    }

    // Render error if any
    if let Some(error) = &host_call.error {
        let error_prefix = if is_last {
            prefix.to_string() + tree_chars::SPACE
        } else {
            prefix.to_string() + tree_chars::CONTINUE
        };

        colors::error(writer);
        write!(writer, "{}{}❌ {}", error_prefix, tree_chars::CORNER, error)?;
        colors::reset(writer);
        writeln!(writer)?;
    }

    Ok(())
}

/// Render the complete execution trace as a tree
pub fn render_trace_tree<W: WriteColor>(
    writer: &mut W,
    trace: &ExecutionTrace
) -> std::io::Result<()> {
    // Header
    colors::contract_id(writer);
    write!(writer, "🔍 Execution Trace Tree")?;
    colors::reset(writer);
    writeln!(writer)?;

    writeln!(writer, "📋 Transaction: {}", trace.tx_hash)?;
    writeln!(writer, "🌐 Network: {}", trace.network)?;
    writeln!(writer, "📊 Ledger Sequence: {}", trace.ledger_sequence)?;
    writeln!(writer)?;

    // Resource summary
    colors::warning(writer);
    write!(writer, "📈 Resource Summary")?;
    colors::reset(writer);
    writeln!(writer)?;
    writeln!(
        writer,
        "   CPU: {}/{} ({}%)",
        format_cpu_usage(trace.resource_profile.total_cpu),
        format_cpu_usage(trace.resource_profile.cpu_limit),
        (((trace.resource_profile.total_cpu as f64) / (trace.resource_profile.cpu_limit as f64)) *
            100.0) as u32
    )?;
    writeln!(
        writer,
        "   Memory: {}/{} ({}%)",
        format_memory_usage(trace.resource_profile.total_memory),
        format_memory_usage(trace.resource_profile.memory_limit),
        (((trace.resource_profile.total_memory as f64) /
            (trace.resource_profile.memory_limit as f64)) *
            100.0) as u32
    )?;
    writeln!(writer)?;

    // Contract invocation tree
    if trace.invocations.is_empty() {
        writeln!(writer, "📭 No contract invocations found")?;
    } else {
        colors::function_name(writer);
        write!(writer, "🌳 Contract Call Stack")?;
        colors::reset(writer);
        writeln!(writer)?;

        for (i, invocation) in trace.invocations.iter().enumerate() {
            let is_last = i == trace.invocations.len() - 1;
            render_contract_tree(writer, invocation, "", is_last, 0)?;
        }
    }

    // Warnings
    if !trace.resource_profile.warnings.is_empty() {
        writeln!(writer)?;
        colors::warning(writer);
        write!(writer, "⚠️  Warnings")?;
        colors::reset(writer);
        writeln!(writer)?;
        for warning in &trace.resource_profile.warnings {
            writeln!(writer, "   {}", warning)?;
        }
    }

    Ok(())
}

/// Format CPU instructions for display
fn format_cpu_usage(cpu: u64) -> String {
    if cpu < 1_000 {
        format!("{} instr", cpu)
    } else if cpu < 1_000_000 {
        format!("{:.1}K instr", (cpu as f64) / 1_000.0)
    } else {
        format!("{:.1}M instr", (cpu as f64) / 1_000_000.0)
    }
}

/// Format memory bytes for display
fn format_memory_usage(bytes: u64) -> String {
    if bytes < 1_024 {
        format!("{} B", bytes)
    } else if bytes < 1_048_576 {
        format!("{:.1} KB", (bytes as f64) / 1_024.0)
    } else if bytes < 1_073_741_824 {
        format!("{:.1} MB", (bytes as f64) / 1_048_576.0)
    } else {
        format!("{:.1} GB", (bytes as f64) / 1_073_741_824.0)
    }
}

/// Print execution trace in tree format to stdout
pub fn print_trace_tree(trace: &ExecutionTrace) -> anyhow::Result<()> {
    use termcolor::{ BufferWriter, ColorChoice };

    let writer = BufferWriter::stdout(ColorChoice::Auto);
    let mut buffer = writer.buffer();

    render_trace_tree(&mut buffer, trace)?;

    writer.print(&buffer)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_core::types::trace::{
        ContractInvocation,
        HostFunctionCall,
        ExecutionTrace,
        ResourceProfile,
        StateDiff,
    };
    use termcolor::{ Buffer, ColorChoice };

    #[test]
    fn test_format_cpu_usage() {
        assert_eq!(format_cpu_usage(500), "500 instr");
        assert_eq!(format_cpu_usage(1500), "1.5K instr");
        assert_eq!(format_cpu_usage(2_500_000), "2.5M instr");
    }

    #[test]
    fn test_format_memory_usage() {
        assert_eq!(format_memory_usage(512), "512 B");
        assert_eq!(format_memory_usage(2048), "2.0 KB");
        assert_eq!(format_memory_usage(3_145_728), "3.0 MB");
    }

    #[test]
    fn test_render_simple_tree() {
        let invocation = ContractInvocation {
            contract_id: "CB...123".to_string(),
            function_name: "transfer".to_string(),
            arguments: vec![
                "from: alice".to_string(),
                "to: bob".to_string(),
                "amount: 100".to_string()
            ],
            return_value: Some("true".to_string()),
            host_calls: vec![],
            sub_invocations: vec![],
            total_cpu_instructions: 1500,
            total_memory_bytes: 2048,
            is_error: false,
        };

        let mut buffer = Buffer::no_color();
        render_contract_tree(&mut buffer, &invocation, "", true, 0).unwrap();

        let output = String::from_utf8_lossy(&buffer.into_inner());
        assert!(output.contains("CB...123"));
        assert!(output.contains("transfer"));
        assert!(output.contains("[OK]"));
        assert!(output.contains("1.5K instr"));
        assert!(output.contains("2.0 KB"));
    }
}
