//! Execution stepping for the interactive debugger.
//!
//! [`ExecutionStepper`] walks the invocation tree of a recorded
//! [`ExecutionTrace`]. The tree is flattened into a depth-first ordered list of
//! [`InvocationFrame`]s so that a single index identifies the frame the
//! debugger is currently paused on. Every step command moves that index
//! forward and refreshes the [`PauseState`] describing the paused frame:
//! the current contract and function, the call depth and the remaining
//! CPU/memory budget.
//!
//! The stepper only ever moves forward through the recorded trace, so a step
//! command that has no valid target (stepping out of a root invocation, or
//! stepping past the last recorded frame) leaves the debugger on the current
//! frame.

use crate::types::trace::{ContractInvocation, ExecutionTrace};
use serde::{Deserialize, Serialize};

/// A command that advances the debugger through a recorded execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepCommand {
    /// Descend into the next nested invocation.
    ///
    /// Moves to the next recorded frame, which is the first child of the
    /// current frame when the current frame has nested calls, and the next
    /// peer invocation otherwise.
    StepInto,

    /// Run the current invocation to completion, skipping its nested calls.
    ///
    /// Moves to the next recorded frame at the same depth or a shallower one,
    /// i.e. the next peer invocation of the current frame.
    StepOver,

    /// Leave the current invocation and resume in its caller.
    ///
    /// Moves to the current frame's recorded parent invocation. Root
    /// invocations have no caller, so stepping out of one leaves the debugger
    /// where it is.
    StepOut,

    /// Run to the next top-level invocation.
    ///
    /// Moves to the next recorded root invocation, skipping the remainder of
    /// the invocation tree the debugger is currently in. When the current
    /// invocation is the last one, the debugger runs out the remainder of the
    /// trace.
    Continue,

    /// Run to the last recorded invocation.
    StepToEnd,

    /// Alias for [`StepCommand::StepToEnd`] kept for backwards compatibility.
    RunToEnd,
}

/// The state of the debugger while it is paused on an invocation frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PauseState {
    /// Position of the current frame inside the flattened invocation list.
    pub trace_position: usize,

    /// Contract that owns the invocation the debugger is paused on.
    pub current_contract: String,

    /// Function of the invocation the debugger is paused on.
    pub current_function: String,

    /// Nesting depth of the current invocation (`0` for a root invocation).
    pub call_depth: usize,

    /// Arguments passed to the invocation the debugger is paused on.
    pub current_arguments: Vec<String>,

    /// CPU instructions still available in the recorded budget.
    pub remaining_cpu: u64,

    /// Memory bytes still available in the recorded budget.
    pub remaining_memory: u64,

    /// Ledger entries visible to the debugger at the paused point.
    pub visible_storage: Vec<(String, String)>,

    /// Authorization requirements recorded by the paused invocation.
    pub auth_context: Vec<String>,
}

/// A single node of the invocation tree, flattened into depth-first order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvocationFrame {
    /// Nesting depth of the frame (`0` for a root invocation).
    pub depth: usize,

    /// Contract that owns the invocation.
    pub contract_id: String,

    /// Function that was invoked.
    pub function_name: String,

    /// Arguments passed to the invocation.
    pub arguments: Vec<String>,

    /// Position of the parent frame inside the flattened frame list.
    pub parent: Option<usize>,

    /// CPU instructions charged to this frame alone, nested calls excluded.
    pub cpu_instructions: u64,

    /// Memory bytes charged to this frame alone, nested calls excluded.
    pub memory_bytes: u64,

    /// CPU instructions consumed from the start of the trace up to this frame.
    pub cumulative_cpu: u64,

    /// Memory bytes consumed from the start of the trace up to this frame.
    pub cumulative_memory: u64,

    /// Authorization requirements recorded by the host calls of the frame.
    pub auth_context: Vec<String>,
}

/// Steps through the invocation tree of a recorded [`ExecutionTrace`].
pub struct ExecutionStepper {
    trace: Option<ExecutionTrace>,

    frames: Vec<InvocationFrame>,

    current_index: Option<usize>,

    current_state: Option<PauseState>,

    visible_storage: Vec<(String, String)>,

    is_paused: bool,
}

impl ExecutionStepper {
    /// Creates a stepper without a trace.
    pub fn new() -> Self {
        Self {
            trace: None,
            frames: Vec::new(),
            current_index: None,
            current_state: None,
            visible_storage: Vec::new(),
            is_paused: false,
        }
    }

    /// Creates a stepper paused on the first invocation of `trace`.
    pub fn with_trace(trace: ExecutionTrace) -> Self {
        let mut stepper = Self::new();
        stepper.load_trace(trace);
        stepper
    }

    /// Loads `trace` and rewinds the stepper to its first invocation.
    pub fn load_trace(&mut self, trace: ExecutionTrace) {
        let frames = flatten_invocations(&trace.invocations);
        let visible_storage = storage_view(&trace);

        self.frames = frames;
        self.visible_storage = visible_storage;
        self.trace = Some(trace);
        self.current_index = (!self.frames.is_empty()).then_some(0);
        self.is_paused = !self.frames.is_empty();
        self.refresh_state();
    }

    /// Runs `command` and returns the state the debugger is paused on.
    pub fn step(&mut self, command: StepCommand) -> Option<&PauseState> {
        tracing::debug!("Stepping: {command:?}");

        let Some(current) = self.current_index else {
            return self.current_state.as_ref();
        };

        let next = match command {
            StepCommand::StepInto => self.next_frame(current, |_| true),
            StepCommand::StepOver => {
                let depth = self.depth_at(current);
                self.next_frame(current, |candidate| candidate <= depth)
            }
            StepCommand::StepOut => self.frames.get(current).and_then(|frame| frame.parent),
            StepCommand::Continue => self
                .next_frame(current, |candidate| candidate == 0)
                .or_else(|| self.frames.len().checked_sub(1))
                .filter(|&position| position > current),
            StepCommand::StepToEnd | StepCommand::RunToEnd => self.frames.len().checked_sub(1),
        };

        if let Some(next) = next {
            if next != current {
                self.current_index = Some(next);
                self.refresh_state();
            }
        }

        self.current_state.as_ref()
    }

    /// Steps backward by one frame, strictly clamping at the first frame (`0`).
    ///
    /// Returns the updated [`PauseState`], or `None` if no trace is loaded.
    pub fn step_back(&mut self) -> Option<&PauseState> {
        let current = self.current_index?;
        let previous = current.saturating_sub(1);
        if previous != current {
            self.current_index = Some(previous);
            self.refresh_state();
        }
        self.current_state.as_ref()
    }

    /// Moves directly to `index`, strictly clamped within `0..self.frames.len()`.
    ///
    /// Returns `None` if no trace is loaded or the trace has no invocations.
    pub fn seek(&mut self, index: usize) -> Option<&PauseState> {
        let last = self.frames.len().checked_sub(1)?;
        let target = index.min(last);
        if self.current_index != Some(target) {
            self.current_index = Some(target);
            self.refresh_state();
        }
        self.current_state.as_ref()
    }

    /// The state the debugger is currently paused on.
    pub fn current_state(&self) -> Option<&PauseState> {
        self.current_state.as_ref()
    }

    /// Whether the debugger is paused on an invocation frame.
    pub fn is_paused(&self) -> bool {
        self.is_paused
    }

    /// Whether the stepper is positioned at the first recorded invocation.
    pub fn is_at_start(&self) -> bool {
        self.current_index == Some(0)
    }

    /// Whether the stepper reached the last recorded invocation.
    pub fn is_finished(&self) -> bool {
        self.current_index
            .is_some_and(|index| index + 1 >= self.frames.len())
    }

    /// Returns the active call stack from the root caller down to the paused frame.
    pub fn call_stack(&self) -> Vec<&InvocationFrame> {
        let mut stack = Vec::new();
        let mut cursor = self.current_index;
        while let Some(index) = cursor {
            let Some(frame) = self.frames.get(index) else {
                break;
            };
            stack.push(frame);
            cursor = frame.parent;
        }
        stack.reverse();
        stack
    }

    /// The trace the stepper is walking.
    pub fn trace(&self) -> Option<&ExecutionTrace> {
        self.trace.as_ref()
    }

    /// The flattened invocation frames, in depth-first order.
    pub fn frames(&self) -> &[InvocationFrame] {
        &self.frames
    }

    /// The frame the debugger is currently paused on.
    pub fn current_frame(&self) -> Option<&InvocationFrame> {
        self.current_index.and_then(|index| self.frames.get(index))
    }

    /// The position of the current frame inside the flattened frame list.
    pub fn current_index(&self) -> Option<usize> {
        self.current_index
    }

    /// Rewinds the stepper to the first invocation of the loaded trace.
    pub fn reset(&mut self) {
        self.current_index = (!self.frames.is_empty()).then_some(0);
        self.is_paused = !self.frames.is_empty();
        self.refresh_state();
    }

    /// Rebuilds the paused state from the frame the debugger is on.
    fn refresh_state(&mut self) {
        let Some(index) = self.current_index else {
            self.current_state = None;
            return;
        };

        let Some(frame) = self.frames.get(index) else {
            self.current_state = None;
            return;
        };

        let (cpu_limit, memory_limit) = self.trace.as_ref().map_or((0, 0), |trace| {
            (
                trace.resource_profile.cpu_limit,
                trace.resource_profile.memory_limit,
            )
        });

        self.current_state = Some(PauseState {
            trace_position: index,
            current_contract: frame.contract_id.clone(),
            current_function: frame.function_name.clone(),
            call_depth: frame.depth,
            current_arguments: frame.arguments.clone(),
            remaining_cpu: cpu_limit.saturating_sub(frame.cumulative_cpu),
            remaining_memory: memory_limit.saturating_sub(frame.cumulative_memory),
            visible_storage: self.visible_storage.clone(),
            auth_context: frame.auth_context.clone(),
        });
    }

    /// The nesting depth of the frame at `index`.
    fn depth_at(&self, index: usize) -> usize {
        self.frames.get(index).map_or(0, |frame| frame.depth)
    }

    /// The first frame after `index` whose depth satisfies `predicate`.
    fn next_frame(&self, index: usize, predicate: impl Fn(usize) -> bool) -> Option<usize> {
        self.frames
            .iter()
            .enumerate()
            .skip(index + 1)
            .find(|(_, frame)| predicate(frame.depth))
            .map(|(position, _)| position)
    }
}

impl Default for ExecutionStepper {
    fn default() -> Self {
        Self::new()
    }
}

/// Flattens the invocation tree into a depth-first ordered list of frames.
///
/// The CPU and memory counters of an invocation are interpreted as covering the
/// whole invocation tree below it, so the cost charged to a single frame is
/// the invocation total minus the totals of its direct children. The cumulative
/// counters then describe how much of the budget has been consumed when the
/// debugger pauses on the frame.
fn flatten_invocations(invocations: &[ContractInvocation]) -> Vec<InvocationFrame> {
    let mut frames = Vec::new();

    for invocation in invocations {
        flatten_invocation(invocation, 0, None, &mut frames);
    }

    let mut cumulative_cpu: u64 = 0;
    let mut cumulative_memory: u64 = 0;

    for frame in &mut frames {
        cumulative_cpu = cumulative_cpu.saturating_add(frame.cpu_instructions);
        cumulative_memory = cumulative_memory.saturating_add(frame.memory_bytes);
        frame.cumulative_cpu = cumulative_cpu;
        frame.cumulative_memory = cumulative_memory;
    }

    frames
}

/// Flattens a single invocation and, recursively, its nested invocations.
fn flatten_invocation(
    invocation: &ContractInvocation,
    depth: usize,
    parent: Option<usize>,
    frames: &mut Vec<InvocationFrame>,
) {
    let nested_cpu: u64 = invocation
        .sub_invocations
        .iter()
        .map(|child| child.total_cpu_instructions)
        .sum();
    let nested_memory: u64 = invocation
        .sub_invocations
        .iter()
        .map(|child| child.total_memory_bytes)
        .sum();

    let position = frames.len();

    frames.push(InvocationFrame {
        depth,
        contract_id: invocation.contract_id.clone(),
        function_name: invocation.function_name.clone(),
        arguments: invocation.arguments.clone(),
        parent,
        cpu_instructions: invocation.total_cpu_instructions.saturating_sub(nested_cpu),
        memory_bytes: invocation.total_memory_bytes.saturating_sub(nested_memory),
        cumulative_cpu: 0,
        cumulative_memory: 0,
        auth_context: auth_requirements(invocation),
    });

    for child in &invocation.sub_invocations {
        flatten_invocation(child, depth + 1, Some(position), frames);
    }
}

/// The authorization requirements recorded by the host calls of an invocation.
fn auth_requirements(invocation: &ContractInvocation) -> Vec<String> {
    invocation
        .host_calls
        .iter()
        .filter(|call| call.function_name.starts_with("require_auth"))
        .map(|call| {
            if call.arguments.is_empty() {
                call.function_name.clone()
            } else {
                format!("{}({})", call.function_name, call.arguments.join(", "))
            }
        })
        .collect()
}

/// The ledger entries visible to the debugger, ordered by ledger key.
fn storage_view(trace: &ExecutionTrace) -> Vec<(String, String)> {
    let mut entries: Vec<(String, String)> = trace
        .state_diff
        .entries
        .iter()
        .map(|entry| {
            let value = entry
                .after
                .clone()
                .or_else(|| entry.before.clone())
                .unwrap_or_default();
            (entry.key.clone(), value)
        })
        .collect();

    entries.sort_by_key(|(key, _)| key.clone());
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::trace::{
        DiffChangeType, HostFunctionCall, LedgerEntryDiff, ResourceProfile, StateDiff, TraceId,
    };

    fn invocation(
        contract_id: &str,
        function_name: &str,
        cpu: u64,
        memory: u64,
        children: Vec<ContractInvocation>,
    ) -> ContractInvocation {
        ContractInvocation {
            trace_id: TraceId::new(),
            contract_id: contract_id.to_string(),
            function_name: function_name.to_string(),
            arguments: Vec::new(),
            return_value: None,
            host_calls: Vec::new(),
            sub_invocations: children,
            total_cpu_instructions: cpu,
            total_memory_bytes: memory,
            is_error: false,
        }
    }

    fn host_call(function_name: &str, arguments: &[&str]) -> HostFunctionCall {
        HostFunctionCall {
            function_name: function_name.to_string(),
            arguments: arguments.iter().map(|arg| (*arg).to_string()).collect(),
            return_value: None,
            cpu_instructions: 0,
            memory_bytes: 0,
            is_error: false,
            error: None,
        }
    }

    /// A mock call tree:
    ///
    /// ```text
    /// root1
    /// |-- a
    /// |   |-- b
    /// |   `-- c
    /// `-- d
    /// root2
    /// |-- e
    /// `-- f
    /// ```
    fn mock_trace() -> ExecutionTrace {
        let mut b = invocation("C_B", "b", 10, 100, Vec::new());
        b.arguments = vec!["42".to_string()];

        let mut a = invocation(
            "C_A",
            "a",
            40,
            400,
            vec![b, invocation("C_C", "c", 20, 200, Vec::new())],
        );
        a.arguments = vec!["G_ALICE".to_string(), "100".to_string()];
        a.host_calls
            .push(host_call("require_auth", &["G_AUTHORIZED"]));

        let mut root1 = invocation(
            "C_ROOT1",
            "root1",
            100,
            1_000,
            vec![a, invocation("C_D", "d", 30, 300, Vec::new())],
        );
        root1.arguments = vec!["init".to_string()];

        let root2 = invocation(
            "C_ROOT2",
            "root2",
            50,
            500,
            vec![
                invocation("C_E", "e", 20, 200, Vec::new()),
                invocation("C_F", "f", 15, 150, Vec::new()),
            ],
        );

        ExecutionTrace {
            tx_hash: "tx-hash".to_string(),
            ledger_sequence: 1,
            network: "testnet".to_string(),
            invocations: vec![root1, root2],
            state_diff: StateDiff {
                entries: vec![LedgerEntryDiff {
                    key: "counter".to_string(),
                    before: Some("1".to_string()),
                    after: Some("2".to_string()),
                    change_type: DiffChangeType::Updated,
                }],
            },
            resource_profile: ResourceProfile {
                total_cpu: 150,
                cpu_limit: 400,
                total_memory: 1_500,
                memory_limit: 4_000,
                total_read_bytes: 0,
                total_write_bytes: 0,
                read_limit: 0,
                write_limit: 0,
                hotspots: Vec::new(),
                warnings: Vec::new(),
            },
            diagnostic_events: Vec::new(),
            auth_chains: Vec::new(),
        }
    }

    fn stepper() -> ExecutionStepper {
        ExecutionStepper::with_trace(mock_trace())
    }

    #[test]
    fn frames_are_flattened_in_depth_first_order() {
        let stepper = stepper();

        let names: Vec<&str> = stepper
            .frames()
            .iter()
            .map(|frame| frame.function_name.as_str())
            .collect();
        assert_eq!(names, ["root1", "a", "b", "c", "d", "root2", "e", "f"]);

        let depths: Vec<usize> = stepper.frames().iter().map(|frame| frame.depth).collect();
        assert_eq!(depths, [0, 1, 2, 2, 1, 0, 1, 1]);

        let parents: Vec<Option<usize>> =
            stepper.frames().iter().map(|frame| frame.parent).collect();
        assert_eq!(
            parents,
            [
                None,
                Some(0),
                Some(1),
                Some(1),
                Some(0),
                None,
                Some(5),
                Some(5)
            ]
        );
    }

    #[test]
    fn step_into_descends_into_the_first_child() {
        let mut stepper = stepper();

        stepper.step(StepCommand::StepInto);
        assert_eq!(stepper.current_index(), Some(1));
        assert_eq!(stepper.current_frame().unwrap().function_name, "a");

        stepper.step(StepCommand::StepInto);
        assert_eq!(stepper.current_index(), Some(2));
        assert_eq!(stepper.current_frame().unwrap().function_name, "b");
    }

    #[test]
    fn step_over_skips_nested_invocations() {
        let mut stepper = stepper();

        stepper.step(StepCommand::StepInto);
        assert_eq!(stepper.current_index(), Some(1));

        stepper.step(StepCommand::StepOver);
        assert_eq!(stepper.current_index(), Some(4));
        assert_eq!(stepper.current_frame().unwrap().function_name, "d");
    }

    #[test]
    fn step_over_from_a_root_reaches_the_next_root_invocation() {
        let mut stepper = stepper();

        stepper.step(StepCommand::StepOver);
        assert_eq!(stepper.current_index(), Some(5));
        assert_eq!(stepper.current_frame().unwrap().function_name, "root2");
    }

    #[test]
    fn step_out_returns_to_the_callers_level() {
        let mut stepper = stepper();

        stepper.step(StepCommand::StepInto);
        stepper.step(StepCommand::StepInto);
        assert_eq!(stepper.current_index(), Some(2));

        stepper.step(StepCommand::StepOut);
        assert_eq!(stepper.current_index(), Some(1));
        assert_eq!(stepper.current_frame().unwrap().function_name, "a");
    }

    #[test]
    fn step_out_of_a_root_invocation_keeps_the_current_frame() {
        let mut stepper = stepper();

        stepper.step(StepCommand::StepOut);
        assert_eq!(stepper.current_index(), Some(0));
    }

    #[test]
    fn continue_runs_to_the_next_root_invocation() {
        let mut stepper = stepper();

        stepper.step(StepCommand::StepInto);
        stepper.step(StepCommand::StepInto);
        stepper.step(StepCommand::Continue);

        assert_eq!(stepper.current_index(), Some(5));
        assert_eq!(stepper.current_frame().unwrap().function_name, "root2");
    }

    #[test]
    fn continue_runs_out_the_remainder_of_the_last_root_invocation() {
        let mut stepper = stepper();

        stepper.step(StepCommand::Continue);
        assert_eq!(stepper.current_index(), Some(5));

        stepper.step(StepCommand::Continue);
        assert_eq!(stepper.current_index(), Some(7));
        assert_eq!(stepper.current_frame().unwrap().function_name, "f");
    }

    #[test]
    fn run_to_end_moves_to_the_last_frame() {
        let mut stepper = stepper();

        stepper.step(StepCommand::RunToEnd);
        assert_eq!(stepper.current_index(), Some(7));
        assert!(stepper.is_finished());
    }

    #[test]
    fn stepping_past_the_last_frame_keeps_the_current_state() {
        let mut stepper = stepper();

        stepper.step(StepCommand::RunToEnd);
        stepper.step(StepCommand::StepInto);

        assert_eq!(stepper.current_index(), Some(7));
        assert_eq!(stepper.current_state().unwrap().current_function, "f");
    }

    #[test]
    fn pause_state_describes_the_current_frame() {
        let mut stepper = stepper();

        let state = stepper.current_state().unwrap();
        assert_eq!(state.trace_position, 0);
        assert_eq!(state.current_contract, "C_ROOT1");
        assert_eq!(state.current_function, "root1");
        assert_eq!(state.call_depth, 0);

        stepper.step(StepCommand::StepInto);

        let state = stepper.current_state().unwrap();
        assert_eq!(state.trace_position, 1);
        assert_eq!(state.current_contract, "C_A");
        assert_eq!(state.current_function, "a");
        assert_eq!(state.call_depth, 1);
    }

    #[test]
    fn remaining_budget_shrinks_while_the_stepper_advances() {
        let mut stepper = stepper();

        let state = stepper.current_state().unwrap();
        assert_eq!(state.remaining_cpu, 370);
        assert_eq!(state.remaining_memory, 3_700);

        stepper.step(StepCommand::StepInto);

        let state = stepper.current_state().unwrap();
        assert_eq!(state.remaining_cpu, 360);
        assert_eq!(state.remaining_memory, 3_600);

        stepper.step(StepCommand::StepInto);

        let state = stepper.current_state().unwrap();
        assert_eq!(state.remaining_cpu, 350);
        assert_eq!(state.remaining_memory, 3_500);
    }

    #[test]
    fn pause_state_exposes_storage_and_auth_context() {
        let mut stepper = stepper();

        let state = stepper.current_state().unwrap();
        assert_eq!(
            state.visible_storage,
            vec![("counter".to_string(), "2".to_string())]
        );
        assert!(state.auth_context.is_empty());

        stepper.step(StepCommand::StepInto);

        let state = stepper.current_state().unwrap();
        assert_eq!(
            state.auth_context,
            vec!["require_auth(G_AUTHORIZED)".to_string()]
        );
    }

    #[test]
    fn stepping_without_a_trace_is_a_no_op() {
        let mut stepper = ExecutionStepper::new();

        assert!(stepper.step(StepCommand::StepInto).is_none());
        assert!(stepper.current_state().is_none());
        assert!(!stepper.is_paused());
        assert!(stepper.frames().is_empty());
    }

    #[test]
    fn reset_rewinds_to_the_first_frame() {
        let mut stepper = stepper();

        stepper.step(StepCommand::StepInto);
        stepper.reset();

        assert_eq!(stepper.current_index(), Some(0));
        assert_eq!(stepper.current_state().unwrap().current_function, "root1");
        assert!(stepper.is_at_start());
    }

    #[test]
    fn step_to_end_moves_to_the_last_frame() {
        let mut stepper = stepper();

        let state = stepper.step(StepCommand::StepToEnd).unwrap();
        assert_eq!(state.trace_position, 7);
        assert_eq!(state.current_function, "f");
        assert!(stepper.is_finished());
        assert!(!stepper.is_at_start());
    }

    #[test]
    fn pause_state_tracks_current_invocation_arguments() {
        let mut stepper = stepper();

        assert_eq!(
            stepper.current_state().unwrap().current_arguments,
            vec!["init".to_string()]
        );

        stepper.step(StepCommand::StepInto);
        assert_eq!(
            stepper.current_state().unwrap().current_arguments,
            vec!["G_ALICE".to_string(), "100".to_string()]
        );

        stepper.step(StepCommand::StepInto);
        assert_eq!(
            stepper.current_state().unwrap().current_arguments,
            vec!["42".to_string()]
        );

        stepper.step(StepCommand::StepInto);
        assert!(stepper
            .current_state()
            .unwrap()
            .current_arguments
            .is_empty());
    }

    #[test]
    fn call_stack_tracks_active_ancestors() {
        fn stack_names(s: &ExecutionStepper) -> Vec<&str> {
            s.call_stack()
                .iter()
                .map(|frame| frame.function_name.as_str())
                .collect()
        }

        let mut stepper = stepper();

        assert_eq!(stack_names(&stepper), vec!["root1"]);

        stepper.step(StepCommand::StepInto);
        assert_eq!(stack_names(&stepper), vec!["root1", "a"]);

        stepper.step(StepCommand::StepInto);
        assert_eq!(stack_names(&stepper), vec!["root1", "a", "b"]);

        stepper.step(StepCommand::StepOut);
        assert_eq!(stack_names(&stepper), vec!["root1", "a"]);

        stepper.step(StepCommand::Continue);
        assert_eq!(stack_names(&stepper), vec!["root2"]);

        stepper.step(StepCommand::StepInto);
        assert_eq!(stack_names(&stepper), vec!["root2", "e"]);
    }

    #[test]
    fn step_back_enforces_start_boundary() {
        let mut stepper = stepper();

        assert!(stepper.is_at_start());
        stepper.step_back();
        assert_eq!(stepper.current_index(), Some(0));
        assert_eq!(stepper.current_state().unwrap().current_function, "root1");

        stepper.step(StepCommand::StepInto);
        stepper.step(StepCommand::StepInto);
        assert_eq!(stepper.current_index(), Some(2));
        assert!(!stepper.is_at_start());

        stepper.step_back();
        assert_eq!(stepper.current_index(), Some(1));
        assert_eq!(stepper.current_state().unwrap().current_function, "a");
        assert_eq!(stepper.current_state().unwrap().remaining_cpu, 360);

        stepper.step_back();
        assert_eq!(stepper.current_index(), Some(0));
        assert!(stepper.is_at_start());

        stepper.step_back();
        assert_eq!(stepper.current_index(), Some(0));
    }

    #[test]
    fn seek_clamps_to_valid_frame_bounds() {
        let mut stepper = stepper();

        let state = stepper.seek(999).unwrap();
        assert_eq!(state.trace_position, 7);
        assert!(stepper.is_finished());

        let state = stepper.seek(0).unwrap();
        assert_eq!(state.trace_position, 0);
        assert!(stepper.is_at_start());

        let mut empty = ExecutionStepper::new();
        assert!(empty.seek(0).is_none());
        assert!(empty.step_back().is_none());
        assert!(empty.call_stack().is_empty());
    }

    #[test]
    fn empty_trace_and_single_frame_boundaries() {
        let mut empty_trace = mock_trace();
        empty_trace.invocations.clear();

        let mut empty_stepper = ExecutionStepper::with_trace(empty_trace);
        assert!(empty_stepper.current_index().is_none());
        assert!(empty_stepper.current_state().is_none());
        assert!(!empty_stepper.is_paused());
        assert!(!empty_stepper.is_at_start());
        assert!(!empty_stepper.is_finished());
        assert!(empty_stepper.step(StepCommand::StepInto).is_none());
        assert!(empty_stepper.step(StepCommand::StepToEnd).is_none());

        let mut single_trace = mock_trace();
        single_trace.invocations = vec![invocation("C_ONLY", "only", 50, 500, Vec::new())];
        let mut single_stepper = ExecutionStepper::with_trace(single_trace);

        assert!(single_stepper.is_at_start());
        assert!(single_stepper.is_finished());

        for cmd in [
            StepCommand::StepInto,
            StepCommand::StepOver,
            StepCommand::StepOut,
            StepCommand::Continue,
            StepCommand::StepToEnd,
        ] {
            let state = single_stepper.step(cmd).unwrap();
            assert_eq!(state.trace_position, 0);
            assert_eq!(state.current_function, "only");
        }
        assert_eq!(single_stepper.step_back().unwrap().trace_position, 0);
    }

    #[test]
    fn debugger_module_re_exports_stepper_types() {
        let stepper = crate::debugger::ExecutionStepper::new();
        assert!(!stepper.is_paused());
        let _: crate::debugger::StepCommand = crate::debugger::StepCommand::StepToEnd;
    }
}
