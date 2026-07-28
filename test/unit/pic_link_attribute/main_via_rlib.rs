fn main() {
    assert_eq!(unsafe { native_dep_sys::native_dep_value() }, 42);
}
