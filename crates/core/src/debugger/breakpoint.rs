//! Breakpoint system for the interactive debugger.
//!
//! Supports breakpoints on: function entries/exits, host function calls,
//! contract addresses, budget thresholds, and storage access patterns.

use serde::{Deserialize, Serialize};

/// A breakpoint definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breakpoint {
    /// Unique breakpoint ID.
    pub id: u32,
    /// Breakpoint condition.
    pub condition: BreakpointCondition,
    /// Whether this breakpoint is enabled.
    pub enabled: bool,
    /// Optional label/description.
    pub label: Option<String>,
    /// Number of times this breakpoint has been hit.
    pub hit_count: u32,
}

/// Conditions that trigger a breakpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BreakpointCondition {
    /// Break when a specific contract function is entered.
    FunctionEntry {
        contract_id: Option<String>,
        function_name: String,
    },
    /// Break when a specific contract function exits.
    FunctionExit {
        contract_id: Option<String>,
        function_name: String,
    },
    /// Break on any call to a specific host function.
    HostFunction { function_name: String },
    /// Break when a cross-contract call targets a specific contract.
    ContractCall { target_contract_id: String },
    /// Break when CPU budget exceeds a threshold.
    BudgetThreshold { cpu_instructions: u64 },
    /// Break when a specific ledger key is accessed.
    StorageAccess { ledger_key: String },
}

/// Breakpoint controller — evaluates conditions at each trace point.
pub struct BreakpointController {
    /// Active breakpoints.
    breakpoints: Vec<Breakpoint>,
    /// Auto-incrementing ID counter.
    next_id: u32,
}

impl BreakpointController {
    /// Create a new breakpoint controller.
    pub fn new() -> Self {
        Self {
            breakpoints: Vec::new(),
            next_id: 1,
        }
    }

    /// Add a breakpoint and return its ID.
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

    /// Remove a breakpoint by ID.
    pub fn remove(&mut self, id: u32) -> bool {
        let len_before = self.breakpoints.len();
        self.breakpoints.retain(|bp| bp.id != id);
        self.breakpoints.len() < len_before
    }

    /// Toggle a breakpoint's enabled state.
    pub fn toggle(&mut self, id: u32) -> Option<bool> {
        self.breakpoints
            .iter_mut()
            .find(|bp| bp.id == id)
            .map(|bp| {
                bp.enabled = !bp.enabled;
                bp.enabled
            })
    }

    /// Get all active breakpoints.
    pub fn list(&self) -> &[Breakpoint] {
        &self.breakpoints
    }
}

impl Default for BreakpointController {
    fn default() -> Self {
        Self::new()
    }
}
