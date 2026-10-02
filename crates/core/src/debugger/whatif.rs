use crate::error::GratResult;
use crate::types::trace::ExecutionTrace;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WhatIfPatch {
    ModifyArgument {
        index: usize,

        new_value: String,
    },

    ModifyLedgerEntry {
        key: String,

        new_value: String,
    },

    ModifyResourceLimits {
        cpu_limit: Option<u64>,

        memory_limit: Option<u64>,
    },

    ModifyAuth {
        add_signer: Option<String>,

        remove_signer: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhatIfResult {
    pub original: ExecutionTrace,

    pub modified: ExecutionTrace,

    pub divergence_point: Option<usize>,

    pub summary: String,
}

pub async fn simulate_whatif(
    _tx_hash: &str,
    _patches: &[WhatIfPatch],
    _network: &crate::types::config::NetworkConfig,
) -> GratResult<WhatIfResult> {
    Err(crate::error::GratError::Internal(
        "What-if simulation not yet implemented".to_string(),
    ))
}
