//! Asserts that the `crate` module extension records its lockfile in
//! `MODULE.bazel.lock` through Bazel's facts API instead of storing the
//! generated repository attributes (notably `build_file_content`).

use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::{Map, Value};

/// The `crate` module extension, matched by suffix because the canonical
/// repository name of `rules_rust` depends on how it was resolved.
const EXTENSION_SUFFIX: &str = "//crate_universe:extensions.bzl%crate";

/// The hub repository declared in `MODULE.bazel`.
const HUB: &str = "crate_index_facts";

/// Bazel rejects facts nested more than this many levels deep to keep
/// `MODULE.bazel.lock` human readable and VCS friendly. A whole lockfile needs
/// more than this, which is why crates are recorded one fact each.
const MAX_FACTS_DEPTH: usize = 7;

fn depth(value: &Value) -> usize {
    match value {
        Value::Object(map) => 1 + map.values().map(depth).max().unwrap_or(0),
        Value::Array(items) => 1 + items.iter().map(depth).max().unwrap_or(0),
        _ => 0,
    }
}

/// Reports the path to the first descendant of `value` keyed by `key`.
fn find_key(value: &Value, key: &str, path: &str) -> Option<String> {
    match value {
        Value::Object(map) => map.iter().find_map(|(k, v)| {
            let child = format!("{path}/{k}");
            if k == key {
                Some(child)
            } else {
                find_key(v, key, &child)
            }
        }),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .find_map(|(i, v)| find_key(v, key, &format!("{path}[{i}]"))),
        _ => None,
    }
}

fn object<'a>(value: &'a Value, key: &str) -> Option<&'a Map<String, Value>> {
    value.get(key).and_then(Value::as_object)
}

fn check(lock: &Value, failures: &mut Vec<String>) {
    let Some(facts) = object(lock, "facts") else {
        failures.push("`facts` is missing: the extension recorded nothing".to_owned());
        return;
    };

    let Some((extension, recorded)) = facts
        .iter()
        .find(|(key, _)| key.ends_with(EXTENSION_SUFFIX))
    else {
        failures.push(format!("no `facts` entry for an extension ending in `{EXTENSION_SUFFIX}`"));
        return;
    };

    let Some(entries) = recorded.as_object() else {
        failures.push(format!("the facts recorded for `{extension}` are not an object"));
        return;
    };

    // A reproducible extension is not written to `moduleExtensions`, which is
    // what keeps the per-crate `http_archive(build_file_content = ...)` specs
    // out of the lock file.
    if let Some(module_extensions) = object(lock, "moduleExtensions") {
        if module_extensions.contains_key(extension) {
            failures.push(format!(
                "`{extension}` has a `moduleExtensions` entry, so it was not reported as reproducible"
            ));
        }
    }

    if lock
        .get("factsVersions")
        .and_then(|versions| versions.get(extension))
        .is_none()
    {
        failures.push(format!("`factsVersions` has no entry for `{extension}`"));
    }

    if let Some(path) = find_key(recorded, "build_file_content", "facts") {
        failures.push(format!(
            "generated BUILD file content was recorded in the lock file at `{path}`"
        ));
    }

    let recorded_depth = depth(recorded);
    if recorded_depth > MAX_FACTS_DEPTH {
        failures.push(format!(
            "recorded facts nest {recorded_depth} levels deep, but Bazel allows at most {MAX_FACTS_DEPTH}"
        ));
    }

    let Some(hub) = entries.get(HUB).and_then(Value::as_object) else {
        failures.push(format!("no fact recorded for hub `{HUB}`"));
        return;
    };

    match hub.get("context").and_then(Value::as_object) {
        // The digest is what `cargo-bazel query` compares against to decide
        // whether the recorded lockfile is still current.
        Some(context) => match context.get("checksum").and_then(Value::as_str) {
            Some(checksum) if checksum.len() == 64 => {}
            other => failures.push(format!("`{HUB}/context` has no valid checksum: {other:?}")),
        },
        None => failures.push(format!("`{HUB}` has no `context`")),
    }

    let Some(crate_ids) = hub.get("crate_ids").and_then(Value::as_array) else {
        failures.push(format!("`{HUB}` has no `crate_ids`"));
        return;
    };

    if crate_ids.is_empty() {
        failures.push(format!("`{HUB}` recorded no crates"));
    }

    for crate_id in crate_ids {
        let Some(crate_id) = crate_id.as_str() else {
            failures.push(format!("`{HUB}/crate_ids` contains a non-string: {crate_id}"));
            continue;
        };
        let key = format!("{HUB}|{crate_id}");
        if !entries.get(&key).is_some_and(Value::is_object) {
            failures.push(format!("no fact recorded for crate `{key}`"));
        }
    }

    // Every crate is expected to be its own top level fact so that changing one
    // crate produces a single key diff.
    if entries.len() != crate_ids.len() + 1 {
        failures.push(format!(
            "expected {} facts ({} crates plus the hub), got {}",
            crate_ids.len() + 1,
            crate_ids.len(),
            entries.len(),
        ));
    }
}

fn main() -> ExitCode {
    let Ok(workspace) = std::env::var("BUILD_WORKSPACE_DIRECTORY") else {
        eprintln!("BUILD_WORKSPACE_DIRECTORY is unset, run this with `bazel run`");
        return ExitCode::FAILURE;
    };

    let path = PathBuf::from(workspace).join("MODULE.bazel.lock");
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) => {
            eprintln!("Failed to read {}: {err}", path.display());
            eprintln!("Run a build first, and ensure `--lockfile_mode` is not `off`.");
            return ExitCode::FAILURE;
        }
    };

    let lock: Value = match serde_json::from_str(&raw) {
        Ok(lock) => lock,
        Err(err) => {
            eprintln!("Failed to parse {}: {err}", path.display());
            return ExitCode::FAILURE;
        }
    };

    let mut failures = Vec::new();
    check(&lock, &mut failures);

    if failures.is_empty() {
        println!("{} records the crate_universe lockfile as facts", path.display());
        return ExitCode::SUCCESS;
    }

    eprintln!("Unexpected contents in {}:", path.display());
    for failure in &failures {
        eprintln!("  - {failure}");
    }
    ExitCode::FAILURE
}
