// The `link` attribute makes rustc emit a bare `-lnative_dep` for the linker,
// which only resolves if a `libnative_dep.a` (and not just the `.pic.a` variant
// `rules_cc` produces) is on the library search path.
#[link(name = "native_dep")]
extern "C" {
    fn native_dep_value() -> i32;
}

fn main() {
    assert_eq!(unsafe { native_dep_value() }, 42);
}
