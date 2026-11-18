# EXP31 - The tray icon survives a taskbar replacement

**Status:** `success` (user, 2026-11-18).

## Summary
Register `TaskbarCreated` on the tray thread and re-add the icon when it
arrives, so a restarted Explorer - or a bar that mimics it, like
komorebi-bar - still shows AirQlality.

## Hypothesis
The tray window is created once and `NIM_ADD` is called once
(`app/src/tray.rs::run` -> `add_icon`). It never registers
`RegisterWindowMessageW("TaskbarCreated")`, so it cannot react when the shell
says every icon was dropped. komorebi-bar's systray widget (via
`systray-util`) cannot enumerate existing icons on Windows 11, so it
broadcasts `TaskbarCreated` at startup exactly like Explorer does after a
restart or a primary-display DPI change, then shows whoever re-adds.

Therefore, if the tray thread registers `TaskbarCreated` and re-runs
`NIM_ADD`/`NIM_SETVERSION` when it arrives, then:

- a broadcast makes the icon reappear in komorebi-bar's systray widget, and
  the app stays in the native tray after an Explorer restart / DPI change;
- the re-added icon carries the live colour and tooltip from `STATE` (and a
  queued balloon, if any), not the launch-time ones;
- the one-shot test seams fire once, not once per re-add.

**Falsified if:** after a broadcast the dump shows a second request but a
non-`ok` result; or a fresh `NIM_ADD` while komorebi-bar runs is still
invisible (then the bar misses our live `WM_COPYDATA` too, and re-announcing
alone cannot fix it).

## Methodology & blast radius

- **How the bar sees icons** (research, not a code change): `systray-util`
  creates a hidden top-level window with the `Shell_TrayWnd` class, keeps it
  topmost, intercepts the `WM_COPYDATA` that `Shell_NotifyIcon` sends to the
  taskbar, and forwards it to the real taskbar. Its startup path
  (`refresh_icons`) broadcasts `TaskbarCreated`; that is why only apps that
  handle the message reappeared in the user's capture.
- **Method**
  1. `app/src/tray.rs`: register `TaskbarCreated` in `run()` into a
     `static TASKBAR_CREATED: AtomicU32`; a pure
     `is_taskbar_created(msg, registered)` predicate (a failed registration
     returns 0 and must not match) with unit tests.
  2. `wndproc`: on that message call `add_icon(hwnd)` (`NIM_ADD` +
     `NIM_SETVERSION`, `flush_pending`), then return 0. No retry timer: the
     bar's spy is already up; `ok` in the dump would expose a failure.
     **Found in the E2E:** a broadcast sent while the real shell still holds
     the icon is refused by `NIM_ADD` (the icon was never dropped), so
     `add_icon` falls back to `NIM_MODIFY` with the same `NOTIFYICONDATAW`.
     `systray-util` treats an update of an icon it does not know as an add, so
     komorebi-bar still shows it (and gets its icon image, callback and
     version), while Explorer repaints the existing icon in place.
  3. `add_icon`: run `maybe_spawn_quit_seam` / `maybe_spawn_refresh_seam` /
     `startup::maybe_toggle_seam` only on the first successful add
     (`TRAY_READY.swap(true, ..)` instead of `store`), so a re-add cannot
     start a second test seam.
  4. `dump`: add `"adds":N`, incremented on every `NIM_ADD` attempt, so the
     E2E can prove a re-add happened (later `NIM_MODIFY` dumps keep the
     counter).
  5. New seam `AIRQLALITY_TRAY_TASKBAR_CREATED_MS=<n>`: after `n` ms,
     broadcast the same registered message from the app
     (`SendNotifyMessageW(HWND_BROADCAST, ..)`), which is exactly what
     Explorer and `systray-util` do.
  6. Windowed E2E: mock frozen, `AIRQLALITY_TRAY_DUMP`, seam at ~1500 ms;
     poll the dump until `"adds":2`, then assert `"ok":true`, the same
     `#f85149` colour and `KEEP CLOSED` tooltip. (A later clock repaint may
     overwrite the dump, so the counter is the stable assertion.)
  7. Unit + E2E gates; full `e2e_widget` once at the end.
- **May touch:** `app/src/tray.rs`, `app/tests/e2e_widget.rs`,
  `experiments/EXP31/**`, `experiments-overview.md`.
- **Off-limits:** `core/**`, the UI, installer, release workflow, EXP01..EXP30,
  declaring a final status.
- **Machine-level:** no new crates, no new `windows` feature (the message APIs
  are in the already-enabled `Win32_UI_WindowsAndMessaging` group), no
  downloads. The E2E sends one `TaskbarCreated` broadcast per run, the same
  event Explorer sends on a DPI change.

## Open questions (known unknowns)

- [ ] Retry if the first `NIM_ADD` after the broadcast fails? - proposed
  default: **no retry timer**; the dump records `ok`, and a `SetTimer` retry is
  only worth adding if a real Explorer restart proves flaky.
- [ ] Broadcast or post only to our tray window in the E2E? - proposed
  default: **broadcast**, because delivery is part of what is under test and it
  is what the bar does. *(Assumption adopted.)*
- [ ] Anything beyond `TaskbarCreated` (e.g. `WM_DISPLAYCHANGE`)? - proposed
  default: **no**; Windows 10/11 re-broadcast `TaskbarCreated` on a
  primary-display DPI change, so the same handler covers it.
- [ ] Version bump / release? - proposed default: **no**, separate user-declared
  step.

## Conclusion (findings)
**Success (user, 2026-11-18).** The tray icon is no longer a one-shot: it
re-registers when the taskbar says it was recreated, which is the only thing an
app can do when komorebi-bar (or Explorer) starts with an empty icon table.

What landed:
- `app/src/tray.rs`: `TaskbarCreated` registered on the tray thread; `wndproc`
  re-runs `add_icon` from the live `STATE` (colour, tooltip, pending balloon).
  `NIM_ADD` refused because the real shell still holds the icon (a
  bar-initiated broadcast) falls back to `NIM_MODIFY` with the same data;
  `systray-util` turns an unknown-icon update into an add, so the bar shows it.
  One-shot seams now fire on the first successful add only; the dump carries
  `adds`.
- `app/tests/e2e_widget.rs`: `widget_e2e_tray_survives_taskbar_created`; the
  `AIRQLALITY_TRAY_TASKBAR_CREATED_MS` seam broadcasts the real message.
- Evidence: `EXP31-taskbar-created.json` =
  `{"color":"#f85149","tip":"...KEEP CLOSED","ok":true,"adds":2}`; app unit
  87/87; new E2E green; full windowed suite 34/36 - the two failures are the
  pre-existing komorebi tiling conflict (proven on `main` with EXP31 stashed),
  not this change.

Limits / not proven:
- Re-announcing only helps apps that handle the message. The bar still cannot
  enumerate icons added before it started, so other apps that ignore
  `TaskbarCreated` stay hidden until they restart.
- No retry timer: if a future shell refuses both `NIM_ADD` and `NIM_MODIFY` at
  broadcast time, the icon stays gone and the dump says `ok:false`.
- The field acceptance is the user's, not a capture of the bar: komorebi-bar is
  outside this repo, so no screenshot of its tray is part of the evidence.

## Learnings
- `Shell_NotifyIcon` reaches the taskbar by messaging the window with the
  `Shell_TrayWnd` class; komorebi-bar (`systray-util`) inserts a topmost spy
  window with that class, relays the `WM_COPYDATA`, and on Windows 11 learns
  existing icons only by broadcasting `TaskbarCreated` at startup. Ignore the
  message and the bar shows nothing.
- When the shell still holds the icon, `NIM_ADD` returns FALSE; follow with
  `NIM_MODIFY` and the same `NOTIFYICONDATAW`. `systray-util` maps an update of
  an unknown icon to an add, so the re-announce works for a bar that never saw
  the original add. Without it the dump reads `{"adds":2,"ok":false}`.
- Gates: `cargo test -p airqlality --bin airqlality` (87/87) and
  `cargo test -p airqlality --test e2e_widget -- --nocapture
  widget_e2e_tray_survives_taskbar_created` (~15 s, prints the dump). Under
  komorebi the widget is tiled to 1296x1420, so the strict geometry/pixel tests
  fail on `main` too; add a komorebi ignore rule for `airqlality.exe` or pause
  komorebi before the full suite.
- Do not run `cargo fmt --all` here: this machine's rustfmt rewrites ~23
  untouched files. It was run once and fully reverted; keep the style by hand.
