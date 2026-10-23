use metastable::js::{js_number, js_to_fixed, to_js_json_pretty};
use serde_json::Value;

#[test]
fn matches_node_number_formatting() {
    let golden: Vec<Value> =
        serde_json::from_str(include_str!("golden/js_number.json")).unwrap();
    assert!(golden.len() >= 50);
    for g in golden {
        let x = f64::from_bits(u64::from_str_radix(g["bits"].as_str().unwrap(), 16).unwrap());
        assert_eq!(js_number(x), g["toString"], "toString({x:e})");
        for (key, f) in [("toFixed0", 0), ("toFixed1", 1), ("toFixed2", 2), ("toFixed4", 4)] {
            assert_eq!(js_to_fixed(x, f), g[key], "{x:e}.{key}");
        }
    }
}

#[test]
fn pretty_json_matches_json_stringify_layout() {
    let v: Value = serde_json::from_str(
        r#"{"a":1,"b":[],"c":{},"d":[1.5,{"e":"x\"y"}],"f":null,"g":true,"h":50.0}"#,
    )
    .unwrap();
    let expected = "{\n  \"a\": 1,\n  \"b\": [],\n  \"c\": {},\n  \"d\": [\n    1.5,\n    {\n      \"e\": \"x\\\"y\"\n    }\n  ],\n  \"f\": null,\n  \"g\": true,\n  \"h\": 50\n}";
    assert_eq!(to_js_json_pretty(&v), expected);
}
