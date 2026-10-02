use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxonomyEntry {
    pub id: String,

    pub category: ErrorCategory,

    pub code: u32,

    pub name: String,

    pub severity: String,

    pub since_protocol: Option<u32>,

    pub deprecated_protocol: Option<u32>,

    pub summary: String,

    pub detailed_explanation: String,

    #[serde(default)]
    pub common_causes: Vec<TaxonomyCause>,

    #[serde(default)]
    pub suggested_fixes: Vec<TaxonomyFix>,

    #[serde(default)]
    pub related_errors: Vec<String>,

    pub source_file: Option<String>,

    pub source_line: Option<u32>,

    pub documentation_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxonomyCause {
    pub description: String,

    pub likelihood: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxonomyFix {
    pub description: String,

    pub difficulty: String,

    pub requires_upgrade: bool,

    pub example: Option<String>,

    pub id: Option<String>,

    pub remedy_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ErrorCategory {
    Budget,
    Storage,
    Auth,
    Context,
    Value,
    Object,
    Crypto,
    Contract,
    Wasm,
    Events,
}

impl std::fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Budget => write!(f, "Budget"),
            Self::Storage => write!(f, "Storage"),
            Self::Auth => write!(f, "Auth"),
            Self::Context => write!(f, "Context"),
            Self::Value => write!(f, "Value"),
            Self::Object => write!(f, "Object"),
            Self::Crypto => write!(f, "Crypto"),
            Self::Contract => write!(f, "Contract"),
            Self::Wasm => write!(f, "WASM"),
            Self::Events => write!(f, "Events"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxonomySchema {
    pub category: CategoryMeta,
    pub errors: Vec<TaxonomyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CategoryMeta {
    pub name: String,
    pub description: String,
    pub source_module: String,
}
