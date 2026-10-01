> Historical FFmpeg build/qualification research, superseded by Core 0.3.0.
> These requirements do not gate Core source releases or host builds.
> See [the current integration contract](../integration.md).

# Recipe 7 application-packaging prerequisite audit

Audit date: 2026-09-16. Core source inspected: `e972b75673beaedfaf4edb950f6b2822231defea`. This report and the accompanying residual-observation updates change documentation only. No release policy, test, runtime recipe or host application was changed.

## Exact requirement and classification

`runtime/ffmpeg/spec.json` sets `qualification_policy.host_packaging` to `required`, with scope “Native Core runtime plus existing host packaging evidence”. `scripts/ffmpeg/package.py::promote` enforces that requirement separately from native archive qualification. For each of the six targets, `validate_qualification` requires:

- A `host_packaging` ledger entry with `status: passed` and nonempty evidence.
- Evidence stored beneath `docs/ffmpeg`, authenticated by its recorded SHA-256.
- A report with matching target, gate and passed status, and **the exact FFmpeg and FFprobe hashes being promoted**.

The report represents actual application packaging/signing, launch and update qualification, as described by `docs/ffmpeg/README.md`, `software-qualification.md`, and the existing host observation's explicit limitations. It does not mean that the Core tarball exists or that a generic Core test application passed. Core's archive, source, provenance and installation checks already pass separately.

**Classification: missing application qualification evidence, with incomplete host release infrastructure/integration.** No new AVID Core runtime defect has been demonstrated. This is not merely an unrun Core command: the unchanged hosts cannot qualify this exact candidate through their current production paths.

| Required target | Existing host-packaging ledger | Exact Recipe 7 evidence |
| --- | --- | --- |
| macos-arm64 | Incomplete; Recipe 6 candidate observations only | Missing |
| macos-x86_64 | Not run | Missing |
| windows-x86_64 | Not run | Missing |
| windows-arm64 | Not run | Missing |
| linux-x86_64 | Not run | Missing |
| linux-arm64 | Not run | Missing |

The old macOS ARM64 report has `status: observed`, uses different executable hashes and explicitly excludes production signing, authenticated updates and manual UI/accessibility validation. Its ad-hoc signing, media/native tests and bundle launch observations remain useful, but cannot become a passed Recipe 7 report by changing its status. Hosted-runner OS authorization already supersedes the older OS execution restriction; that is not the blocker here.

## Read-only host audit

Audited ATIV at `f38c433076c2980ff356da3d54e6f9b465439e6d` and EnCAP at `860c408d424979aefc84a26adcfec6b3052ccd8e`. The cited workflow, runtime-pin and helper files had no local modifications. No host files were written and no host workflows were dispatched.

- Both `runtime/core-revision` files and native workflow checkouts select Core `eab97dd043187aa8b7a1cae4eb2c1228fa25a9db`, not the Recipe 7 qualified source. ATIV's `script/acquire_core_runtime.py` and EnCAP's `script/check_core_runtime.sh` reject a mismatched Core checkout. EnCAP also checks its Cargo Git pin and requires a clean Core checkout. Overriding those guards would not qualify the existing production integration.
- ATIV's `.github/workflows/native-release.yml` acquires the pinned production runtime before packaging. Its signing-enabled macOS job requires Developer ID certificate/identity and notarization configuration. Existing Core evidence contains no corresponding production-signed/notarized Recipe 7 application result. The repository-secret name checks found no macOS signing/notarization entries in Core or either host; this is not a claim about certificates or credentials that might exist elsewhere.
- EnCAP's `.github/workflows/build-platforms.yml` still obtains third-party FFmpeg for Windows, performs ARM64 cross-build assembly on the x64 runner, and defines only a Linux x64 application job. Dispatching it unchanged cannot provide exact Core Recipe 7 package evidence for the full matrix. Its macOS `script/build_and_run.sh` uses ad-hoc signing; the inspected workflow does not implement Developer ID notarization qualification.
- EnCAP's `script/package_core_candidate_macos.sh` is explicitly a local qualification candidate route, depends on an existing application template, ad-hoc signs, and disables automatic update checks. It cannot establish authenticated application upgrade behavior.

The current ordering also has a dependency cycle: Core requires application evidence before publication, while normal ATIV production provisioning requires the published Core runtime first. Existing candidate helpers offer a way to exercise prepublication payloads, but only within an explicitly prepared host qualification setup; they do not replace production signing/update evidence or silently authorize changing host source pins.

## Reproduced failure

The already retained, verified candidates come from [native run 35157560636](https://github.com/tlolabs/avid-core/actions/runs/35157560636) at `fab2ed86bb64582d3f7a7dd736c713cc3951550b`. Their checksums and native results are in [the qualification report](qualification-2026-09-16.md) and [machine-readable evidence](evidence/native-qualification-r7.json).

From the Core checkout, with that run's archives, corresponding sources and receipts retained in the indicated ignored directory:

```sh
GITHUB_SHA=fab2ed86bb64582d3f7a7dd736c713cc3951550b \
  python3 scripts/ffmpeg/package.py promote \
  .ffmpeg-work/repair-audit/promotable-35157560636
```

Result:

```text
ValueError: Unperformed qualification gate: macos-arm64 host_packaging
```

The explicit SHA above selects the identity of these existing candidates for a **local preflight only**; it is not a publication workaround. The actual release workflow must run from its exact qualified source commit. Calling the same `validate_qualification(..., gates=['host_packaging'])` for every target with its recorded binary pair confirms the identical missing gate for all six. No manifest was produced and no publication was attempted.

## Why this cannot be completed safely within Core alone

Completing the requirement requires qualifying actual host applications against the exact candidate: their selected Core source, native package layout, signatures/notarization where applicable, launch, and authenticated replacement/update path. Updating a Core JSON report cannot supply these observations. A new synthetic host inside Core would prove another Core integration test, not the documented application gate. Selecting the existing `--host-packaging downstream` option would explicitly redefine the current prerequisite and is excluded by the owner's instruction for this attempt.

**Smallest next action:** authorize a narrowly scoped ATIV/EnCAP candidate qualification effort to select the exact Core candidate and supply the required signing/notarization configuration, then return genuine exact-pair application evidence for the six target gates to Core. That work must resolve the prepublication candidate-versus-production acquisition ordering without bypassing trust checks, and retain the hosts' launch/update requirements. It must happen in the host scope; this task was expressly limited to Core.

Once evidence is available, bind it in Core's ledger, run the complete final native matrix from the chosen release commit, promote those exact archives, and download/verify the immutable published assets using `scripts/ffmpeg/acquire.py` on each native target. Those final-release steps remain unperformed, rather than being represented by the earlier successful native run.

## Historical cleanup disposition

The one late historical failure is a residual observation, **not an active release blocker**, per the owner's 2026-09-16 direction. Its original panic line, numeric OS error and responsible owner were not retained. Another 3,000 historical full cycles and 1,000 reduced cycles did not reproduce it. The corrected implementation passed 1,500 lifecycle cycles and verified 53,250 released children. No proven attribution is assigned retrospectively; investigate again only if concrete evidence establishes a current reproducible defect. See [the retained sanitized record](evidence/windows-historical-cleanup-r7.json).

No `ffmpeg-9.0.1-r7` runtime release or `v0.2.2` Core tag has been published. There are no published asset checksums or release commit to report. Recipe 7's narrow Windows compiler fix remains unchanged. Core's native runtime is qualified on the six recorded hosted platforms; production migration is still blocked by the required application evidence.
