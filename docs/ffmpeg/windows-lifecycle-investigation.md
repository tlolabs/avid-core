# Windows runtime lifecycle investigation

Resumed from `8d8a108530db85c999aac21e4e54c9b7ab0c96e2`. Recipe 7 compiler flags and release gates remain unchanged. No cause is assigned to the external process solely from the previous Restart Manager snapshot.

## Source audit before new instrumentation

- All production FFmpeg/FFprobe invocations funnel through `src/process.rs::run`. No production code sets the working directory to the runtime. Child stdin is null; stdout/stderr are piped and drained by two threads.
- Normal completion uses `try_wait`; cancellation and timeout call `kill` then `wait`. The normal return path joins both reader threads. `OwnedChild::drop` also attempts kill/wait, then Rust drops the child process object and its OS handle. Kill errors are ignored, and wait errors on interruption are currently folded into the operation error; diagnostics must establish whether these occur in the failing scenario.
- An independently identified edge case needs attention: if joining the stderr reader returns an I/O error, `?` returns before the stdout thread is joined. Dropping its JoinHandle detaches it. A panicking EventSink callback can likewise bypass explicit joins. This is not yet connected to the observed rename failure, which has not reported either condition.
- `MediaTools` stores only executable paths and version strings. `Renderer` stores MediaTools and timeout settings. Neither owns an executable file, process, memory mapping, directory handle, background worker or current-directory reference. Keeping a Renderer value in scope is therefore not itself evidence of an open executable.
- Managed manifest reads use `std::fs::read`, whose file handle closes before returning. Capability discovery uses the same synchronous process runner. Staging uses synchronous `fs::copy` and `fs::read`; their temporary handles do not escape.
- `StagedOutput` owns an output NamedTempFile under the test root, outside the runtime directory. Its successful publication returns after the persisted file and extra sync handle drop; failure/cancellation drops the temporary file. `same_file` comparisons use temporary handles outside the runtime.
- The qualification application's direct fixture/probe invocations use `Command::output`, which waits and captures both streams. These calls are outside Core and require explicit diagnostic coverage too.
- The test replaces the runtime after completed render, publication cancellation, zero-duration render timeout and staged-update discovery. The zero timeout can terminate a child during loader startup; this is a useful case that must remain covered.

## Application replacement contract

A host must stop scheduling work against the old runtime, cancel or finish active work, join its own operation threads, and then release its Renderer/MediaTools references before replacement. Core operations are synchronous and immutable Renderer clones can be called concurrently; Core has no global operation registry capable of stopping host-owned threads. The current test exercises completed synchronous operations but keeps the Renderer value alive, which is a stronger expectation for today's path-only structure. It does not yet explicitly exercise cancellation of an actively encoding native Windows child from a host worker thread. Additional coverage should supplement, not replace, the existing immediate rename/rollback/removal checks.

## Diagnostic boundary

A scoped handle-owner snapshot, child-process cleanup trace and repeated clean-runner reproduction are prepared next. Diagnostic instrumentation must be opt-in or test-only and must not add production runtime dependencies. No arbitrary delay, successful retry or fallback will convert the original rename failure into a passing result. Public artifact upload of process/executable/handle metadata is awaiting explicit approval after automatic review rejected that payload.

## Prepared diagnostic milestone

The optional `lifecycle-diagnostics` feature records child spawn, successful final wait, both reader joins and a guard dropped after the Child object. Default builds contain no tracing branch and add no dependency. Test-owned fixture/probe calls explicitly record spawn and `wait_with_output` completion. A log verifier requires complete child cleanup before the unchanged immediate rename boundary.

The failure-only inspector searches only the synthetic installed directory and descendants. Restart Manager registers only its FFmpeg/FFprobe files; executable identity is requested only for the returned owners. The workflow verifies Microsoft's Authenticode signature before executing Handle.exe. It records no environment variables, credentials, unrelated file contents or machine-wide traces. Three fresh Windows workers are configured for up to 500 cycles each, stopping at the first failure rather than retrying toward a passing result. A separate native C experiment tests normal exit, immediate termination and termination while suspended, comparing rename with process/thread handles retained and after they close. Its results are diagnostic, not qualification evidence.

Local preparation validation: 42 default Rust tests, Clippy with diagnostics enabled, 19 Python infrastructure tests, workflow actionlint and diff whitespace checks pass. The native macOS ARM64 installed-runtime lifecycle test passes against the retained recipe-7 pair; its trace verifies 55 fully released children and one immediate rename boundary. This does not qualify Windows or establish the rename root cause. Public Windows diagnostic upload remains pending approval; no new Windows execution or release is claimed at this milestone.
