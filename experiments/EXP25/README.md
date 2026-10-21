# EXP25 - UI clarity pass: fewer rows, one vocabulary, readable times

## Summary
Strip chrome that informs nobody: source credit to the footer with a hover explanation, rejected-key detail to an on-demand header chip, one verdict vocabulary, 12-hour times with a click-to-24-hour toggle, and no redundant axis key.

## Hypothesis
In the same 560x340 frame the header contains only interactive things (plus one small issue chip while a source is down), every visible time is 12-hour until clicked, the stale detail is reachable on demand instead of eating ~16 px of plot height, and no verdict information is lost.

## Methodology & blast radius
- Method: presentation-only changes in `app/src/ui.rs`. Pure helpers for clock formatting, census wording, the credit label and the issue chip; gpui's built-in `tooltip` (present in gpui-ce 0.2.2, needs one tiny `Render` view) for the footer credit and the chip. A11y labels carry the full text, so nothing lives only in a hover. No data, threshold or key-flow change.
- May touch: `app/src/ui.rs`, its unit tests, `app/tests/e2e_widget.rs` (tab stops, a11y needles), `core/src/decision.rs` (`Verdict::advice` wording only), `experiments/EXP25/`, `experiments-overview.md`.
- Off-limits: thresholds/classifier, keys flow and persistence, tray/notifications, window size, theme palette, existing screenshots and baselines.

## Open questions (known unknowns)
- [x] Number/branch? - EXP25; no later experiment exists.
- [x] Where the source credit lives? - footer right, visible `data (c) Open-Meteo` (or `data (c) N sources`), hover tooltip explains what the API is and names every source in the stack. Attribution stays visible because the data is CC BY 4.0.
- [x] Stale detail? - header chip `N source issue(s)`, amber, only while data is on screen and a source failed; hover/focus reads the full message; click opens that source's key prompt for a rejected key, otherwise retries. No banner row. The no-data error screen is unchanged.
- [x] Census vocabulary? - `next 24 h: N open . N vent . N closed`, matching the headline and the legend; the legend adds `closed >25`; `Verdict::advice` says "not clean" instead of "mediocre".
- [x] Clock? - 12-hour everywhere (`6 PM - 1 AM (8 h)`, `clean from 6 PM`, tooltip `Sun 3:00 PM`). Clicking the best-window time switches to 24-hour for the session only; a relaunch is 12-hour again. The toggle is a labelled tab stop.
- [x] Footer axis key? - remove the whole right-hand string (`PM2.5 ug/m3 . past . forecast`, including the stray `N`). The unit is already in `PM2.5 16 ug/m3 now`; the chart tooltip says observed/forecast.
- [x] Chart hour labels? - out of scope: chart geometry, its own experiment.
- [x] Re-baseline? - new evidence screenshots under `experiments/EXP25/`; the a11y `tab_stop_count` goes 3 -> 4 and the census unit strings change; pixel-count assertions stay in kind.

## Conclusion (findings)
_Pending._

## Learnings
- `$env:PATH = "C:\Users\MW\AppData\Local\mingw64\bin;" + $env:PATH; cargo build -p airqlality` - the MinGW-W64 toolchain is under `%LOCALAPPDATA%\mingw64`; without it the link fails on `-lktmw32`.
- `cargo test -p airqlality --bin airqlality` - iterate on the view without flashing a window; the windowed E2E suite runs once, at the end.
- gpui-ce already synthesizes a **click on keyup** for Enter/Space on a focusable, clickable element. Do not also handle Enter in `on_key_down`: the clock toggled twice (state true, then a synthetic click set it false). This also means the EXP10 location trigger was double-toggling (Enter opened and immediately closed the panel); removing the redundant handler fixed it.
- `capture.ps1 -ClickFracX 0.5 -ClickFracY 0.185` posts a real left click at a fraction of the client area - use it to exercise a clickable control and screenshot the result (the ClickFrac pair was added in EXP25).
