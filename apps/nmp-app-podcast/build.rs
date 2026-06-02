use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let target = env::var("TARGET").unwrap();

    if target.contains("apple-ios") {
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let stub_src = out_dir.join("chkstk_stub.c");
        let stub_obj = out_dir.join("chkstk_stub.o");

        std::fs::write(&stub_src, "void __chkstk_darwin(void) {}\n")
            .expect("Failed to write chkstk stub source");

        // aarch64-apple-ios-sim contains "ios-sim"; device targets do not.
        let is_sim = target.contains("ios-sim") || target.contains("x86_64");
        let sdk = if is_sim { "iphonesimulator" } else { "iphoneos" };

        // Use xcrun to get the SDK path — works regardless of Xcode install location.
        let sdk_path = Command::new("xcrun")
            .args(["--sdk", sdk, "--show-sdk-path"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_owned())
            .unwrap_or_default();

        // Rust uses "aarch64-apple-ios-sim" but clang requires "aarch64-apple-ios-simulator".
        let clang_target = target.replace("ios-sim", "ios-simulator");

        let status = Command::new("clang")
            .args(["-isysroot", &sdk_path, "-target", &clang_target, "-c"])
            .arg(&stub_src)
            .arg("-o")
            .arg(&stub_obj)
            .status()
            .expect("Failed to run clang for chkstk stub");

        if !status.success() {
            panic!("chkstk stub compilation failed for target {target}");
        }

        println!("cargo:rustc-link-search=native={}", out_dir.display());
        println!("cargo:rustc-link-arg={}", stub_obj.display());
    }
}
