# EXP13 - Hover tooltip on the timeline

## Summary
Hovering the chart shows the hour under the pointer: a crosshair + a small panel
with the time (weekday, 12-hour with am/pm) and the PM2.5 value plus that hour's
verdict. Mouse-only; the normal summary stays unchanged.

## Hypothesis
The gpui-ce `canvas` paint space is window-absolute (the existing `draw_timeline`
already computes `ox = bounds.origin.x`), so the app can hit-test the pointer
against the chart by storing the canvas `Bounds` from the paint closure and
mapping `event.position.x -> nearest hour`. A `Div::on_mouse_move` on the
`#timeline` wrapper, an absolute tooltip element positioned from the stored
bounds, and a crosshair/dot painted inside `draw_timeline` give a tooltip that
tracks the pointer - with no new crate.

**Falsified if:** a posted `WM_MOUSEMOVE` does not reach `Div::on_mouse_move`
(hover hit-test needs a physical pointer); the tooltip cannot be positioned
without flicker; or the existing no-hover screenshot changes
(`closed=2043 open=790 panel=0`).

## Methodology & blast radius
- Method (app-only, `core/**` untouched):
  1. `app/src/ui.rs`: extract the geometry inlined in `draw_timeline` into a
     small pure `ChartGeom { left, top, right, chart_bottom, pw, ph, ribbon_h,
     y_max, n }` with `x_at(i)`, `y_at(v)`, and `index_at_x(x) -> Option<usize>`
     (`None` outside `[left,right]`); `draw_timeline` and the hit-test share it.
  2. Widget state: `hover: Option<usize>` and `chart_bounds:
     Rc<Cell<Bounds<Pixels>>>` (last painted canvas bounds; `Bounds` is `Copy`),
     set inside the canvas paint closure.
  3. `#timeline`: `.cursor_crosshair()`, `.on_mouse_move` (index from
     `chart_bounds` + `event.position`, `cx.notify()`) and `.on_hover` to clear
     `hover` when the pointer leaves.
  4. `draw_timeline`: with a hovered hour, paint a vertical crosshair and a dot
     on the curve (verdict-coloured).
  5. Tooltip: an absolutely positioned child at the root (same space the
     dropdown uses), `left` clamped/flipped at the chart edges; lines: `Sun
     02:00 PM · forecast|observed`, `PM2.5 26 µg/m³`, the verdict word.
- Verify: `cargo test -p airqlality-core` (21/21 unchanged); new app unit tests
  on `ChartGeom::index_at_x` (edges/middle/outside/n<2); E2E baseline unchanged
  (`panel=0`) plus a new case that posts a real `WM_MOUSEMOVE` over the chart
  and asserts the tooltip `PANEL` pixels appear.
- Test seam: `app/tests/capture.ps1` gains `-MouseFracX`/`-MouseFracY`
  (fractions of the client area) that post `WM_MOUSEMOVE (0x0200)`.
- Evidence into `experiments/EXP13/`: a hovered screenshot (tooltip visible) and
  a no-hover screenshot (tooltip absent), from a real posted mouse move.
- May touch: `app/src/ui.rs`, `app/src/theme.rs`,
  `app/tests/{capture.ps1,e2e_widget.rs}`, `experiments/EXP13/**`,
  `experiments-overview.md`, branch `EXP13` (from `60ac511`).
- Off-limits: `core/**`, `app/src/{tray,startup,persist}.rs`, `app/Cargo.toml`,
  `UI.md`, `AGENTS.md`, `experiments/EXP01..12/**` (frozen evidence),
  declaring a final status.

## Open questions (known unknowns)
- [x] Time format - **user decision: weekday + 12-hour with am/pm**
  (`%a %I:%M %p`, e.g. `Sun 02:00 PM`).
- [x] Scope - **user decision: mouse-only**; the chart keeps its existing
  `Role::Image` text alternative; a keyboard cursor is future work.
- [x] Tooltip content - default: time + forecast/observed tag, PM2.5 (µg/m³),
  verdict word (verdict colour). `pm10`/`US AQI`/`wind` remain unsurfaced.
- [x] Trigger zone - default: anywhere over the canvas; `x` maps to the nearest
  hour, clamped to first/last.
- [x] Visual anchor - default: full-height crosshair + dot on the curve + panel
  pinned near the chart top, flipping at the edges.
- [x] Test seam - default: real posted `WM_MOUSEMOVE` (proves the hit-test);
  `ChartGeom` unit tests cover the pure math; no new crate.

## Conclusion (findings)
**Concluded `success` by the user on 2026-10-05.**

**Hypothesis upheld.** Hovering the timeline maps the pointer to the hour under
it and shows a tooltip with the time (weekday + 12-hour am/pm), the PM2.5 value,
and that hour's verdict - app-only, no new crate, no change to the no-hover view.

- **Hit-testing.** The canvas paint space is window-absolute (the pre-existing
  `draw_timeline` already used `bounds.origin` as the anchor). A pure `ChartGeom`
  now owns the geometry and `index_at_x`; the paint closure stores the canvas
  `Bounds` in an `Rc<Cell<Bounds<Pixels>>>`, and the `#timeline` wrapper's
  `on_mouse_move` maps `event.position.x -> nearest hour` (`None` outside the
  plot). This is pure and unit-tested, not a guess.
- **Render.** A full-height crosshair (`theme::HOVER`) + a verdict-coloured dot
  are painted inside `draw_timeline`; the text panel is a plain absolute child of
  the root, `left` centred on the hour and clamped/flipped at the chart edges.
  The tooltip div has no listeners/styles that create a hitbox
  (`Div::should_insert_hitbox`), so it cannot steal hover or flicker.
- **Time format.** `%a %I:%M %p` -> `Sun 03:00 PM`; the weekday disambiguates the
  48 h window across midnight, and a `forecast`/`observed` tag marks the split.
- **Scope.** Mouse-only, as decided; the chart keeps its `Role::Image` text
  alternative unchanged, so assistive tech is unaffected.
- **Tests.** core 21/21; app 16/16 (4 new `ui` `ChartGeom::index_at_x`: edges,
  middle, outside, n<2); e2e 16/16 (1 new hover case). No-hover pixel counts
  unchanged (`closed=2043 open=790 panel=0`). In the hovered screenshot
  `panel=14053 hover=146`; tooltip centre 365 vs crosshair column 366.
- **Evidence.** `EXP13-hover.png` (tooltip + crosshair over the middle hour, from
  a real posted `WM_MOUSEMOVE`) and `EXP13-no-hover.png` (nothing on hover),
  both on the frozen mock.
- **Not proven (out of scope):** keyboard/AT parity for the per-hour detail (the
  chart is still one image to a screen reader); `pm10`/`US AQI`/`wind` are not
  surfaced; tooltip placement on a hypothetical very narrow window is only
  clamped, not re-flowed; touch/pen input.

**Recommended next:** keyboard cursor for the `Role::Image` chart (Left/Right +
the same tooltip), then the long-deferred disk cache so a restart shows the last
good series offline.

## Learnings
- **Hit-test a gpui `canvas`:** paint `Bounds` are window space (same as
  `MouseMoveEvent.position`). Store them from the paint closure in an
  `Rc<Cell<Bounds<Pixels>>>`; a `Div::on_mouse_move` on the wrapper maps
  `x -> index`. The paint closure can't `cx.notify`, and a one-frame lag is
  invisible. Factor geometry into a pure struct so draw + hit-test agree.
- **A plain absolute `div()` tooltip does not steal hover** (no hitbox unless it
  has listeners/`mouse_cursor`/hover style; see `Div::should_insert_hitbox`), so
  it needs no `occlude` and does not flicker.
- **12-hour time (chrono):** `time.format("%a %I:%M %p")` -> `Sun 03:00 PM`.
- **E2E a real hover without a pointer:** `PostMessage(hwnd, WM_MOUSEMOVE=0x0200,
  0, (y<<16)|x)`; the Windows backend reads `lParam` directly. `lParam` is
  *client* coords, so pass fractions (`-MouseFracX/-MouseFracY` in
  `capture.ps1`) computed from `GetClientRect` to stay DPI-independent.

