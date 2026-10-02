use crate::decode::argument_decoder::ArgumentDecoder;
use crate::decode::return_decoder::ReturnValueDecoder;
use crate::spec::decoder::{ContractFunction, ContractSpec};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stellar_xdr::curr::ScVal;

pub type JsonValue = Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecodedArgument {
    pub name: String,

    pub value: JsonValue,

    pub formatted: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecodedFunctionCall {
    pub function_name: String,

    pub arguments: Vec<DecodedArgument>,

    pub return_value: Option<JsonValue>,

    pub formatted_return_value: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct FunctionCallDecoder {
    argument_decoder: ArgumentDecoder,
    return_decoder: ReturnValueDecoder,
}

impl FunctionCallDecoder {
    pub fn new() -> Self {
        Self {
            argument_decoder: ArgumentDecoder::new(),
            return_decoder: ReturnValueDecoder::new(),
        }
    }

    /// Decodes a full smart contract function call, mapping arguments and optional return value
    /// using function parameter and return specs.
    pub fn decode(
        &self,
        function_name: &str,
        raw_args: &[ScVal],
        func_spec: Option<&ContractFunction>,
        return_val: Option<&ScVal>,
        contract_spec: Option<&ContractSpec>,
    ) -> DecodedFunctionCall {
        let arguments = match func_spec {
            Some(func) => self
                .argument_decoder
                .decode_function_args(raw_args, func, contract_spec),
            None => self.argument_decoder.decode_dynamic(raw_args),
        };

        let (return_value, formatted_return_value) = match return_val {
            Some(val) => {
                let type_def = func_spec.and_then(|f| f.return_type_def.as_ref());
                let json_val = self.return_decoder.decode(val, type_def, contract_spec);
                let formatted = self
                    .return_decoder
                    .decode_to_string(val, type_def, contract_spec);
                (Some(json_val), Some(formatted))
            }
            None => (None, None),
        };

        DecodedFunctionCall {
            function_name: function_name.to_string(),
            arguments,
            return_value,
            formatted_return_value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::curr::ScSpecTypeDef;

    #[test]
    fn test_function_call_decoder_with_specs() {
        let decoder = FunctionCallDecoder::new();
        let func = ContractFunction {
            name: "add".to_string(),
            params: vec![
                ("a".to_string(), "U32".to_string()),
                ("b".to_string(), "U32".to_string()),
            ],
            return_type: "U32".to_string(),
            doc: None,
            return_type_def: Some(ScSpecTypeDef::U32),
            param_defs: vec![
                ("a".to_string(), ScSpecTypeDef::U32),
                ("b".to_string(), ScSpecTypeDef::U32),
            ],
        };

        let raw_args = vec![ScVal::U32(10), ScVal::U32(20)];
        let return_val = ScVal::U32(30);

        let decoded = decoder.decode("add", &raw_args, Some(&func), Some(&return_val), None);

        assert_eq!(decoded.function_name, "add");
        assert_eq!(decoded.arguments.len(), 2);
        assert_eq!(decoded.arguments[0].name, "a");
        assert_eq!(decoded.arguments[0].value, serde_json::json!(10));
        assert_eq!(decoded.arguments[1].name, "b");
        assert_eq!(decoded.arguments[1].value, serde_json::json!(20));
        assert_eq!(decoded.return_value, Some(serde_json::json!(30)));
        assert_eq!(decoded.formatted_return_value, Some("30".to_string()));
    }

    #[test]
    fn test_function_call_decoder_dynamic_fallback() {
        let decoder = FunctionCallDecoder::new();
        let raw_args = vec![ScVal::U32(5)];

        let decoded = decoder.decode("test_func", &raw_args, None, None, None);

        assert_eq!(decoded.function_name, "test_func");
        assert_eq!(decoded.arguments.len(), 1);
        assert_eq!(decoded.arguments[0].name, "arg0");
        assert_eq!(decoded.arguments[0].value, serde_json::json!(5));
        assert!(decoded.return_value.is_none());
    }
}
