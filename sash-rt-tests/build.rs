fn main() {
    let rt_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("sash-rt/target/release");
    let rt_lib = rt_dir.join("libsash_rt.a");

    // sash-rt is a separate cargo workspace, so nothing here changes when the
    // runtime is rebuilt. Without this, cargo skips the build script and the
    // tests link against a STALE libsash_rt_test.a — new runtime symbols come
    // up "undefined" and changed behavior tests green against the old code.
    println!("cargo:rerun-if-changed={}", rt_lib.display());

    // Create a modified version of libsash_rt.a that localizes _start and main
    // so they don't conflict with the test harness
    let status = std::process::Command::new("objcopy")
        .args([
            "--localize-symbol=_start",
            "--localize-symbol=panic",
            "--localize-symbol=strlen",
            "--localize-symbol=memcpy",
            "--localize-symbol=memset",
            "--localize-symbol=memcmp",
            "--localize-symbol=bcmp",
        ])
        .arg(&rt_lib)
        .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/libsash_rt_test.a"))
        .status()
        .expect("objcopy failed");
    assert!(status.success(), "objcopy failed");

    println!(
        "cargo:rustc-link-search=native={}/target",
        env!("CARGO_MANIFEST_DIR")
    );
    println!("cargo:rustc-link-lib=static=sash_rt_test");
}
