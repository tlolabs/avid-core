use crate::{Error, EventSink, ProcessFailure, Progress, Result};
use std::{
    collections::HashMap,
    ffi::OsString,
    io::{self, Read},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);
impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub(crate) fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }
}
impl From<Arc<AtomicBool>> for CancellationToken {
    fn from(flag: Arc<AtomicBool>) -> Self {
        Self(flag)
    }
}
pub(crate) struct RunOptions<'a> {
    pub operation: &'static str,
    pub timeout: Option<Duration>,
    pub progress_duration: Option<f64>,
    pub parse_progress: bool,
    pub events: &'a dyn EventSink,
}
#[cfg(feature = "lifecycle-diagnostics")]
struct ChildTrace(u32);
#[cfg(feature = "lifecycle-diagnostics")]
impl Drop for ChildTrace {
    fn drop(&mut self) {
        // Declared before OwnedChild; Child and its OS handle have dropped first.
        eprintln!("core_child_resources_dropped pid={}", self.0);
    }
}
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let result = self.0.wait();
        #[cfg(feature = "lifecycle-diagnostics")]
        eprintln!(
            "core_child_final_wait pid={} result={result:?}",
            self.0.id()
        );
        let _ = result;
    }
}
struct Capture {
    bytes: Vec<u8>,
    truncated: bool,
}
const STDOUT_LIMIT: usize = 4 * 1024 * 1024;
const STDERR_LIMIT: usize = 64 * 1024;
// Drain concurrently without blocking on events, even when a tool emits a line without a newline.
fn drain(
    mut reader: impl Read,
    limit: usize,
    progress: Option<(mpsc::SyncSender<Progress>, Option<f64>)>,
) -> io::Result<Capture> {
    let mut capture = Capture {
        bytes: Vec::new(),
        truncated: false,
    };
    let mut buffer = [0u8; 8192];
    let mut line = Vec::new();
    let mut values = HashMap::new();
    loop {
        let n = match reader.read(&mut buffer) {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            other => other?,
        };
        if n == 0 {
            break;
        }
        if capture.bytes.len() + n > limit {
            capture.truncated = true;
            let remove = (capture.bytes.len() + n - limit).min(capture.bytes.len());
            capture.bytes.drain(..remove);
        }
        capture.bytes.extend_from_slice(&buffer[..n]);
        if let Some((sender, duration)) = &progress {
            for byte in &buffer[..n] {
                if *byte == b'\n' {
                    if let Ok(text) = std::str::from_utf8(&line) {
                        if let Some((key, value)) = text.trim().split_once('=') {
                            if matches!(key, "out_time_us" | "out_time_ms" | "out_time" | "speed") {
                                values.insert(key.to_owned(), value.to_owned());
                            }
                            if key == "progress" {
                                let _ = sender.try_send(crate::progress::parse(&values, *duration));
                            }
                        }
                    }
                    line.clear();
                } else if line.len() < 4096 {
                    line.push(*byte);
                }
            }
        }
    }
    Ok(capture)
}
pub(crate) fn run(
    executable: &Path,
    args: &[OsString],
    token: &CancellationToken,
    options: RunOptions<'_>,
) -> Result<Vec<u8>> {
    token.check()?;
    let context = |status, stderr| {
        Box::new(ProcessFailure {
            operation: options.operation,
            executable: executable.to_path_buf(),
            arguments: args.to_vec(),
            status,
            stderr,
        })
    };
    let child = Command::new(executable)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| Error::Process {
            failure: context(None, String::new()),
            source: Some(source),
        })?;
    #[cfg(feature = "lifecycle-diagnostics")]
    let _trace = {
        eprintln!(
            "core_child_spawn pid={} executable={}",
            child.id(),
            match executable.file_name().and_then(|n| n.to_str()) {
                Some("ffmpeg" | "ffmpeg.exe") => "ffmpeg",
                Some("ffprobe" | "ffprobe.exe") => "ffprobe",
                _ => "<test-tool>",
            }
        );
        ChildTrace(child.id())
    };
    let mut child = OwnedChild(child);
    let stdout = child.0.stdout.take().ok_or_else(|| Error::Process {
        failure: context(None, "stdout pipe unavailable".into()),
        source: None,
    })?;
    let stderr = child.0.stderr.take().ok_or_else(|| Error::Process {
        failure: context(None, "stderr pipe unavailable".into()),
        source: None,
    })?;
    let (sender, receiver) = mpsc::sync_channel(64);
    let progress = options
        .parse_progress
        .then_some((sender, options.progress_duration));
    let out = thread::spawn(move || drain(stdout, STDOUT_LIMIT, progress));
    let err = thread::spawn(move || drain(stderr, STDERR_LIMIT, None));
    let started = Instant::now();
    let mut interrupted = None;
    let status = loop {
        for progress in receiver.try_iter() {
            options.events.progress(progress);
        }
        if token.is_cancelled() {
            interrupted = Some(false);
            let _ = child.0.kill();
            break child.0.wait();
        }
        if options
            .timeout
            .is_some_and(|timeout| started.elapsed() >= timeout)
        {
            interrupted = Some(true);
            let _ = child.0.kill();
            break child.0.wait();
        }
        match child.0.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(source) => {
                let _ = child.0.kill();
                let _ = child.0.wait();
                break Err(source);
            }
        }
    };
    let joined = |handle: thread::JoinHandle<io::Result<Capture>>| {
        handle
            .join()
            .unwrap_or_else(|_| Err(io::Error::other("Output reader panicked")))
    };
    let stderr = joined(err).map_err(|source| Error::Process {
        failure: context(status.as_ref().ok().copied(), String::new()),
        source: Some(source),
    })?;
    let stderr = String::from_utf8_lossy(&stderr.bytes).into_owned();
    for line in stderr.lines() {
        options.events.diagnostic(line);
    }
    let stdout = joined(out).map_err(|source| Error::Process {
        failure: context(status.as_ref().ok().copied(), stderr.clone()),
        source: Some(source),
    })?;
    for progress in receiver.try_iter() {
        options.events.progress(progress);
    }
    #[cfg(feature = "lifecycle-diagnostics")]
    eprintln!(
        "core_child_readers_joined pid={} status={status:?}",
        child.0.id()
    );
    if interrupted == Some(false) || token.is_cancelled() {
        return Err(Error::Cancelled);
    }
    if interrupted == Some(true) {
        return Err(Error::Timeout(context(status.ok(), stderr)));
    }
    let status = status.map_err(|source| Error::Process {
        failure: context(None, stderr.clone()),
        source: Some(source),
    })?;
    if !status.success() {
        return Err(Error::Process {
            failure: context(Some(status), stderr),
            source: None,
        });
    }
    if stdout.truncated && !options.parse_progress {
        return Err(Error::CaptureLimit(context(Some(status), stderr)));
    }
    Ok(stdout.bytes)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    fn execute(
        script: &str,
        token: &CancellationToken,
        timeout: Option<Duration>,
    ) -> Result<Vec<u8>> {
        run(
            Path::new("/bin/sh"),
            &["-c".into(), script.into()],
            token,
            RunOptions {
                operation: "test",
                timeout,
                progress_duration: None,
                parse_progress: false,
                events: &(),
            },
        )
    }
    #[test]
    fn cancellation_reaps_owned_child() {
        let token = CancellationToken::default();
        let other = token.clone();
        let start = Instant::now();
        let worker = thread::spawn(move || execute("exec sleep 10", &other, None));
        thread::sleep(Duration::from_millis(100));
        token.cancel();
        assert!(matches!(worker.join().unwrap(), Err(Error::Cancelled)));
        assert!(start.elapsed() < Duration::from_secs(2));
    }
    #[test]
    fn timeout_and_diagnostic_context() {
        assert!(matches!(
            execute(
                "exec sleep 10",
                &CancellationToken::default(),
                Some(Duration::from_millis(50))
            ),
            Err(Error::Timeout(_))
        ));
        match execute(
            "printf 'specific failure' >&2; exit 7",
            &CancellationToken::default(),
            None,
        )
        .unwrap_err()
        {
            Error::Process { failure, .. } => {
                assert_eq!(failure.status.unwrap().code(), Some(7));
                assert!(failure.stderr.contains("specific failure"));
                assert_eq!(failure.arguments.len(), 2);
            }
            e => panic!("{e:?}"),
        }
    }
    #[test]
    fn pipe_flood_is_bounded_and_does_not_deadlock() {
        let bytes = vec![b'x'; STDOUT_LIMIT + 8192];
        let capture = drain(&bytes[..], STDOUT_LIMIT, None).unwrap();
        assert_eq!(capture.bytes.len(), STDOUT_LIMIT);
        assert!(capture.truncated);
        let script="i=0; while [ $i -lt 2000 ]; do printf 'diagnostic diagnostic diagnostic diagnostic diagnostic diagnostic diagnostic diagnostic\\n' >&2; i=$((i+1)); done; printf ok";
        assert_eq!(
            execute(
                script,
                &CancellationToken::default(),
                Some(Duration::from_secs(5))
            )
            .unwrap(),
            b"ok"
        );
    }
    #[test]
    fn precancel_does_not_spawn() {
        let token = CancellationToken::default();
        token.cancel();
        assert!(matches!(
            execute("exit 7", &token, None),
            Err(Error::Cancelled)
        ));
    }
}

#[cfg(all(test, unix))]
mod capture_limit_tests {
    use super::*;
    #[test]
    fn excessive_probe_stdout_fails_explicitly() {
        let result = run(
            Path::new("/bin/sh"),
            &[
                "-c".into(),
                "exec dd if=/dev/zero bs=1048576 count=5 2>/dev/null".into(),
            ],
            &CancellationToken::default(),
            RunOptions {
                operation: "large probe",
                timeout: Some(Duration::from_secs(5)),
                progress_duration: None,
                parse_progress: false,
                events: &(),
            },
        );
        assert!(matches!(result, Err(Error::CaptureLimit(_))));
    }
}
