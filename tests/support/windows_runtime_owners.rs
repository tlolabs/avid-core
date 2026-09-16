//! Failure-only, in-process diagnostic. Never compiled into the library/runtime.
//! Queries only the synthetic runtime and its four known files. No global process
//! enumeration, command lines, user names or full paths are printed.
use std::{
    ffi::c_void,
    fs::OpenOptions,
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    path::Path,
    time::{Duration, Instant},
};
#[repr(C)]
struct IoStatus {
    status: usize,
    information: usize,
}
#[link(name = "ntdll")]
extern "system" {
    fn NtQueryInformationFile(
        handle: *mut c_void,
        status: *mut IoStatus,
        information: *mut c_void,
        length: u32,
        class: u32,
    ) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
    fn CloseHandle(handle: *mut c_void) -> i32;
    fn QueryFullProcessImageNameW(
        handle: *mut c_void,
        flags: u32,
        name: *mut u16,
        length: *mut u32,
    ) -> i32;
    fn WaitForSingleObject(handle: *mut c_void, timeout: u32) -> u32;
}
fn owners(path: &Path) -> Result<Vec<u32>, u32> {
    // FILE_READ_ATTRIBUTES; share read/write/delete; support directory handles.
    let file = OpenOptions::new()
        .access_mode(0x80)
        .share_mode(7)
        .custom_flags(0x02000000)
        .open(path)
        .map_err(|e| e.raw_os_error().unwrap_or(-1) as u32)?;
    // FILE_PROCESS_IDS_USING_FILE_INFORMATION: ULONG count, native-aligned ULONG_PTR ids[].
    let mut buffer = [0usize; 1025];
    let mut status = IoStatus {
        status: 0,
        information: 0,
    };
    let code = unsafe {
        NtQueryInformationFile(
            file.as_raw_handle(),
            &mut status,
            buffer.as_mut_ptr().cast(),
            std::mem::size_of_val(&buffer) as u32,
            47,
        )
    };
    if code < 0 {
        return Err(code as u32);
    }
    let count = buffer[0] as u32 as usize;
    if count > buffer.len() - 1 {
        return Err(0xc0000004);
    }
    // This query's own file reference is expected; do not mistake it for a Core leak.
    let self_pid = std::process::id();
    Ok(buffer[1..=count]
        .iter()
        .map(|id| *id as u32)
        .filter(|id| *id != self_pid)
        .collect())
}
fn identity(pid: u32) -> (String, u32) {
    let process = unsafe { OpenProcess(0x1000 | 0x100000, 0, pid) };
    if process.is_null() {
        return ("unavailable".into(), u32::MAX);
    }
    let mut buffer = [0u16; 32768];
    let mut len = buffer.len() as u32;
    let valid = unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut len) };
    let state = unsafe { WaitForSingleObject(process, 0) };
    unsafe {
        CloseHandle(process);
    }
    let name = if valid != 0 {
        String::from_utf16_lossy(&buffer[..len as usize])
            .rsplit('\\')
            .next()
            .unwrap_or("unavailable")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            .collect()
    } else {
        "unavailable".into()
    };
    (name, state)
}
pub fn observe(runtime: &Path) {
    let began = Instant::now();
    let resources = [
        (runtime.to_owned(), "<RUNTIME>"),
        (runtime.join("ffmpeg.exe"), "<RUNTIME>/ffmpeg.exe"),
        (runtime.join("ffprobe.exe"), "<RUNTIME>/ffprobe.exe"),
        (runtime.join("spec.json"), "<RUNTIME>/spec.json"),
        (runtime.join("build.json"), "<RUNTIME>/build.json"),
    ];
    let mut previous = Vec::new();
    // Sampling occurs only AFTER failure and never changes the failing result.
    // Its sole purpose is measuring owner lifetime, not delaying/retrying rename.
    for sample in 0..=100 {
        let snapshot: Vec<_> = resources.iter().map(|(p, _)| owners(p)).collect();
        if sample == 0 || snapshot != previous || sample == 100 {
            let elapsed = began.elapsed().as_micros();
            for ((_, label), result) in resources.iter().zip(&snapshot) {
                match result {
                    Ok(ids) if ids.is_empty() => eprintln!(
                        "native_owner elapsed_us={elapsed} target={label} external_count=0"
                    ),
                    Ok(ids) => {
                        for pid in ids {
                            let (name, state) = identity(*pid);
                            eprintln!("native_owner elapsed_us={elapsed} target={label} pid={pid} name={name} wait={state}");
                        }
                    }
                    Err(code) => eprintln!(
                        "native_owner elapsed_us={elapsed} target={label} query_error={code}"
                    ),
                }
            }
        }
        previous = snapshot;
        if sample < 100 {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
