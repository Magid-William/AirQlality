# EXP04 - E2E test harness: launch the real widget on mock data, assert pixels

**Status:** `success` (user-declared, 2026-10-04).

## Summary
Add one unattended end-to-end test that runs the real `airqlality.exe` on the
deterministic mock source, screenshots its Windows window, and asserts the rendered
verdict by exact colour — so one command proves the whole mock -> core -> UI ->
render chain, not just `core`.

## Hypothesis
The real binary, launched unattended with the deterministic mock, yields a
screenshot whose exact theme colours encode the expected result (red `#f85149`
KEEP CLOSED banner, green `#3fb950` open-ribbon cells, amber `#d29922` vent cells),
so an automated test can assert the full chain and fail loudly on broken
rendering or wiring.

**Falsified if:** capture is non-deterministic / colour-shifting across runs, the
assertion cannot distinguish a correct from a broken render, or it cannot run
unattended in one command.

## Methodology & blast radius
- Method:
  1. `app/tests/e2e_widget.rs`: a Rust integration test that locates the freshly
     built binary via `env!("CARGO_BIN_EXE_airqlality")` and shells out to a
     committed capture helper.
  2. `app/tests/capture.ps1`: a generalised copy of EXP02's proven
     `CopyFromScreen` capture (`PrintWindow` misses GPU content). It prepends the
     MinGW runtime to `PATH`, launches the exe, waits, screenshots the window
     rect, prints `HWND`/`RECT`/`SAVED`, then kills it.
  3. The test decodes the PNG (dev-dep `image`, already cached) and asserts:
     - **smoke:** no `PROCESS_EXITED`, `HWND` non-zero, `RECT` ~560x340 scaled,
       PNG produced;
     - **headline band (top ~30%):** contains CLOSED red, contains **no** OPEN green;
     - **totals:** CLOSED >= 300, OPEN >= 200, VENT >= 300, BG >= 50k (calibrated
       from EXP03).
  4. One command: `cargo test -p airqlality --test e2e_widget`. Unit level stays
     `cargo test -p airqlality-core`.
  5. Bite-proof: temporarily flip a threshold so the headline is no longer
     `KEEP CLOSED`, show the E2E fails, revert; keep both logs as evidence.
- May touch: `app/tests/**` (new), `app/Cargo.toml` (dev-dep `image`),
  `experiments/EXP04/**`, `experiments-overview.md`, branch `EXP04` from `6f2c6fb`.
- Off-limits: `UI.md`, `AGENTS.md`, `core/**` rule semantics (thresholds/logic
  unchanged), `experiments/EXP01..03/**` (frozen evidence), any network call,
  declaring a final status.
- Machine-level blast radius: none new — reuses the cached GNU-toolchain build;
  the test briefly opens and kills one real window.

## Open questions (known unknowns)
- [x] Test form — **Rust integration test** in `app/tests/` so `cargo test` owns
      it (plan default).
- [x] Capture mechanism — **reuse `CopyFromScreen`, committed at
      `app/tests/capture.ps1`**; do not depend on the frozen EXP02 artifact
      (plan default).
- [x] Assertion strictness — **region + colour presence + count floors**, not
      exact coordinates (display is 128% DPI); exact verdicts stay pinned by
      `core` unit tests (plan default).
- [x] Mock scenarios — **single deterministic scenario** (current `MockSource`);
      injectable multi-scenario is a separate experiment needing an app seam
      (plan default).
- [x] Regression bite-proof — **yes**, flip a threshold, capture the failing run,
      revert (plan default).
- [x] Headless logical layer (gpui `TestAppContext`/`debug_bounds`) — **out of
      scope**: `gpui_ce_platform::current_headless_renderer()` returns `None` on
      Windows, so no headless pixel path exists (plan default).
- [x] Base branch — **`EXP04` from `EXP03` tip `6f2c6fb`** (plan default).

## Conclusion (findings)
**Concluded `success` by the user on 2026-10-04.**

**Hypothesis upheld.** The real `airqlality.exe`, launched unattended on the
deterministic mock, renders a screenshot whose exact theme colours encode the
expected end-to-end result, and the assertion distinguishes a correct render from
a broken one.

- **One command:** `cargo test -p airqlality --test e2e_widget -- --nocapture`
  (with the toolchain on `PATH`) builds the bin, launches it, screenshots the
  window, and asserts — 1 passed in ~11 s.
- **Deterministic oracle:** live counts are byte-identical to the EXP03 capture
  (`closed=2043 vent=6293 open=790 bg=272362`), so exact-colour counting needs no
  tolerance. `top_closed=1131, top_open=0` pins the KEEP CLOSED banner.
- **It bites:** flipping `Thresholds::default().open_max` to `30.0` turned the
  banner green (`top_closed=0, top_open=646`) and failed the test at the headline
  assertion; reverting restored green. The E2E catches a real rule/wiring
  regression, not just a blank window.
- **Unit level intact:** `airqlality-core` still 4/4.
- **Evidence:** `experiments/EXP04/EXP04-e2e.png`; code `app/tests/`.
- **Not proven (out of scope):** multiple mock scenarios (clean day -> OPEN,
  no-clean-window) need an injectable source seam — a separate experiment; the
  gpui headless logical layer is unavailable on Windows (no headless renderer);
  there are no user interactions to drive (window only).

**Recommended next:** the single-scenario seam is now covered; the higher-value
next step is still EXP03's recommendation — a real `OpenMeteoSource` behind the
existing trait, with this E2E extended to assert error/loading states.

## Learnings
- **Run the E2E:** prepend the toolchain to `PATH`, then
  `cargo test -p airqlality --test e2e_widget -- --nocapture`.
- **Fresh bin in integration tests:** `env!("CARGO_BIN_EXE_airqlality")` yields the
  just-built binary; `cargo test` builds it, so no manual build/copy.
- **Pixel counts are deterministic:** reproducing the EXP03 screenshot exactly
  (`closed=2043 vent=6293 open=790 bg=272362`, 718x472 @ 128% DPI) makes
  exact-colour counting a stable oracle with no tolerance. Needs `CopyFromScreen`
  (in `tests/capture.ps1`) and MinGW `bin` on `PATH`.
- **PNG decoding:** `image` 0.25.10 is already in `Cargo.lock` via gpui; add it as
  a dev-dependency with `default-features = false, features = ["png"]`.
- **Harmless noise:** the GNU link warns `ld.exe: .rsrc merge failure: multiple
  non-default manifests`; ignore it (same as EXP02/EXP03).
