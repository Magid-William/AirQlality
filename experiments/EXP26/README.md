# EXP26 - CI pipeline and the first downloadable release

**Status:** `inProg`.

## Summary
A pipeline that builds and tests every push, plus a tag-driven release that ships
a self-contained Windows zip, so the widget can be downloaded instead of built.

## Hypothesis
A tag push `v0.1.0` produces a GitHub release asset that runs on a machine with
no Rust and no MinGW installed, and any regression in `core`/`app` turns CI red
before it can be released.

**Falsified if:**
- the exe extracted from the release zip fails to start when the MinGW directory
  is removed from `PATH`, or shows anything but the timeline;
- its SHA-256 does not match `SHA256SUMS.txt` in the same asset;
- a deliberately failing test still yields a green CI run;
- the release job accepts a tag whose version disagrees with `Cargo.toml`;
- the release asset needs a runtime DLL that is not part of Windows.

## Methodology & blast radius
- **CI (`.github/workflows/ci.yml`).** `windows-2025`, on push to any branch and
  on pull requests. MSYS2's MinGW is preinstalled but *not* on `PATH`; a probe
  step puts `C:\msys64\mingw64\bin` first and fails loudly if `gcc` or `windres`
  is missing. `rustup` then picks the channel from `rust-toolchain.toml`
  (`stable-x86_64-pc-windows-gnu`). Gates: core tests, app unit tests
  (`--bin airqlality`), `cargo build --release`. `cargo fmt --check` and clippy
  run as **reports, not gates** - the tree is hand-formatted and a `--check` diff
  spans most of `core/` and `app/`, so adopting rustfmt is its own experiment.
  `Swatinem/rust-cache`, concurrency cancels superseded runs.
  The windowed E2E suite is **not** a gate (250 s, needs a desktop and a GPU; a
  software adapter could make pixel assertions flaky) - it is a separate
  `workflow_dispatch`-only job that probes feasibility.
- **Release (`.github/workflows/release.yml`).** On tag `v*` and on manual
  dispatch. Asserts `tag == workspace version` (parsed from `Cargo.toml`), reruns
  the same tests, builds release, stages
  `AirQlality-v<ver>-windows-x64.zip` = `airqlality.exe` + `README.md` +
  `LICENSE` + `NOTICE`, writes `SHA256SUMS.txt`, then
  `gh release create --verify-tag --notes-from-tag` with `contents: write`.
- **Release profile untouched.** `gpui_ce_windows` keeps `debug-assertions = true`
  (no `fxc.exe` locally): the runner does have the Windows SDK, but a release
  experiment should ship the binary kind already proven, not a new configuration.
- **Artifact is one exe.** `objdump -p` on the local release exe lists only
  system imports (UCRT `api-ms-win-crt-*`, `d3d11`/`d3dcompiler_47`/`dcomp`/
  `dxgi`/`dwrite`, `icuuc`, `uiautomationcore`, Win32) - no `libgcc_s_seh-1` and
  no `libwinpthread-1`. The `capture.ps1` comment about MinGW runtime DLLs is
  stale, which the verification below re-tests from the outside.
- **README.** A `Download` section (the zip, SmartScreen "More info -> Run
  anyway", Windows 10 1903+ for `icuuc.dll`) and CI / release / licence badges.
- **Verification.** (1) green CI run linked from the branch push; (2) a
  deliberately broken test on a throwaway commit, red, reverted; (3)
  `gh release view v0.1.0` lists both assets; (4) the zip downloaded to a temp
  directory, unzipped, hash checked, and the exe launched with `PATH` reduced to
  `System32`, screenshot showing real Cairo data; (5) `gh run view` timings.
- **May touch:** `.github/workflows/`, `experiments/EXP26/`, `experiments-overview.md`,
  `README.md`, tag `v0.1.0`, the GitHub release, repository Actions settings.
- **Off-limits:** `core/` and `app/` source and tests (no code changes at all,
  including `Cargo.toml` - the version stays `0.1.0`), the E2E suite's behaviour,
  other experiments, history rewrites, new crates, installers or code signing.

## Open questions (known unknowns)
- [ ] First tag — `v0.1.0`, matching the workspace version. **Assumption:** yes.
- [ ] Triggers — CI on every branch push and PR, release on tags only (manual
      dispatch as backstop). **Assumption:** yes.
- [ ] Windowed E2E on CI — excluded from the gate, manual probe job only.
      **Assumption:** excluded.
- [ ] Clippy — non-blocking for now, promoted in a later experiment.
      **Assumption:** non-blocking.
- [ ] Release notes — authored on the annotated tag, `--notes-from-tag`.
      **Assumption:** yes.
- [ ] Artifact shape — one zip, no MSI/NSIS. **Assumption:** zip only.
- [ ] Store / winget / scoop — out of scope. **Assumption:** deferred.

## Conclusion (findings)
_Pending._

## Learnings
- `objdump -p target\release\airqlality.exe | Select-String "DLL Name"` — use
  before claiming a GNU build is self-contained; it settles the runtime-DLL
  question that `capture.ps1` only guesses at.
