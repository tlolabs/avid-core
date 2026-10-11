# Repository development and CI/CD policy

This repository follows a nonblocking development and CI/CD model. Automated tests, builds, security scans, and compatibility checks provide diagnostic information; they must not prevent commits, pushes, merges, or unrelated development. Their failures must remain visible and be tracked until resolved.

No manual approvals, required reviews, or mandatory status checks are permitted. Do not add branch protection, repository rulesets, or workflow checks that enforce these gates. DCO trailers are a development convention, not an enforced requirement. Cryptographic commit/tag signing and distribution signing are not prerequisites for development or source releases.

Releases are autonomous and platform-independent where the artifact format and its consumers support that safely. Never publish an artifact that fails its functional, integrity, dependency, or target-platform verification. Do not weaken runtime or update-signature verification to make publication easier. Preserve diagnostics and publish successful platforms independently when the release contract permits; document external or consumer-contract restrictions in the consolidated CI/CD failure issue.

Future development must not reintroduce blocking gates without explicit authorization from the repository owner. Keep workflows and documentation consistent with this policy. Every task's final report must include a concise summary of failed or unrun checks and unresolved restrictions; do not present a failure as success.
