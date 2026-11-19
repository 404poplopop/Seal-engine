//! Property tests for SealText parse and write.

use proptest::prelude::*;
use seal_sealtext::{parse, parse_with_limits, Limits, SealMap, SealValue};

proptest! {
    #[test]
    fn property_sealtext_canonical_values_roundtrip(value in seal_value(4)) {
        let text = value.try_to_text(&Limits::default()).expect("write");
        let parsed = parse(&text).expect("parse");
        prop_assert_eq!(parsed.to_text(), text.clone());
        let again = parse(&parsed.to_text()).expect("re-parse");
        prop_assert_eq!(again.to_text(), text);
    }

    #[test]
    fn property_sealtext_arbitrary_suffixes_do_not_panic(tail in prop::string::string_regex(".{0,96}").unwrap()) {
        let seed = "schema_version = 1\nname = \"x\"\n";
        let mut blob = seed.to_string();
        blob.push_str(&tail);
        let _ = parse_with_limits(&blob, &Limits::default());
    }
}

fn seal_value(depth: u32) -> impl Strategy<Value = SealValue> {
    let leaf = prop_oneof![
        Just(SealValue::Null),
        any::<bool>().prop_map(SealValue::Bool),
        (-10_000i32..10_000).prop_map(|n| SealValue::Number(n.to_string())),
        "[a-z0-9_-]{0,24}".prop_map(SealValue::String),
    ];
    leaf.prop_recursive(depth, 48, 6, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..6).prop_map(SealValue::Array),
            prop::collection::vec(("key", inner.clone()), 0..6).prop_map(|entries| {
                let entries = entries
                    .into_iter()
                    .enumerate()
                    .map(|(index, (_, value))| (format!("k{index}"), value))
                    .collect();
                SealValue::Map(SealMap::new(entries))
            }),
            inner.clone(),
        ]
    })
}
