/// Resolve the SDK's explicitly selected C++ ABI. Windows GNU/LLVM must never
/// inherit the MSVC assumption, and Android must use the same shared NDK runtime
/// as FFmpeg/x265 rather than introducing a second static libc++ instance.
pub fn cpp_runtime(
    os: &str,
    target_env: &str,
    declared: Option<&str>,
) -> Result<Option<String>, &'static str> {
    let runtime = match os {
        "macos" | "ios" => "c++",
        "android" => {
            if declared != Some("c++_shared") {
                return Err("Android inference SDK must select the shared NDK libc++ runtime");
            }
            "c++_shared"
        }
        "windows" if target_env == "msvc" => return Ok(None),
        "windows" => match declared {
            Some("c++") => "c++",
            Some("stdc++") => "stdc++",
            _ => return Err("Windows GNU/LLVM inference SDK must declare c++ or stdc++"),
        },
        _ => "stdc++",
    };
    if declared.is_some_and(|value| value != runtime) {
        return Err("C++ runtime conflicts with target SDK policy");
    }
    Ok(Some(runtime.to_owned()))
}
