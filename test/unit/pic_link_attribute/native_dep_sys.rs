// The shape of a hand-written `*-sys` crate: the `link` attribute lives in an
// rlib, and only the binary that eventually links it resolves `-lnative_dep`.
#[link(name = "native_dep")]
extern "C" {
    pub fn native_dep_value() -> i32;
}
