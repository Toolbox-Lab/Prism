use crate::spec::decoder::{decode_contract_spec, ContractStructDef};
use stellar_xdr::curr::{
    Limits, ScSpecEntry, ScSpecTypeDef, ScSpecTypeUdt, ScSpecTypeVec, ScSpecUdtStructFieldV0,
    ScSpecUdtStructV0, WriteXdr,
};

fn leb128_encode(mut value: u64) -> Vec<u8> {
    let mut result = Vec::new();
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        result.push(byte);
        if value == 0 {
            break;
        }
    }
    result
}

fn build_wasm_with_spec_entries(entries: &[ScSpecEntry]) -> Vec<u8> {
    let mut xdr_data = Vec::new();
    for entry in entries {
        let bytes = entry.to_xdr(Limits::none()).unwrap();
        xdr_data.extend_from_slice(&bytes);
    }

    let name = "contractspecv0";
    let mut payload = Vec::new();
    payload.extend_from_slice(&leb128_encode(name.len() as u64));
    payload.extend_from_slice(name.as_bytes());
    payload.extend_from_slice(&xdr_data);

    let mut wasm = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
    wasm.push(0);
    wasm.extend_from_slice(&leb128_encode(payload.len() as u64));
    wasm.extend_from_slice(&payload);
    wasm
}

fn inner_struct_entry() -> ScSpecEntry {
    ScSpecEntry::UdtStructV0(ScSpecUdtStructV0 {
        doc: "".try_into().unwrap(),
        lib: "".try_into().unwrap(),
        name: "Inner".try_into().unwrap(),
        fields: vec![ScSpecUdtStructFieldV0 {
            doc: "".try_into().unwrap(),
            name: "value".try_into().unwrap(),
            type_: ScSpecTypeDef::U32,
        }]
        .try_into()
        .unwrap(),
    })
}

fn middle_struct_entry() -> ScSpecEntry {
    ScSpecEntry::UdtStructV0(ScSpecUdtStructV0 {
        doc: "A struct containing Inner".try_into().unwrap(),
        lib: "".try_into().unwrap(),
        name: "Middle".try_into().unwrap(),
        fields: vec![
            ScSpecUdtStructFieldV0 {
                doc: "".try_into().unwrap(),
                name: "inner".try_into().unwrap(),
                type_: ScSpecTypeDef::Udt(ScSpecTypeUdt {
                    name: "Inner".try_into().unwrap(),
                }),
            },
            ScSpecUdtStructFieldV0 {
                doc: "A label for the struct".try_into().unwrap(),
                name: "label".try_into().unwrap(),
                type_: ScSpecTypeDef::Symbol,
            },
        ]
        .try_into()
        .unwrap(),
    })
}

fn outer_struct_entry() -> ScSpecEntry {
    ScSpecEntry::UdtStructV0(ScSpecUdtStructV0 {
        doc: "Top-level struct with deep nesting".try_into().unwrap(),
        lib: "".try_into().unwrap(),
        name: "Outer".try_into().unwrap(),
        fields: vec![
            ScSpecUdtStructFieldV0 {
                doc: "".try_into().unwrap(),
                name: "middle".try_into().unwrap(),
                type_: ScSpecTypeDef::Udt(ScSpecTypeUdt {
                    name: "Middle".try_into().unwrap(),
                }),
            },
            ScSpecUdtStructFieldV0 {
                doc: "".try_into().unwrap(),
                name: "items".try_into().unwrap(),
                type_: ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
                    element_type: Box::new(ScSpecTypeDef::Udt(ScSpecTypeUdt {
                        name: "Inner".try_into().unwrap(),
                    })),
                })),
            },
            ScSpecUdtStructFieldV0 {
                doc: "".try_into().unwrap(),
                name: "id".try_into().unwrap(),
                type_: ScSpecTypeDef::U64,
            },
        ]
        .try_into()
        .unwrap(),
    })
}

fn find_struct<'a>(structs: &'a [ContractStructDef], name: &str) -> Option<&'a ContractStructDef> {
    structs.iter().find(|s| s.name == name)
}

#[test]
fn test_deeply_nested_structs_extracted_correctly() {
    let entries = vec![
        inner_struct_entry(),
        middle_struct_entry(),
        outer_struct_entry(),
    ];
    let wasm = build_wasm_with_spec_entries(&entries);
    let spec = decode_contract_spec(&wasm).unwrap();

    assert_eq!(spec.structs.len(), 3, "should extract all three structs");

    let inner = find_struct(&spec.structs, "Inner").expect("Inner struct should exist");
    assert_eq!(inner.fields.len(), 1);
    assert_eq!(inner.fields[0].name, "value");
    assert_eq!(inner.fields[0].type_name, "U32");
    assert_eq!(inner.doc, None);

    let middle = find_struct(&spec.structs, "Middle").expect("Middle struct should exist");
    assert_eq!(middle.fields.len(), 2);
    assert_eq!(middle.fields[0].name, "inner");
    assert_eq!(middle.fields[0].type_name, "Inner");
    assert_eq!(middle.fields[1].name, "label");
    assert_eq!(middle.fields[1].type_name, "Symbol");
    assert_eq!(middle.doc, Some("A struct containing Inner".to_string()));

    let outer = find_struct(&spec.structs, "Outer").expect("Outer struct should exist");
    assert_eq!(outer.fields.len(), 3);
    assert_eq!(outer.fields[0].name, "middle");
    assert_eq!(outer.fields[0].type_name, "Middle");
    assert_eq!(outer.fields[1].name, "items");
    assert_eq!(outer.fields[1].type_name, "Vec<Inner>");
    assert_eq!(outer.fields[2].name, "id");
    assert_eq!(outer.fields[2].type_name, "U64");
    assert_eq!(
        outer.doc,
        Some("Top-level struct with deep nesting".to_string())
    );
}

#[test]
fn test_nested_struct_type_defs_preserved() {
    let entries = vec![inner_struct_entry(), middle_struct_entry()];
    let wasm = build_wasm_with_spec_entries(&entries);
    let spec = decode_contract_spec(&wasm).unwrap();

    let middle = find_struct(&spec.structs, "Middle").unwrap();

    let inner_field = &middle.fields[0];
    let type_def = inner_field
        .type_def
        .as_ref()
        .expect("field type_def should be present");

    match type_def {
        ScSpecTypeDef::Udt(udt) => {
            let name: String = udt.name.to_string();
            assert_eq!(name, "Inner");
        }
        other => panic!("expected Udt variant, got {other:?}"),
    }

    let label_field = &middle.fields[1];
    let label_def = label_field
        .type_def
        .as_ref()
        .expect("label type_def should be present");
    assert!(matches!(label_def, ScSpecTypeDef::Symbol));
}

#[test]
fn test_vec_of_nested_struct_type_def() {
    let entries = vec![inner_struct_entry(), outer_struct_entry()];
    let wasm = build_wasm_with_spec_entries(&entries);
    let spec = decode_contract_spec(&wasm).unwrap();

    let outer = find_struct(&spec.structs, "Outer").unwrap();
    let items_field = &outer.fields[1];

    assert_eq!(items_field.name, "items");
    assert_eq!(items_field.type_name, "Vec<Inner>");

    let type_def = items_field
        .type_def
        .as_ref()
        .expect("items type_def should be present");

    match type_def {
        ScSpecTypeDef::Vec(vec) => match &*vec.element_type {
            ScSpecTypeDef::Udt(udt) => {
                let name: String = udt.name.to_string();
                assert_eq!(name, "Inner");
            }
            other => panic!("expected Vec element to be Udt, got {other:?}"),
        },
        other => panic!("expected Vec variant, got {other:?}"),
    }
}

#[test]
fn test_three_deep_nesting_chain() {
    let entries = vec![
        inner_struct_entry(),
        middle_struct_entry(),
        outer_struct_entry(),
    ];
    let wasm = build_wasm_with_spec_entries(&entries);
    let spec = decode_contract_spec(&wasm).unwrap();

    let outer = find_struct(&spec.structs, "Outer").unwrap();

    let middle_field = &outer.fields[0];
    assert_eq!(middle_field.name, "middle");
    assert_eq!(middle_field.type_name, "Middle");

    let middle_type = middle_field
        .type_def
        .as_ref()
        .expect("middle type_def should be present");
    match middle_type {
        ScSpecTypeDef::Udt(udt) => {
            let name: String = udt.name.to_string();
            assert_eq!(name, "Middle");
        }
        other => panic!("expected Udt for middle field, got {other:?}"),
    }

    let items_field = &outer.fields[1];
    assert_eq!(items_field.type_name, "Vec<Inner>");

    let id_field = &outer.fields[2];
    assert_eq!(id_field.type_name, "U64");
}

#[test]
fn test_struct_with_no_nesting() {
    let entries = vec![inner_struct_entry()];
    let wasm = build_wasm_with_spec_entries(&entries);
    let spec = decode_contract_spec(&wasm).unwrap();

    assert_eq!(spec.structs.len(), 1);
    let inner = &spec.structs[0];
    assert_eq!(inner.name, "Inner");
    assert_eq!(inner.fields.len(), 1);
    assert_eq!(inner.fields[0].name, "value");
    assert_eq!(inner.fields[0].type_name, "U32");
}
