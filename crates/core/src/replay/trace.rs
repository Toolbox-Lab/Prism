use crate::error::GratResult;
use crate::replay::sandbox::{SandboxResult, TraceEventType};
use crate::types::trace::{ContractInvocation, HostFunctionCall};

pub fn build_trace_tree(result: &SandboxResult) -> GratResult<Vec<ContractInvocation>> {
    let mut root_invocations: Vec<ContractInvocation> = Vec::new();
    let mut stack: Vec<ContractInvocation> = Vec::new();

    for event in &result.events {
        match event.event_type {
            TraceEventType::InvocationStart => {
                let invocation = ContractInvocation {
                    trace_id: result.trace_id.clone(),
                    contract_id: event
                        .data
                        .get("contract_id")
                        .and_then(|c| c.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    function_name: event
                        .data
                        .get("function")
                        .and_then(|f| f.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    arguments: Vec::new(),
                    return_value: None,
                    host_calls: Vec::new(),
                    sub_invocations: Vec::new(),
                    total_cpu_instructions: 0,
                    total_memory_bytes: 0,
                    is_error: false,
                };
                stack.push(invocation);
            }
            TraceEventType::InvocationEnd => {
                if let Some(invocation) = stack.pop() {
                    if let Some(parent) = stack.last_mut() {
                        parent.sub_invocations.push(invocation);
                    } else {
                        root_invocations.push(invocation);
                    }
                }
            }
            TraceEventType::HostFunctionCall => {
                if let Some(current) = stack.last_mut() {
                    let call = HostFunctionCall {
                        function_name: event
                            .data
                            .get("function")
                            .and_then(|f| f.as_str())
                            .unwrap_or("unknown")
                            .to_string(),
                        arguments: Vec::new(),
                        return_value: None,
                        cpu_instructions: 0,
                        memory_bytes: 0,
                        is_error: false,
                        error: None,
                    };
                    current.host_calls.push(call);
                }
            }
            _ => {}
        }
    }

    Ok(root_invocations)
}
