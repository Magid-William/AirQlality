# EXP02 - gpui Windows widget spike: timeline graph

**Status:** `success` (user-declared, 2026-10-04).

## Summary
Prove that a small Windows **gpui** window can render the `UI.md` widget — a
timeline graph with a past/forecast curve, a threshold line and a "now" marker —
using the built-in paint API. A viability spike, not the app.

## Hypothesis
Using **`gpui-ce` 0.2 + the `canvas`/`PathBuilder` paint API**, we can compile and
run on this Windows machine a single-window app that reproduces the `UI.md`
widget's graph (header + axes + past line + forecast line + threshold + now
divider) in one session, with **no** `gpui-component` dependency.

**Falsified if:** the toolchain cannot be installed/compiled here, `gpui-ce` fails
to open/render a window on Windows, or the hand-drawn line chart cannot be
produced acceptably.

## Methodology & blast radius
- Method:
  1. Toolchain: `winget install Rustlang.Rustup`, then VS 2022 Build Tools with
     `VC.Tools.x86.x64` + Windows SDK (MSVC toolchain, per gpui-ce `SETUP.md`).
     Verify `rustc -V` >= 1.95 and `cl.exe`.
  2. Scaffold: `cargo new` spike project, dep `gpui-ce = "0.2"`, nothing else.
  3. Widget: a view with a header (`Window: Close` left, `Cairo, Egypt` right) and
     a `canvas` painting the mockup — value axis (`N`) and time axis (`T`), past
     curve (solid), forecast curve (dotted), horizontal threshold line, vertical
     "now" divider, top-left legend mark (`Q`).
  4. Data: hardcoded sample series (EXP01 Cairo PM2.5 shape, ~14-26 ug/m3). No
     network in the app.
  5. Run + evidence: `cargo run`, screenshot the result, add it to this README.
- May touch: `experiments/EXP02/**`, `experiments-overview.md`, git (branch
  `EXP02`), cargo/rustup caches, `%TEMP%`.
- Off-limits: `UI.md`, `AGENTS.md`, `experiments/EXP01/**`; no app-final
  architecture decisions; no network calls; no declaring a final status.
- Machine-level blast radius (call-out): installing Rust + VS Build Tools +
  Windows SDK is a multi-GB, machine-wide change. First `cargo build` of the
  graphics stack is slow. This is the main risk/time cost of the spike.

## Open questions (known unknowns)
- [x] "igpu" meaning — resolved: typo for **gpui**.
- [x] Which gpui crate — resolved: **`gpui-ce 0.2.2` + `gpui_ce_platform 0.1.0`**
      (the latter is the app entry, and on Windows pulls the `gpui_ce_wgpu`
      backend). `gpui-ce` alone is core-only.
- [x] Chart approach — resolved: **hand-draw with `canvas` + `PathBuilder`**;
      kept the widget dependency-free (no `gpui-component`).
- [x] Data — resolved: **hardcoded sample series**; no network.
- [x] Window behavior — resolved: **normal window**; always-on-top + tray deferred.
- [x] Toolchain install — **deviated from default**: MSVC path blocked (no admin);
      used rustup's GNU host + portable MinGW-w64 instead (see Learnings).
- [x] Project location — resolved: **`experiments/EXP02/spike/`**.
- [x] Branch base — resolved: **`EXP02` from the `EXP01` tip**.

## Conclusion (findings)
**Concluded `success` by the user on 2026-10-04.**

**Hypothesis upheld.** gpui renders the `UI.md` widget on Windows. A single
`canvas` + `PathBuilder` view draws the whole timeline graph — grid, axes, PM2.5
threshold line, solid past curve, dashed forecast curve, and the "now" divider —
with no charting library. Evidence: `experiments/EXP02/EXP02-widget.png`;
code: `experiments/EXP02/spike/`.

- **Chosen stack:** `gpui-ce 0.2.2` + `gpui_ce_platform 0.1.0`, Windows backend
  `gpui_ce_windows` over **wgpu** (`gpui_ce_wgpu`). `gpui-ce` alone is core-only;
  the app entry `application()` lives in the platform crate.
- **Chart approach:** hand-drawing beat adding `gpui-component`; the widget's
  threshold/now/forecast decorations are trivial with the paint API and cost zero
  extra dependencies.
- **The plan's central risk (EXP01's "gpui viability") is answered: yes.** But the
  *supported* MSVC toolchain was unavailable (no admin), so the spike rode Rust's
  GNU host + a portable MinGW-w64. Caveat: the spike now depends on that
  non-standard toolchain until MSVC is installed.
- **Not proven (out of scope):** always-on-top overlay, system tray, and real
  data — still the next unknowns.

**Recommended next steps:** (a) install MSVC Build Tools + Windows SDK and
re-verify on the supported toolchain, or (b) spike always-on-top + tray, which
EXP01 flagged as the least-maintained fork features.

## Learnings
- **Non-admin gpui/Windows build:** MSVC + Windows SDK are absent and the shell is
  not elevated, so use rustup's **GNU host** (`stable-x86_64-pc-windows-gnu`) plus
  a portable **WinLibs MinGW-w64** extracted to `%LOCALAPPDATA%\mingw64`, with
  `%LOCALAPPDATA%\mingw64\bin` prepended to `PATH` when building. Rust's bundled
  self-contained MinGW lacks `as`/`windres` (breaks `windows-sys` raw-dylib);
  the `gnullvm` host lacks `clang`. Pin with a project `rust-toolchain.toml`.
  First full build ~9 min.
- **Crate set:** `gpui-ce` (lib name `gpui`) is **core only**; `application()`
  comes from `gpui_ce_platform` (alias `gpui_platform`); on Windows it pulls
  `gpui_ce_windows` + `gpui_ce_wgpu` (**wgpu**, pure-Rust `naga` shaders, no SDK
  `fxc`). `gpui_ce_platform` forces `gpui/windows-manifest`, which needs
  `windres`. Expect a harmless `ld.exe: .rsrc merge failure: multiple non-default
  manifests` link warning.
- **Drawing a chart:** `canvas(prepaint, paint)` + `PathBuilder::stroke(px(w))`
  (`.dash_array(&[px(a), px(b)])` for dashed) + `window.paint_path(path, color)`;
  filled rects via `window.paint_quad(fill(bounds, color))`. Compute geometry in
  `f32` and wrap with `px()`; keep axis labels as sibling `div`s.
- **Run + evidence:** launch `spike\target\debug\spike.exe` with the MinGW `bin`
  on PATH (the GNU build needs its runtime DLLs); `experiments/EXP02/capture.ps1`
  screenshots the window rect with `CopyFromScreen` (`PrintWindow` misses wgpu
  content).
