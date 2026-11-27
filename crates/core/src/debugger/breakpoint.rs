use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Breakpoint {
    pub id: u32,
    pub condition: BreakpointCondition,
    pub enabled: bool,
    pub hit_count: u32,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakpointCondition {
    FunctionEntry { contract_id: Option<String>, function_name: String },
    FunctionExit { contract_id: Option<String>, function_name: String },
    HostFunction { function_name: String },
    ContractCall { target_contract_id: String },
    BudgetThreshold { cpu_instructions: u64 },
    StorageAccess { ledger_key: String },
}

/// Event information emitted by the replay engine for a single frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InvocationEvent {
    FunctionEntry,
    FunctionExit,
    HostFunction { function_name: String },
    ContractCall,
    StorageAccess { ledger_key: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionFrame {
    pub contract_id: String,
    pub function_name: String,
    pub event: InvocationEvent,
    pub target_contract_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BudgetState {
    pub cpu_instructions: u64,
}

#[derive(Debug)]
pub struct BreakpointController {
    breakpoints: Vec<Breakpoint>,
    next_id: u32,
}

impl BreakpointController {
    pub fn new() -> Self { Self { breakpoints: Vec::new(), next_id: 1 } }

    pub fn add_breakpoint(&mut self, condition: BreakpointCondition, label: Option<String>) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).unwrap_or(1);
        self.breakpoints.push(Breakpoint { id, condition, enabled: true, hit_count: 0, label });
        id
    }

    pub fn remove_breakpoint(&mut self, id: u32) -> bool {
        let before = self.breakpoints.len();
        self.breakpoints.retain(|bp| bp.id != id);
        before != self.breakpoints.len()
    }

    pub fn toggle_breakpoint(&mut self, id: u32) -> Option<bool> {
        self.breakpoints.iter_mut().find(|bp| bp.id == id).map(|bp| {
            bp.enabled = !bp.enabled;
            bp.enabled
        })
    }

    /// Evaluate all enabled conditions; increment the hit count of every match.
    pub fn should_break(&mut self, frame: &ExecutionFrame, budget: &BudgetState) -> bool {
        let mut matched = false;
        for bp in &mut self.breakpoints {
            if !bp.enabled { continue; }
            let hit = match &bp.condition {
                BreakpointCondition::FunctionEntry { contract_id, function_name } =>
                    frame.event == InvocationEvent::FunctionEntry
                    && frame.function_name == *function_name
                    && contract_id.as_ref().map_or(true, |id| id == &frame.contract_id),
                BreakpointCondition::FunctionExit { contract_id, function_name } =>
                    frame.event == InvocationEvent::FunctionExit
                    && frame.function_name == *function_name
                    && contract_id.as_ref().map_or(true, |id| id == &frame.contract_id),
                BreakpointCondition::HostFunction { function_name } =>
                    matches!(&frame.event, InvocationEvent::HostFunction { function_name: actual } if actual == function_name),
                BreakpointCondition::ContractCall { target_contract_id } =>
                    frame.event == InvocationEvent::ContractCall
                    && frame.target_contract_id.as_ref() == Some(target_contract_id),
                BreakpointCondition::BudgetThreshold { cpu_instructions } =>
                    budget.cpu_instructions >= *cpu_instructions,
                BreakpointCondition::StorageAccess { ledger_key } =>
                    matches!(&frame.event, InvocationEvent::StorageAccess { ledger_key: actual } if actual == ledger_key),
            };
            if hit { bp.hit_count = bp.hit_count.saturating_add(1); matched = true; }
        }
        matched
    }

    pub fn reset(&mut self) { for bp in &mut self.breakpoints { bp.hit_count = 0; } }
    pub fn list(&self) -> &[Breakpoint] { &self.breakpoints }

    // Compatibility with the original controller API.
    pub fn add(&mut self, condition: BreakpointCondition, label: Option<String>) -> u32 {
        self.add_breakpoint(condition, label)
    }
    pub fn remove(&mut self, id: u32) -> bool { self.remove_breakpoint(id) }
    pub fn toggle(&mut self, id: u32) -> Option<bool> { self.toggle_breakpoint(id) }
}

impl Default for BreakpointController {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(event: InvocationEvent) -> ExecutionFrame {
        ExecutionFrame { contract_id: "C123".into(), function_name: "transfer".into(),
            event, target_contract_id: Some("C456".into()) }
    }

    #[test]
    fn evaluates_entry_exit_host_contract_budget_and_storage_conditions() {
        let mut controller = BreakpointController::new();
        let conditions = [
            BreakpointCondition::FunctionEntry { contract_id: Some("C123".into()), function_name: "transfer".into() },
            BreakpointCondition::FunctionExit { contract_id: None, function_name: "transfer".into() },
            BreakpointCondition::HostFunction { function_name: "put".into() },
            BreakpointCondition::ContractCall { target_contract_id: "C456".into() },
            BreakpointCondition::BudgetThreshold { cpu_instructions: 75 },
            BreakpointCondition::StorageAccess { ledger_key: "balance".into() },
        ];
        for condition in conditions { controller.add_breakpoint(condition, None); }
        let budget = BudgetState { cpu_instructions: 80 };
        for event in [
            InvocationEvent::FunctionEntry, InvocationEvent::FunctionExit,
            InvocationEvent::HostFunction { function_name: "put".into() },
            InvocationEvent::ContractCall,
            InvocationEvent::StorageAccess { ledger_key: "balance".into() },
        ] {
            assert!(controller.should_break(&frame(event), &budget));
        }
        assert_eq!(controller.list().iter().map(|bp| bp.hit_count).collect::<Vec<_>>(), [1, 1, 1, 1, 5, 1]);
    }

    #[test]
    fn disabled_breakpoints_do_not_hit_and_reset_clears_counts() {
        let mut controller = BreakpointController::new();
        let id = controller.add_breakpoint(BreakpointCondition::BudgetThreshold { cpu_instructions: 10 }, None);
        assert_eq!(controller.toggle_breakpoint(id), Some(false));
        assert!(!controller.should_break(&frame(InvocationEvent::FunctionEntry), &BudgetState { cpu_instructions: 20 }));
        assert_eq!(controller.toggle_breakpoint(id), Some(true));
        assert!(controller.should_break(&frame(InvocationEvent::FunctionEntry), &BudgetState { cpu_instructions: 20 }));
        controller.reset();
        assert_eq!(controller.list()[0].hit_count, 0);
        assert!(controller.remove_breakpoint(id));
    }
}
