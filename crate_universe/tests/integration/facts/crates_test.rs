//! The crates here are generated from a lockfile that only ever existed in
//! `MODULE.bazel.lock` facts, so linking against them at all is the assertion.

#[test]
fn crate_from_facts_backed_lockfile_is_usable() {
    let value: serde_json::Value = serde_json::from_str(r#"{"crate_universe": "facts"}"#).unwrap();
    assert_eq!(value["crate_universe"], "facts");
}
