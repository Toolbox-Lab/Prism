use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breakpoint {
    pub id: u32,

    pub condition: BreakpointCondition,

    pub enabled: bool,

    pub label: Option<String>,

    pub hit_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BreakpointCondition {
    FunctionEntry {
        contract_id: Option<String>,
        function_name: String,
    },

    FunctionExit {
        contract_id: Option<String>,
        function_name: String,
    },

    HostFunction {
        function_name: String,
    },

    ContractCall {
        target_contract_id: String,
    },

    BudgetThreshold {
        cpu_instructions: u64,
    },

    StorageAccess {
        ledger_key: String,
    },
}

pub struct BreakpointController {
    breakpoints: Vec<Breakpoint>,

    next_id: u32,
}

impl BreakpointController {
    pub fn new() -> Self {
        Self {
            breakpoints: Vec::new(),
            next_id: 1,
        }
    }

    pub fn add(&mut self, condition: BreakpointCondition, label: Option<String>) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.breakpoints.push(Breakpoint {
            id,
            condition,
            enabled: true,
            label,
            hit_count: 0,
        });
        id
    }

    pub fn remove(&mut self, id: u32) -> bool {
        let len_before = self.breakpoints.len();
        self.breakpoints.retain(|bp| bp.id != id);
        self.breakpoints.len() < len_before
    }

    pub fn toggle(&mut self, id: u32) -> Option<bool> {
        self.breakpoints
            .iter_mut()
            .find(|bp| bp.id == id)
            .map(|bp| {
                bp.enabled = !bp.enabled;
                bp.enabled
            })
    }

    pub fn list(&self) -> &[Breakpoint] {
        &self.breakpoints
    }
}

impl Default for BreakpointController {
    fn default() -> Self {
        Self::new()
    }
}
