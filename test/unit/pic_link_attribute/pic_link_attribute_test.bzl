"""Unittests for `#[link(name = ...)]` resolution against PIC `cc_library` deps.

`rules_cc` names the PIC variant of a static library `libfoo.pic.a`, and since
rules_rust started preferring PIC artifacts when rustc emits PIE binaries that
is the artifact fed to the linker in `--compilation_mode=opt` builds. Neither a
`#[link(name = "foo")]` attribute in the sources nor a bare `-lfoo` can resolve
that file name, so a canonically named `libfoo.a` symlink has to be placed on
the library search path.

See https://github.com/bazelbuild/rules_rust/issues/4148.
"""

load("@bazel_skylib//lib:unittest.bzl", "analysistest", "asserts", "unittest")
load("@bazel_skylib//rules:build_test.bzl", "build_test")
load("//rust:defs.bzl", "rust_binary", "rust_library")
load(
    "//test/unit:common.bzl",
    "assert_argv_contains",
    "assert_argv_contains_prefix_not",
)

# The file names a linker resolves `-lnative_dep` from. `libnative_dep.pic.a`,
# which is what `rules_cc` hands us, is deliberately not among them.
_LINKABLE_BASENAMES = ["libnative_dep.a", "native_dep.lib"]

def _pic_link_attribute_test_impl(ctx):
    env = analysistest.begin(ctx)
    tut = analysistest.target_under_test(env)
    rustc_action = [action for action in tut.actions if action.mnemonic == "Rustc"][0]

    # The `.pic` infix is not part of the library's name, so it must not leak
    # into the `-l` flag: the `#[link(name = "native_dep")]` attribute in
    # `main.rs` makes rustc emit a bare `-lnative_dep` for the same library,
    # and both have to resolve to the same file.
    assert_argv_contains(env, rustc_action, "-lstatic=native_dep")
    assert_argv_contains_prefix_not(env, rustc_action, "-lstatic=native_dep.pic")

    # Whichever artifact that `-l` flag resolves to has to be reachable from
    # the library search path under exactly that name.
    inputs = rustc_action.inputs.to_list()
    search_dirs = [
        input.dirname
        for input in inputs
        if input.basename in _LINKABLE_BASENAMES
    ]
    if not search_dirs:
        unittest.fail(env, "None of {} are inputs of the Rustc action".format(_LINKABLE_BASENAMES))
        return analysistest.end(env)

    asserts.true(
        env,
        [d for d in search_dirs if "-Lnative={}".format(d) in rustc_action.argv],
        "None of {} are on the library search path in {}".format(search_dirs, rustc_action.argv),
    )

    return analysistest.end(env)

_pic_link_attribute_test = analysistest.make(
    _pic_link_attribute_test_impl,
    config_settings = {
        # `rules_cc` only gives static libraries the `.pic` infix when both a
        # PIC and a non-PIC variant may be built, which on Linux means `opt`.
        "//command_line_option:compilation_mode": "opt",
    },
)

def _opt_transition_impl(_settings, _attr):
    return {"//command_line_option:compilation_mode": "opt"}

_opt_transition = transition(
    implementation = _opt_transition_impl,
    inputs = [],
    outputs = ["//command_line_option:compilation_mode"],
)

def _opt_alias_impl(ctx):
    # `ctx.attr.actual` is a list of 1 item due to the transition.
    return [DefaultInfo(files = ctx.attr.actual[0][DefaultInfo].files)]

_opt_alias = rule(
    implementation = _opt_alias_impl,
    attrs = {
        "actual": attr.label(mandatory = True, cfg = _opt_transition),
        "_allowlist_function_transition": attr.label(
            default = "@bazel_tools//tools/allowlists/function_transition_allowlist",
        ),
    },
    doc = "Builds `actual` with `--compilation_mode=opt`.",
)

def pic_link_attribute_test_suite(name):
    """Entry-point macro called from the BUILD file.

    Args:
        name: Name of the macro.
    """
    rust_binary(
        name = "main",
        srcs = ["main.rs"],
        edition = "2018",
        link_deps = [":native_dep"],
        tags = ["manual"],
    )

    # The shape hand-written `*-sys` crates have: the `link` attribute sits in
    # an rlib, which doesn't link anything itself, and only the binary pulling
    # it in has to resolve `-lnative_dep` — against a library that is a
    # transitive rather than a direct dependency there.
    rust_library(
        name = "native_dep_sys",
        srcs = ["native_dep_sys.rs"],
        edition = "2018",
        link_deps = [":native_dep"],
        tags = ["manual"],
    )

    rust_binary(
        name = "main_via_rlib",
        srcs = ["main_via_rlib.rs"],
        edition = "2018",
        tags = ["manual"],
        deps = [":native_dep_sys"],
    )

    _pic_link_attribute_test(
        name = "pic_link_attribute_analysis_test",
        target_under_test = ":main",
    )

    _pic_link_attribute_test(
        name = "pic_link_attribute_via_rlib_analysis_test",
        target_under_test = ":main_via_rlib",
    )

    # Actually link the binaries in `opt` so that a regression surfaces as a
    # link failure and not only as a changed command line.
    _opt_alias(
        name = "main_opt",
        actual = ":main",
        tags = ["manual"],
    )

    _opt_alias(
        name = "main_via_rlib_opt",
        actual = ":main_via_rlib",
        tags = ["manual"],
    )

    build_test(
        name = "pic_link_attribute_build_test",
        targets = [
            ":main_opt",
            ":main_via_rlib_opt",
        ],
    )

    native.test_suite(
        name = name,
        tests = [
            ":pic_link_attribute_analysis_test",
            ":pic_link_attribute_via_rlib_analysis_test",
            ":pic_link_attribute_build_test",
        ],
    )
