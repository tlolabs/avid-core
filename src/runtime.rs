//! Optional filesystem helpers for caller-owned tool directories.
use std::{path::Path, time::Duration};

/// Move a caller-owned runtime directory, allowing transient Windows readers to close.
///
/// Before calling, the host must stop new operations, cancel or finish active ones,
/// join its worker threads, and serialize runtime updates. This does not stop users
/// of the runtime, authenticate a package, or replace an application's update transaction.
/// The destination should be a new path on the same volume (for example a rollback
/// directory). Rename remains atomic; no copy/delete fallback is performed.
///
/// The first attempt is immediate. On Windows only, access-denied, sharing-violation
/// and lock-violation errors can be retried for at most `max_wait` between attempts.
/// Other errors fail immediately. A persistent lock still fails. `Duration::ZERO`
/// requests one attempt. Two seconds accommodates the measured sub-second hosted
/// runner executable reads; applications may choose a different bounded policy.
/// Other operating systems perform one ordinary filesystem rename.
pub fn move_runtime_directory(
    source: &Path,
    destination: &Path,
    max_wait: Duration,
) -> std::io::Result<()> {
    runtime_directory_operation(|| std::fs::rename(source, destination), max_wait)
}

/// Remove a caller-owned obsolete runtime, with the same Windows wait policy as
/// [`move_runtime_directory`]. Removal is not atomic and can be partial on error;
/// keep the active runtime and any required rollback copy in separate directories.
pub fn remove_runtime_directory(directory: &Path, max_wait: Duration) -> std::io::Result<()> {
    runtime_directory_operation(|| std::fs::remove_dir_all(directory), max_wait)
}

fn runtime_directory_operation(
    mut operation: impl FnMut() -> std::io::Result<()>,
    max_wait: Duration,
) -> std::io::Result<()> {
    #[cfg(not(windows))]
    {
        let _ = max_wait;
        operation()
    }
    #[cfg(windows)]
    {
        let started = std::time::Instant::now();
        let mut interval = Duration::from_millis(5);
        #[cfg(feature = "lifecycle-diagnostics")]
        let mut retries = 0u32;
        loop {
            match operation() {
                Ok(()) => {
                    #[cfg(feature = "lifecycle-diagnostics")]
                    if retries > 0 {
                        eprintln!(
                            "runtime_directory_recovered retries={retries} elapsed_ms={}",
                            started.elapsed().as_millis()
                        );
                    }
                    return Ok(());
                }
                Err(error) => {
                    if !matches!(error.raw_os_error(), Some(5 | 32 | 33)) {
                        return Err(error);
                    }
                    let Some(remaining) = max_wait.checked_sub(started.elapsed()) else {
                        return Err(error);
                    };
                    if remaining.is_zero() {
                        return Err(error);
                    }
                    #[cfg(feature = "lifecycle-diagnostics")]
                    {
                        retries += 1;
                        eprintln!(
                            "runtime_directory_retry attempt={retries} os_error={}",
                            error.raw_os_error().unwrap()
                        );
                    }
                    std::thread::sleep(interval.min(remaining));
                    // Never issue another filesystem operation after the wait budget.
                    if started.elapsed() >= max_wait {
                        return Err(error);
                    }
                    interval = (interval * 2).min(Duration::from_millis(50));
                }
            }
        }
    }
}
