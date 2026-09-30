use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepCommand {
    StepInto,

    StepOver,

    StepOut,

    Continue,

    RunToEnd,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PauseState {
    pub trace_position: usize,

    pub current_contract: String,

    pub current_function: String,

    pub call_depth: usize,

    pub remaining_cpu: u64,

    pub remaining_memory: u64,

    pub visible_storage: Vec<(String, String)>,

    pub auth_context: Vec<String>,
}

pub struct ExecutionStepper {
    current_state: Option<PauseState>,

    is_paused: bool,
}

impl ExecutionStepper {
    pub fn new() -> Self {
        Self {
            current_state: None,
            is_paused: false,
        }
    }

    pub fn step(&mut self, command: StepCommand) -> Option<&PauseState> {
        tracing::debug!("Stepping: {command:?}");
        self.current_state.as_ref()
    }

    pub fn current_state(&self) -> Option<&PauseState> {
        self.current_state.as_ref()
    }

    pub fn is_paused(&self) -> bool {
        self.is_paused
    }
}

impl Default for ExecutionStepper {
    fn default() -> Self {
        Self::new()
    }
}
