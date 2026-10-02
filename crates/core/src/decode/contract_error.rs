use crate::decode::decode_context::DecodeContext;
use crate::error::GratResult;
use crate::types::config::NetworkConfig;
use crate::types::contract_id::ContractId;
use crate::types::report::ContractErrorInfo;

pub async fn resolve(
    contract_id: &str,
    error_code: u32,
    ctx: &DecodeContext,
) -> GratResult<ContractErrorInfo> {
    resolve_with_network(contract_id, error_code, &ctx.network).await
}

async fn resolve_with_network(
    contract_id: &str,
    error_code: u32,
    network: &NetworkConfig,
) -> GratResult<ContractErrorInfo> {
    ContractId::new(contract_id)?;

    let resolver =
        crate::decode::contract_error_resolver::ContractErrorResolver::new(network.clone());
    let (error_name, doc_comment) = resolver.resolve(contract_id, error_code).await;

    let error_name_opt = if error_name == error_code.to_string() {
        None
    } else {
        Some(error_name)
    };

    Ok(ContractErrorInfo {
        contract_id: contract_id.to_string(),
        error_code,
        error_name: error_name_opt,
        doc_comment,
        learn_more: "https://developers.stellar.org/docs/learn/smart-contracts/errors#contract-specific-errors".to_string(), 
    })
}
