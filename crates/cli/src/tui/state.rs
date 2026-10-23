use grat_core::types::trace::{ContractInvocation, ExecutionTrace, ResourceProfile, StateDiff};

pub struct TuiState {
    pub tx_hash: String,
    pub network: String,
    pub ledger_sequence: u32,
    pub selected_panel: Panel,
    pub scroll_offset: usize,
    pub selected_invocation: Option<ContractInvocation>,
    pub state_diff: StateDiff,
    pub entries: Vec<TraceEntry>,
    pub invocations: Vec<ContractInvocation>,
    pub resources: ResourceProfile,
}

pub struct TraceEntry {
    pub depth: usize,
    pub contract: String,
    pub function: String,
    pub arguments: Vec<String>,
    pub return_value: Option<String>,
    pub is_error: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Panel {
    #[default]
    Timeline,
    Inspector,
    Controls,
}

impl TuiState {
    pub fn new(trace: ExecutionTrace) -> Self {
        let mut entries = Vec::new();
        append_invocations(&trace.invocations, 0, &mut entries);
        let selected_invocation = invocation_at(&trace.invocations, 0).cloned();
        Self {
            tx_hash: trace.tx_hash,
            network: trace.network,
            ledger_sequence: trace.ledger_sequence,
            selected_panel: Panel::Timeline,
            scroll_offset: 0,
            selected_invocation,
            state_diff: trace.state_diff,
            entries,
            invocations: trace.invocations,
            resources: trace.resource_profile,
        }
    }

    pub fn switch_panel(&mut self, reverse: bool) {
        self.selected_panel = match (self.selected_panel, reverse) {
            (Panel::Timeline, false) | (Panel::Controls, true) => Panel::Inspector,
            (Panel::Inspector, false) | (Panel::Timeline, true) => Panel::Controls,
            (Panel::Controls, false) | (Panel::Inspector, true) => Panel::Timeline,
        };
    }

    pub fn next(&mut self) {
        self.scroll_offset = self
            .scroll_offset
            .saturating_add(1)
            .min(self.entries.len().saturating_sub(1));
        self.update_selected_invocation();
    }

    pub fn previous(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
        self.update_selected_invocation();
    }

    /// Advance past descendants of the selected recorded invocation.
    pub fn step_over(&mut self) {
        let Some(current) = self.entries.get(self.scroll_offset) else {
            return;
        };
        self.scroll_offset = self
            .entries
            .iter()
            .enumerate()
            .skip(self.scroll_offset + 1)
            .find(|(_, entry)| entry.depth <= current.depth)
            .map_or(self.entries.len().saturating_sub(1), |(index, _)| index);
        self.update_selected_invocation();
    }

    fn update_selected_invocation(&mut self) {
        self.selected_invocation = invocation_at(&self.invocations, self.scroll_offset).cloned();
    }

    pub fn can_step(&self) -> bool {
        self.scroll_offset.saturating_add(1) < self.entries.len()
    }

    pub fn replay_status(&self) -> String {
        if self.entries.is_empty() {
            "Replay: no invocations".to_owned()
        } else {
            format!(
                "Replay: {} {}/{}",
                if self.can_step() { "Paused" } else { "End" },
                self.scroll_offset + 1,
                self.entries.len()
            )
        }
    }
}

fn invocation_at(
    invocations: &[ContractInvocation],
    target: usize,
) -> Option<&ContractInvocation> {
    fn visit<'a>(
        invocations: &'a [ContractInvocation],
        target: &mut usize,
    ) -> Option<&'a ContractInvocation> {
        for invocation in invocations {
            if *target == 0 {
                return Some(invocation);
            }
            *target -= 1;
            if let Some(found) = visit(&invocation.sub_invocations, target) {
                return Some(found);
            }
        }
        None
    }

    let mut target = target;
    visit(invocations, &mut target)
}

fn append_invocations(
    invocations: &[ContractInvocation],
    depth: usize,
    entries: &mut Vec<TraceEntry>,
) {
    for invocation in invocations {
        entries.push(TraceEntry {
            depth,
            contract: invocation.contract_id.clone(),
            function: invocation.function_name.clone(),
            arguments: invocation.arguments.clone(),
            return_value: invocation.return_value.clone(),
            is_error: invocation.is_error,
        });
        append_invocations(&invocation.sub_invocations, depth + 1, entries);
    }
}

#[cfg(test)]
pub fn fixture() -> TuiState {
    use grat_core::types::trace::{StateDiff, TraceId};
    fn invocation(name: &str, children: Vec<ContractInvocation>) -> ContractInvocation {
        ContractInvocation {
            trace_id: TraceId("fixture".to_owned()),
            contract_id: "CEXAMPLE".to_owned(),
            function_name: name.to_owned(),
            arguments: vec![],
            return_value: None,
            host_calls: vec![],
            sub_invocations: children,
            total_cpu_instructions: 0,
            total_memory_bytes: 0,
            is_error: false,
        }
    }
    TuiState::new(ExecutionTrace {
        tx_hash: "abc123".to_owned(),
        network: "testnet".to_owned(),
        ledger_sequence: 1,
        invocations: vec![
            invocation("parent", vec![invocation("child", vec![])]),
            invocation("sibling", vec![]),
        ],
        state_diff: StateDiff::default(),
        diagnostic_events: vec![],
        resource_profile: ResourceProfile {
            total_cpu: 250,
            cpu_limit: 1000,
            total_memory: 500,
            memory_limit: 1000,
            ..ResourceProfile::default()
        },
    })
}
