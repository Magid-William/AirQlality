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
**Recommended: success** (the user declares the status). One presentation-only pass
removed four things that carried no information and made three that did:

- **Header**: the source credit left the location row, so `Cairo, Egypt` is the
  only thing that brightens on hover; the header now holds the verdict, an
  optional issue chip, `API keys` and the location. `Ctrl+K` still opens it.
- **Credit**: `data (c) Open-Meteo` (or `data (c) N sources`) sits in the footer,
  where attribution belongs. Its tooltip says what Open-Meteo is (a free, keyless
  weather/air-quality API, CAMS forecast), names every source in the stack and
  points at NOTICE for licences. With only Open-Meteo answering (the user's case:
  no OpenAQ key, OWM key refused), the credit says exactly that.
- **Stale detail**: the long `showing last data ...` line is gone. A failed
  source while data is on screen is now an amber `N source issue(s)` chip beside
  the verdict; hover reads the full message, a screen reader gets it from the
  aria label, and activating it opens the refused key's prompt (or retries a
  network failure). With no data at all the message screen is unchanged.
- **Vocabulary**: the next-24-h census reads `N open / N vent / N closed`, the
  legend gained `closed >25`, and the VENT advice says "not clean" instead of
  "mediocre" - the band name now matches the headline in every place. (The
  internal `DayShape.mediocre` field keeps its historical name.)
- **Clock**: every visible time is 12-hour by default (`7:00 PM - 1:00 AM (7 h)`,
  `clean from 6 PM`, `Sun 3:00 PM`); clicking the best-window time flips the whole
  frame to 24-hour for the session, and it is a labelled button reachable with
  Tab + Enter. Deliberately not persisted: a relaunch is 12-hour again, which is
  what "always 12h" asked for.
- **Footer**: the right-hand `PM2.5 ug/m3 . past . forecast` key (and its stray
  leading `N`) is deleted. The unit is in `PM2.5 12 ug/m3 now`; the chart tooltip
  still says observed/forecast.
- **Space**: the stale row cost one `text_xs` line plus a gap (~20 logical px);
  the plot is `flex_1`, so those pixels went straight to the graph - the obvious
  win in the real error frame (see `EXP25-real-issue.png`).

Two defects surfaced while proving the interactions:

- **Location trigger Enter was broken (pre-existing, EXP10)**: gpui-ce already
  synthesises a click on keyup for Enter/Space on a focusable, clickable element,
  and the extra `on_key_down` toggled the panel twice - it opened and closed in
  the same keypress. The redundant handlers are removed from the trigger, the
  API-keys button, the clock and the issue chip; Enter now works on all of them.
- The test harness could post keys and mouse moves but not clicks, so the clock's
  pointer path was unprovable; `capture.ps1` gained `-ClickFracX/-ClickFracY`.

Not proven / manual: the 24-hour state is session-only by choice (no persistence
test); the hover-only location glow and both tooltips are proven by screenshots
(the built-in tooltip is mouse-only, so keyboard users get the issue detail from
the aria label, not visually); no x-axis hour labels were added (left as its own
experiment); the clock toggle has no E2E click test, only unit tests plus the
`-ClickFrac` screenshot.

Suite: core 64/64 (unchanged count; only the VENT advice string differs), app
65 -> 67 (+`clock`, +`issue_label`), windowed E2E 29/29 in 250 s (a11y tab stops
3 -> 4 and the clock button's label pinned). No pixel assertion was re-baselined:
the night/verdict counts kept their meaning, and the mock frame still paints no
green in the closed headline band.

Evidence: `EXP25-real-cairo.png` (hero, clean real frame),
`EXP25-real-issue.png` / `EXP25-real-issue-hover.png` (chip and its full
message), `EXP25-mock.png` (12-hour), `EXP25-mock-24h.png` (after the click),
`EXP25-mock-location-hover.png` (only the location brightens),
`EXP25-mock-credit-hover.png` (what Open-Meteo is), `EXP25-mock-stack.png`
(`data (c) 3 sources`), `EXP25-trigger-enter.png` (the Enter fix).

## Learnings
- `$env:PATH = "C:\Users\MW\AppData\Local\mingw64\bin;" + $env:PATH; cargo build -p airqlality` - the MinGW-W64 toolchain is under `%LOCALAPPDATA%\mingw64`; without it the link fails on `-lktmw32`.
- `cargo test -p airqlality --bin airqlality` - iterate on the view without flashing a window; run the windowed E2E suite once, at the end.
- gpui-ce synthesises a **click on keyup** for Enter/Space on a focusable, clickable element: handle activation with `on_click` only. A second `on_key_down` toggles twice (this hid in the EXP10 location trigger as "Enter does nothing").
- `capture.ps1 -ClickFracX/-ClickFracY` posts a real left click at a client-area fraction - use it to prove a click path; keys use `-Press "tab tab enter"`.
- 12-hour without a leading zero is chrono `%-I:%M %p` (`%I` would print `06:00 PM`).

