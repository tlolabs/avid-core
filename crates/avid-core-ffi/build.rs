fn main() {
    // Consumers must never inherit Cargo's absolute development output path as
    // an LC_ID_DYLIB dependency. Static XCFramework slices need no install name.
    if matches!(
        std::env::var("CARGO_CFG_TARGET_OS").as_deref(),
        Ok("macos" | "ios")
    ) {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/libavid_core_ffi.dylib");
    }
}
