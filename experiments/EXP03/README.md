# EXP03 - App scaffold + mock-data core (verdict engine + timeline)

**Status:** `success` (user-declared, 2026-10-04).

## Summary
Replace the single-file EXP02 spike with the real app skeleton: a Cargo workspace
splitting a pure, testable `core` (domain model, data-source trait, mock provider,
transparent verdict rule) from the gpui `app`. The timeline renders a real verdict.
Mock data only — no network.

## Hypothesis
A two-crate seam (pure `core` + gpui `app`) lets a mock Cairo provider yield hourly
points and a **documented** open/vent/closed rule produce a per-hour verdict plus a
"best window today", which the EXP02 timeline canvas renders unchanged. Swapping the
mock for Open-Meteo later should touch only the source implementation.

**Falsified if:** `core` cannot stay gpui-free, the verdict cannot be computed from
mock data, or the UI cannot be wired and rendered in one session.

## Methodology & blast radius
- Method:
  1. Layout: root Cargo workspace with `core/` (lib) + `app/` (bin); carry EXP02's
     GNU toolchain pin and `gpui-ce`/`gpui_ce_platform` deps into `app/`.
  2. `core`: `model` (`AirQualityPoint { time, pm2_5, pm10, us_aqi, wind_speed }`,
     series split past/forecast at "now"), `source` (`AirQualitySource` trait +
     deterministic `MockSource` shaped from the EXP01 Cairo probe, ~48 h), `decision`
     (transparent threshold rule -> per-hour `Verdict::{Open, Vent, KeepClosed}`,
     `best_window()`, documented constants), plus unit tests.
  3. `app`: window + `ui/timeline` canvas adapted from the spike, a verdict banner
     (verdict word + best window + a one-line "why"), and a legend; header keeps
     location/close.
  4. Verify: `cargo test` for `core`; `cargo run` + screenshot via the existing
     `experiments/EXP02/capture.ps1`.
- May touch: new root `Cargo.toml`/`Cargo.lock`, `core/**`, `app/**`,
  `experiments/EXP03/**`, `experiments-overview.md`, `.gitignore`, git branch `EXP03`.
- Off-limits: `UI.md`, `AGENTS.md`, `experiments/EXP01/**`, `experiments/EXP02/**`
  (spike stays frozen as evidence), any network call, declaring a final status.
- Machine-level blast radius: adds a second GNU-toolchain gpui build; first build
  ~9 min, then cached.

## Open questions (known unknowns)
- [x] Layout — assumed **workspace, `core` lib + `app` bin**, so the "transparent
      rule" differentiator is unit-testable and gpui-free (plan default).
- [x] Location — assumed **repo root** (`core/`, `app/`); product code, not an
      experiment artifact (plan default).
- [x] Rule thresholds — assumed **Open <= 15 ug/m3 (WHO 24 h), Vent 15-25 (brief,
      CO2 caveat), KeepClosed > 25**; documented in code (plan default).
- [x] Wind — assumed **wind field in the model, not driving the verdict yet**
      (PM2.5-only rule this experiment) (plan default).
- [x] Horizon/granularity — assumed **48 h hourly (24 past + 24 forecast)**,
      matching `UI.md` (plan default).
- [x] Time dep — assumed **`chrono`**; reuse pinned `gpui-ce` + `gpui_ce_platform`;
      no chart/UI libs (plan default).
- [x] Window behavior — assumed **normal window**, tray/always-on-top deferred
      (plan default).
- [x] Base branch — assumed **`EXP03` from the `EXP02` tip** (`fe20b75`) (plan
      default).
- [x] Evidence tooling — assumed **reuse `experiments/EXP02/capture.ps1`** (plan
      default).

## Conclusion (findings)
**Concluded `success` by the user on 2026-10-04.**

**Hypothesis upheld.** The two-crate seam holds: `core` is gpui-free and fully
unit-tested, the mock provider yields hourly points, the documented rule produces a
per-hour verdict + best window, and the app renders it with the EXP02 canvas
approach intact. Swapping data sources is a one-line change in `Widget::new()`.

- **Workspace:** root `Cargo.toml` with `core` (lib) + `app` (bin),
  `exclude = ["experiments"]`. `core` = `model` / `source` (`AirQualitySource` +
  `MockSource`) / `decision`; **4/4 unit tests pass**.
- **Rule (documented, in code):** Open ≤ 15 µg/m³ (WHO 24 h), Vent 15–25 (CO₂
  caveat), KeepClosed > 25; `best_window` = longest Open run from "now".
- **Mock verdict:** "now" 26 µg/m³ → `KEEP CLOSED`; best window 02:00–06:00
  (indices 35–39, 5 h). Verified in the running app.
- **UI:** header, verdict banner (word + best window + one-line "why"), hand-drawn
  timeline (solid past / dashed forecast, threshold line, "now" divider, per-hour
  verdict ribbon), legend.
- **Release:** builds and renders identically; **26.6 MB vs 460 MB debug**
  (`[profile.release] debug=false`, `strip=true`).
- **All plan defaults held** (no open question needed a change). Wind is carried in
  the model but does not affect the verdict yet.
- **Not proven (out of scope):** real network data, error/loading states,
  always-on-top, tray, indoor CO₂ — the source is still `unwrap()`ed.
- **Corrected a prior label:** `gpui_ce_platform`'s `wgpu` feature is **off by
  default**, so the renderer is **DirectX**, not wgpu (EXP02 called it wgpu).

**Recommended next:** implement an `OpenMeteoSource` behind the same trait (real
data, error handling, caching) — EXP01 ranks Open-Meteo AQ as the primary source;
then the always-on-top + tray frontier EXP01 flagged.

## Learnings
- **Rust is not on `PATH` here.** Prepend
  `$env:USERPROFILE\.cargo\bin;$env:LOCALAPPDATA\mingw64\bin` before cargo; host is
  `stable-x86_64-pc-windows-gnu` (rustc 1.99).
- **Build / test / run / evidence:**
  `cargo test -p airqlality-core`;
  `cargo build -p airqlality` → `target\debug\airqlality.exe`;
  `cargo build --release -p airqlality` → `target\release\airqlality.exe`;
  run either with MinGW `bin` on PATH, then
  `experiments\EXP02\capture.ps1 -Exe <exe> -Out <png>`.
- **Release needs `fxc.exe`** only because `gpui_ce_windows` precompiles its DirectX
  HLSL when `debug-assertions` are off (debug compiles them at runtime via
  `D3DCompileFromFile`). No Windows SDK here ⇒ keep
  `[profile.release.package.gpui_ce_windows] debug-assertions = true`; every other
  crate stays a true release.
- **Backend:** `gpui_ce_platform`'s `wgpu` feature is off by default ⇒ DirectX
  renderer (`gpui_ce_windows` + `directx_renderer`).
- **gpui API notes:** `PathBuilder` and `AppContext` must be imported explicitly
  (not in `prelude::*`); style helpers (`px_4`, `gap_2`, `size_full`, …) are
  macro-generated in `gpui_ce_macros`'s `styles.rs`, not literal `fn`s.
- **Sizes:** debug exe ≈ 460 MB, release ≈ 26.6 MB.
