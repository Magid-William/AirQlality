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
  on pull requests. The MinGW toolchain is **`C:\mingw64\bin`** (winlibs, gcc
  15.2) - the docs' "MSYS2 has mingw" reading is wrong, `C:\msys64` ships pacman
  and no mingw-w64 packages - and it is not on `PATH`. A probe step takes the
  first of `C:\mingw64\bin`, `C:\msys64\mingw64\bin`, `C:\msys64\ucrt64\bin` that
  has both `gcc.exe` and `windres.exe`, and fails with that list otherwise.
  `rustup` then picks the channel from `rust-toolchain.toml`
  (`stable-x86_64-pc-windows-gnu`). Gates: core tests, app unit tests
  (`--bin airqlality`), `cargo build --profile dist`. `cargo fmt --check` and clippy
  run as **reports, not gates** - the tree is hand-formatted and a `--check` diff
  spans most of `core/` and `app/`, so adopting rustfmt is its own experiment.
  `Swatinem/rust-cache`, concurrency cancels superseded runs.
  The windowed E2E suite is **not** a gate (250 s, needs a desktop and a GPU; a
  software adapter could make pixel assertions flaky) - it is a separate
  `workflow_dispatch`-only job that probes feasibility.
- **Release (`.github/workflows/release.yml`).** On tag `v*` and on manual
  dispatch. Asserts `tag == workspace version` (parsed from `Cargo.toml`), reruns
  the same tests, builds **`--profile dist`**, stages
  `AirQlality-v<ver>-windows-x64.zip` = `airqlality.exe` + `README.md` +
  `LICENSE` + `NOTICE`, writes `SHA256SUMS.txt`, then
  `gh release create --verify-tag --notes-from-tag` with `contents: write`.
- **Artifact is one exe.** `objdump -p` on the built exe lists only
  system imports (UCRT `api-ms-win-crt-*`, `d3d11`/`d3dcompiler_47`/`dcomp`/
  `dxgi`/`dwrite`, `icuuc`, `uiautomationcore`, Win32) - no `libgcc_s_seh-1` and
  no `libwinpthread-1`. The `capture.ps1` comment about MinGW runtime DLLs is
  stale, which the verification below re-tests from the outside.
- **A release profile that can actually ship (found the hard way).** The first
  `v0.1.0` asset panicked at startup on this machine (`thread 'main' panicked at
  ... gpui_platform.rs:56: failed to initialize Windows platform: Error creating
  DirectWriteTextSystem ... os error 3`), while the locally built exe next to it
  ran fine. Cause: with `debug-assertions` on, `gpui_ce_windows` compiles its
  HLSL *at runtime* from `env!("CARGO_MANIFEST_DIR")/src/shaders.hlsl`
  (`directx_renderer.rs:2355`, `canonicalize`), so the exe only starts where the
  crate source tree exists - the builder's cargo registry. With them off, its
  build script runs `fxc.exe` from the Windows SDK and `include!`s the bytecode
  (`directx_renderer.rs:2393`). Therefore: new `[profile.dist]` (inherits
  `release`, `debug-assertions = false` for that crate only); `release` stays the
  local profile for a machine without the SDK, `dist` is what CI builds and what
  the release ships. Nothing else in the workspace changes behaviour between the
  two profiles.
- **README.** A `Download` section (the zip, SmartScreen "More info -> Run
  anyway", Windows 10 1903+ for `icuuc.dll`) and CI / release / licence badges.
- **Verification.** (1) green CI run linked from the branch push; (2) a
  deliberately broken test on a throwaway commit, red, reverted; (3)
  `gh release view v0.1.0` lists both assets; (4) the zip downloaded to a temp
  directory, unzipped, hash checked, and the exe launched with `PATH` reduced to
  `System32`, screenshot showing real Cairo data; (5) `gh run view` timings.
  Results: cold CI run `39648321358` green in 17m43s - core 64/64, app 67/67,
  release build, `59 system imports, no MinGW runtime, 32.5 MiB`; the red probe
  (`19da922`, branch deleted afterwards) failed at `core tests` with the later
  steps skipped.
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
- On `windows-2025` the GNU toolchain is at **`C:\mingw64\bin`** (winlibs, gcc
  15.2), not in MSYS2: `C:\msys64\mingw64\bin\gcc.exe` does not exist, `C:\msys64`
  carries only the base MSYS2 (pacman). Put `C:\mingw64\bin` on `PATH` for `psm`
  and `windres`.
- `gh run view <id> --log-failed` — use to read a red step; plain `--log` mixes
  every step's post-action output and buries the failure.
- `Start-Process -RedirectStandardError <f>` — a GUI-subsystem exe has no console,
  but Rust's panic still lands in the redirected file; that is how the `os error 3`
  from DirectWrite init was found. Without `-Wait`, check `$p.HasExited` after a
  few seconds: a healthy widget never exits, so `-Wait` hangs forever.
- `[profile.release.package.gpui_ce_windows] debug-assertions = true` makes the
  shipped exe depend on `CARGO_MANIFEST_DIR` at runtime (HLSL compiled from
  `src/shaders.hlsl` in the *builder's* cargo registry). A `--release` exe only
  runs where it was built; publish `--profile dist` instead.
- Same-machine proof is not portability proof: the local release exe ran for
  months because its baked path happened to exist here. Always launch the
  downloaded asset, from a temp directory.
