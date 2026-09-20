# Release guide

The manifest identifies the current release as `0.2.0`.
Preparing or validating a package does not publish it.

## Toolchain and compatibility

The crate uses edition 2024. Validation uses Rust 1.98.1; no minimum supported
Rust version is declared. Do not infer an MSRV from the edition. Before promising
an older toolchain, run the full applicable feature and target checks with it.

The default features are `std,opt,digest,simd`. Keep public documentation aligned
with Cargo metadata. The legacy backend feature remains an alias for `opt`;
other feature combinations should be selected explicitly. The `digest` adapter
uses Digest 0.11 and does not implement Digest 0.10 traits.

## Validation

Use the matrix in [AGENTS.md](../AGENTS.md) and the repository CI configuration.
At minimum, check default, force-portable, no-default, std-only, Digest, SIMD,
zeroize, and pair-interleave combinations. Run default and all-features clippy
separately because force-portable excludes accelerated adapters.

```bash
cargo test --locked
cargo test --locked --all-features
cargo test --locked --no-default-features
cargo test --locked --no-default-features --features std
cargo test --locked --no-default-features --features simd,digest
cargo test --locked --no-default-features --features zeroize
cargo test --locked --no-default-features --features std,pair-interleave
cargo test --locked --release --test api_surface --test boundaries --test many
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps
cargo run --locked --example etag_stream
cargo run --locked --release --example checksum_profile
```

README examples are also crate-level doctests. Check x86_64 and aarch64 on native
hosts for the backends being released. Record when an ISA is unavailable and
separate cross-compilation from actual execution. Current local validation does
not establish native AVX2/AVX-512 execution or big-endian runtime coverage.

No kernel behavior changes are intended by packaging or documentation cleanup.
For actual backend changes, follow the [performance methodology](performance.md)
and retain data rather than claiming speedups from a single diagnostic run.

## Package contents

Cargo.toml uses an explicit allowlist. The package includes:

- Source, tests, benchmarks, and declared examples.
- Cargo metadata and lockfile.
- README, changelog, LICENSE, and NOTICE.
- Architecture, performance notes, and curated validation results.

Maintainer scripts, CI configuration, contributor instructions, release tooling,
IDE state, environment files, logs, and build output are excluded. `.gitignore`
controls local repository noise; it does not replace the package allowlist.
Keep Cargo.lock for repeatable `--locked` checks, even though consumers resolve
library dependencies using their own lockfile.

```bash
cargo package --locked --list
cargo package --locked
cargo publish --locked --registry crates-io --dry-run
```

A local review of uncommitted changes may add `--allow-dirty`. Do not interpret
that as a clean release checkout. Inspect the archive's LICENSE/NOTICE and
examples, and verify the package after extraction. `cargo package` performs
compilation verification; the tests above provide behavior checks.

## Local preparation check — 2026-09-19

The `0.1.0` candidate was checked on aarch64 macOS with Rust 1.98.1.

| Check | Result |
| --- | --- |
| Default / all-features / force-portable | Passed, including README doctests |
| No-default / std / simd,digest / zeroize / std,pair-interleave | Passed |
| Release API, boundary, and batch tests | 18 passed |
| Default and all-features clippy | Passed with warnings denied |
| Default and no-default rustdoc | Passed with warnings denied |
| x86_64 optimized features; wasm32 no-std | Compilation passed; no native runtime claim |
| Renamed profile and streaming examples | Ran successfully |
| Metadata, ignore rules, documentation links, archive contents | Verified |
| Package verification and crates.io dry run | Passed; 47 files, no upload performed |

The library's executable hashing code is unchanged by this preparation. That check used an uncommitted local snapshot and performed no registry upload.
Remote CI and native ISA coverage remain separate release evidence.

## 0.2.0 preparation — 2026-09-20

The manifest, lockfile, changelog, and registry installation example identify
`0.2.0`. Curated validation JSON is versioned and included in the package;
local binaries, raw benchmark runs, and maintainer scripts remain excluded.

Native aarch64 macOS checks passed for default and all features, minimal
zeroize, accelerated zeroize, and the std/pair-interleave profile. Release API,
boundary, batch, and long-message differential tests passed, as did both clippy
profiles, formatting, docs.rs-feature and minimal rustdoc, release metadata guard
tests, and the x86_64 cross-check. Cross-compilation is not native execution.
Windows release validation additionally exposed excessive stack use from debug
SIMD inlining. Debug builds now reuse a single compression step's frame; release
builds retain the measured inlining behavior. A 1 MiB thread-stack regression
test covers lane groups and padding boundaries.

The package preview was verified offline from the local snapshot. The tagged
release workflow must separately pass native cross-platform CI, a fresh RustSec
audit, clean-checkout packaging, and the registry dry run before upload. Consult
that workflow's result and the registry for publication status; this preparation
record is not a claim that an upload has occurred.

## Automated checks and release workflow

- `ci.yml` runs on branch pushes and pull requests, and is reusable by releases.
  It covers native Linux x86_64/aarch64, macOS, and Windows tests, feature profiles,
  no-std target checks, lints, docs, release boundary tests, and package verification.
- `audit.yml` runs on branch pushes, pull requests, manual dispatch, and each Monday
  at 03:23 UTC. It uses cargo-audit 0.22.2, refreshes the RustSec database, and scans
  the committed lockfile, including development dependencies. Advisories and audit
  warnings fail the job; it does not create issues or modify dependency versions.
- `release.yml` starts on `v*` tag pushes or manual dispatch **on a version tag**.
  It requires the tag to equal `v` plus the Cargo version, a clean checkout, and a
  commit in `main` history. It reuses CI and Audit, runs `cargo package` first to
  create the archive, then runs `cargo publish --dry-run`, and retains the verified
  `.crate` as a workflow artifact for 14 days.

Tag pushes create release candidates only. Actual registry upload additionally
requires manual dispatch with `publish=true` and the `crates-io` environment.
Before enabling upload, configure that environment's reviewer/tag restrictions
and its `CARGO_REGISTRY_TOKEN` secret with a crates.io token scoped to publishing
this crate. The workflow does not configure environment protection automatically.
The token is exposed only to the final publish step through
`CARGO_REGISTRY_TOKEN`, Cargo's credential variable for the built-in crates.io
registry (including `--registry crates-io`). No publishing credentials are needed by CI,
Audit, or candidate dry runs. Re-publishing an existing version is not supported.

Release helper scripts use Python 3.11+ (`tomllib`); the validation jobs use the
Ubuntu 24.04 runner. Test their rejection paths locally with:

```bash
python3 -m unittest discover -s .github/scripts -p 'test_*.py' -v
```

After preparing, committing, and reviewing a version, the release sequence is:

```bash
# Example only: use the version actually recorded in Cargo.toml.
git tag -a v0.2.0 -m 'Release v0.2.0'
git push origin v0.2.0
# The tag run validates and retains a candidate without uploading to crates.io.
gh workflow run release.yml --ref v0.2.0 -f publish=false
# Only after deciding to publish and configuring the environment:
gh workflow run release.yml --ref v0.2.0 -f publish=true
```

Both candidate and publish jobs check out the triggering revision by SHA, so a
later branch update does not change the source being released. This workflow
publishes the Rust crate; it does not create a GitHub Release entry automatically.

## Before publication

- Confirm the intended version, repository URLs, license notices, and changelog.
- Update the changelog's unreleased heading with the chosen version/date.
- Confirm registry availability and owner permissions at release time.
- Commit the reviewed source and lockfile, then repeat packaging from the clean
  checkout. Record local validation and CI status separately.
- Publish and tag only when the release has been explicitly requested.

The crate uses Apache-2.0 with retained third-party notices. This guide does not
change licensing, promise an untested MSRV, or assert that registry publication
has already occurred.
