# Third-party notices

AVID Core is a source library. It does not redistribute FFmpeg, ffprobe, Microsoft runtime DLLs, fonts, or third-party artwork. Its crate includes small synthetic audio and image test fixtures generated locally with FFmpeg; their commands and hashes are in [the fixture provenance record](tests/fixtures/ffmpeg/provenance.json). Hosts provide their own media executables and preserve their licenses separately.

The locked Rust dependency graph is listed with exact versions, sources, checksums and license expressions in [docs/dependency-inventory.json](docs/dependency-inventory.json). The direct dependencies are `serde` and `serde_json` (MIT or Apache-2.0), `tempfile` (MIT or Apache-2.0), and `same-file` (Unlicense or MIT). Their copyright notices and full license texts remain in the upstream source packages downloaded by Cargo; this file does not claim ownership of them.

Target-specific dependencies include Microsoft's `windows-sys` and `windows-link` (MIT or Apache-2.0) from [windows-rs](https://github.com/microsoft/windows-rs). These are open-source Rust crates, not Windows App SDK or proprietary redistributable binaries. Other transitive licenses include Unlicense, Unicode-3.0, and optional Apache-2.0 with LLVM exception or LGPL-2.1-or-later alternatives; the inventory records each package's exact expression. No dependency needs to be relicensed as GPL for AVID Core's own source to use GPL-3.0-or-later.

Historical FFmpeg build research in this repository does not make the source crate a distributor of FFmpeg executables. Any future binary or media release must be inventoried and carry the applicable upstream notices.

`CODE_OF_CONDUCT.md` adapts [Contributor Covenant 2.1](https://www.contributor-covenant.org/version/2/1/code_of_conduct/) by replacing its reporting placeholder. The upstream 2.1 text is licensed [CC BY 4.0](https://github.com/EthicalSource/contributor_covenant/blob/2.1/LICENSE.md); its attribution is retained in that file. It is independent community documentation, not AVID Core library code or a Windows distribution component.
