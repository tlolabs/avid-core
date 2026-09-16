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
            for ((path, label), result) in resources.iter().zip(&snapshot) {
                match result {
                    Ok(ids) if ids.is_empty() => eprintln!(
                        "native_owner elapsed_us={elapsed} target={label} external_count=0"
                    ),
                    Ok(ids) => {
                        for pid in ids {
                            let (name, state) = identity(*pid);
                            file_details(path, label, *pid);
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

#[repr(C)]
#[derive(Clone, Copy)]
struct HandleEntry {
    object: *mut c_void,
    pid: usize,
    value: usize,
    access: u32,
    backtrace: u16,
    object_type: u16,
    attributes: u32,
    reserved: u32,
}
#[link(name = "ntdll")]
extern "system" {
    fn NtQuerySystemInformation(
        class: u32,
        buffer: *mut c_void,
        length: u32,
        needed: *mut u32,
    ) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn DuplicateHandle(
        source: *mut c_void,
        handle: *mut c_void,
        destination: *mut c_void,
        result: *mut *mut c_void,
        access: u32,
        inherit: i32,
        options: u32,
    ) -> i32;
    fn GetFileInformationByHandle(handle: *mut c_void, information: *mut u32) -> i32;
}
fn file_details(path: &Path, label: &str, owner: u32) {
    let Ok(file) = OpenOptions::new()
        .access_mode(0x80)
        .share_mode(7)
        .custom_flags(0x02000000)
        .open(path)
    else {
        return;
    };
    let mut expected = [0u32; 13];
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), expected.as_mut_ptr()) } == 0 {
        return;
    }
    // System table is inspected in memory only, filtered to this already identified
    // runtime owner and the same file object type. Unrelated entries are never logged.
    let mut table = vec![0usize; 1024 * 1024];
    let mut needed = 0;
    let status = unsafe {
        NtQuerySystemInformation(
            64,
            table.as_mut_ptr().cast(),
            (table.len() * std::mem::size_of::<usize>()) as u32,
            &mut needed,
        )
    };
    if status < 0 {
        eprintln!("native_detail query_error={}", status as u32);
        return;
    }
    let count = table[0];
    let max = (table.len() * std::mem::size_of::<usize>() - 2 * std::mem::size_of::<usize>())
        / std::mem::size_of::<HandleEntry>();
    if count > max {
        return;
    }
    let entries =
        unsafe { std::slice::from_raw_parts(table.as_ptr().add(2).cast::<HandleEntry>(), count) };
    let Some(own_entry) = entries
        .iter()
        .find(|e| e.pid == std::process::id() as usize && e.value == file.as_raw_handle() as usize)
    else {
        return;
    };
    let process = unsafe { OpenProcess(0x40, 0, owner) };
    if process.is_null() {
        return;
    }
    for entry in entries
        .iter()
        .filter(|e| e.pid == owner as usize && e.object_type == own_entry.object_type)
    {
        let mut duplicate = std::ptr::null_mut();
        if unsafe {
            DuplicateHandle(
                process,
                entry.value as *mut c_void,
                -1isize as *mut c_void,
                &mut duplicate,
                0,
                0,
                2,
            )
        } == 0
        {
            continue;
        }
        let mut actual = [0u32; 13];
        let matches = unsafe { GetFileInformationByHandle(duplicate, actual.as_mut_ptr()) } != 0
            && [actual[7], actual[11], actual[12]] == [expected[7], expected[11], expected[12]];
        if matches {
            let mut io = IoStatus {
                status: 0,
                information: 0,
            };
            let mut position = -1i64;
            let status = unsafe {
                NtQueryInformationFile(
                    duplicate,
                    &mut io,
                    (&mut position as *mut i64).cast(),
                    8,
                    14,
                )
            };
            let size = ((actual[8] as u64) << 32) | actual[9] as u64;
            eprintln!("native_detail target={label} pid={owner} access={} inheritable={} file_position={} file_size={size} position_status={}",
                entry.access, entry.attributes & 2 != 0, position, status as u32);
        }
        unsafe {
            CloseHandle(duplicate);
        }
    }
    unsafe {
        CloseHandle(process);
    }
}
