# EXP09 - Header cleanup: drop "Window", inline the status chip

## Summary
Remove the static `Window` label from the header and move the verdict headline
(`KEEP CLOSED` / `VENT` / `OPEN`) up into the header, on the same row as the
`data (c) Open-Meteo - Cairo, Egypt` location trigger.

## Hypothesis
The header renders as a single row — a coloured verdict chip on the left and the
attribution/location trigger on the right — with no `Window` text; removing the
banner's duplicate headline keeps the E2E pixel asserts green because the verdict
colour still paints in the top band (chip) and in the chart ribbon.

## Methodology & blast radius
- Method: edit `app/src/ui.rs` only.
  - `header()`: drop the `Window` child; add a left chip from `self.decision`
    (square in `verdict_color` + `headline.label()` in the same colour, bold),
    shown only when a verdict exists. Right side (attribution + caret) unchanged.
  - `render_banner()`: drop the now-duplicate headline row; keep the PM2.5 /
    best-window subline, the advice, and the stale/offline note.
- Fix found while running: `app/tests/capture.ps1` now raises the app window
  (SW_RESTORE + HWND_TOPMOST -> NOTOPMOST) before `CopyFromScreen`. Without it a
  background window can cover the app's left half, and because the verdict chip
  now lives in the header's left corner the pixel asserts saw no red. Assertions
  in `e2e_widget.rs` are untouched.
- May touch: `app/src/ui.rs`, `app/tests/capture.ps1`; `experiments/EXP09/`;
  `experiments-overview.md`.
- Off-limits: `core/`, the E2E assertions in `app/tests/e2e_widget.rs` (must pass
  unchanged), anything outside this project.
- Verify: `cargo test -p airqlality --test e2e_widget` (5 tests) still passes;
  screenshots of the closed and open states into `experiments/EXP09/`.

## Open questions (known unknowns)
- [x] Chip text size — resolved: user asked for it bigger, so the chip uses the
  previous banner size (`text_xl`, bold) with a matching 12 px square; it fits the
  36 px header.
- [ ] No-verdict states (first load / fetch error) — assumed: header-left stays
  empty; the loading/error screens are otherwise unchanged.
- [ ] Left vs right — assumed: chip on the left (matching UI.md's `Window: Close`
  position); the location trigger stays on the right.

## Conclusion (findings)
_Pending._

## Learnings
- `Get-Process airqlality -ErrorAction SilentlyContinue | Stop-Process -Force`
  before `cargo test` — use when the build fails with `failed to remove file
  ...target\debug\airqlality.exe: Access is denied (os error 5)` (a previous
  capture left the app running).
- `capture.ps1` must raise the window with a HWND_TOPMOST -> HWND_NOTOPMOST
  `SetWindowPos` toggle before `CopyFromScreen` — use when E2E pixel counts come
  back low / `bg` roughly halved and red is missing in the header band: another
  window was overlapping the app, and `SetForegroundWindow` alone silently failed
  because this process does not hold the foreground lock.
