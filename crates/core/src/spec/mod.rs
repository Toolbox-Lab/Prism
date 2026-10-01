pub mod decoder;
pub mod metadata;
pub mod resolver;

pub use decoder::{
    ContractEnumCase, ContractEnumDef, ContractErrorEntry, ContractFunction, ContractSpec,
    ContractStructDef, ContractStructField, ContractUnionCase, ContractUnionDef, SpecParser,
};
pub use metadata::{extract_contract_metadata, ContractMetadata};
pub use resolver::{ContractId, ResolverStats, SCSpecResolver};

#[cfg(test)]
mod tests;
