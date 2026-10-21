# EXP22 - Tray "Refresh": recover a failed fetch without a restart

**Status:** `success` (user-declared, 2026-10-21).

## Summary
The tray's right-click menu gains **Refresh**, so a widget stuck on
"COULDN'T FETCH" (or showing stale data behind an `offline -` note) can be
recovered the moment the user notices, instead of waiting for the next
`:00`/`:30` boundary or restarting the app.

## Hypothesis
A tray item that wakes the widget's real fetch loop **immediately** makes a
broken fetch recoverable in place: point the source at a server that fails the
first request and succeeds the second, click Refresh, and real data appears
without touching the schedule.

**Falsified if:**
- the click cannot reach the widget inside one capture window (~6 s), or the
  only way to reach it is a polling timer;
- the data only appears at the next wall-clock boundary (the refresh was not
  immediate);
- a manual refresh re-fires the `KEEP CLOSED` balloon on unchanged data;
- any pre-EXP22 frame moves: the frozen mock keeps `closed=2043 open=790`,
  `panel=0`, `night=0`.

## Methodology & blast radius
- **The boundary.** The tray is a dedicated Win32 thread (EXP11) and may not
  touch gpui state; today it only reads/writes statics and posts window
  messages. The request therefore travels one new way: a `bounded(1)`
  `async-channel` per widget. `async-channel` 2.5.0 is **already in `Cargo.lock`
  via `gpui-ce`**, so this adds no download (the EXP11 `windows` precedent).
- **New `app/src/refresh.rs`.** A single `static Mutex<Option<Sender<()>>>` and
  `publish`, `available`, `request`. The interesting logic is a pure
  `deliver(&Option<Sender<()>>) -> bool`, so "delivers once / coalesces / no
  listener is a no-op / a dead listener is a no-op" are unit tests with no
  globals and no threads. Bounded(1) + `try_send` is the coalescing: ten clicks
  during a slow fetch leave exactly one pending refresh.
- **`Widget`.** `new()` publishes the sender **only in real mode** (so the item
  is inert and greyed under the mock seams) and spawns one long-lived bridge
  task that only awaits the receiver - no polling, no timer. A click calls
  `refresh_now`: tray goes muted + `refreshing...`, a not-yet-loaded widget
  drops the old error so the screen says `FETCHING...`, and `spawn_real` runs
  again - which re-reads the keys, rebuilds every source, fetches immediately
  and then re-anchors to the next `:00`/`:30`. `spawn_real` grows a `bool`
  return (started or not) so a failed *build* does not leave the tray sitting on
  `refreshing...` forever.
- **`tray.rs`.** `MENU_REFRESH`, the item `Refresh` between `Show/Hide` and
  `Run at startup`, flags from the pure `refresh_menu_flags(available)`
  (`MF_GRAYED` when nothing listens), the command calls `refresh::request()`.
  Two new states, both dumpable: `set_refreshing(location)` (muted, keeps
  `last_verdict`, so a still-closed refresh never warns twice) and
  `set_no_data(location)` (muted, after a round that left nothing to show - it
  replaces the cold-start tip that used to be bare `AirQlality`).
  Seam `AIRQLALITY_TRAY_REFRESH_MS=<n>` fires the same `request()` the menu
  does, mirroring `AIRQLALITY_TRAY_QUIT_MS`.
- **`ui.rs`.** Seam `AIRQLALITY_FETCH_DUMP=<path>`: one JSON line per real fetch
  round (`{"round":1,"ok":0,"failed":["Open-Meteo"]}`), so the E2E can prove a
  refresh *fetched again* rather than re-rendered the old state.
- **Tests.** `refresh` units (5), `tray` units (`refresh_menu_flags`), and one
  new E2E: the test itself binds a `TcpListener` on `127.0.0.1:0` and answers
  request #1 with **503** and request #2 with the committed
  `core/tests/fixtures/openmeteo-cairo.json`. The app runs the real source
  against that port with `AIRQLALITY_TRAY_REFRESH_MS=3000`, and the test asserts
  **two** requests served, `round 1` failed / `round 2` ok in the dump, the
  curve painted in `theme::PAST` (absent on the error screen), and the tray icon
  no longer muted. That is the falsification clause measured, not described.
- **Evidence.** `EXP22-after-refreshed.png` and `EXP22-before-stuck.png` (the same
  loopback source, recovered vs. stuck), the real right-click menu shot
  (`EXP22-tray-menu-real-zoom.png`), plus the fetch/tray dumps and the E2E colour
  counts.
- **May touch:** `app/src/refresh.rs` (new), `app/src/{ui.rs,tray.rs}`,
  `app/Cargo.toml` + `Cargo.lock` (the app's own dependency entry),
  `app/tests/e2e_widget.rs`, `experiments/EXP22/**`, `experiments-overview.md`,
  branch `EXP22` (from `997c525`, the `EXP21` tip; `main` is stale at the initial
  commit + one AGENTS bump, the chain is the project's practice).
- **Off-limits:** verdict/threshold/`fuse`/`schedule` semantics, the census and
  `best_window` from EXP20, `experiments/EXP01..21/**`, declaring a final
  status.
- Machine-level blast radius: **no new download**, no new network call (the E2E
  talks only to its own loopback server).

## Open questions (known unknowns)
- [ ] **Label and position** - `Refresh`, second item, above `Run at startup`
      (recorded as `Refresh now`, shortened on the user's request).
      *(Assumption adopted, then renamed.)*
- [ ] **Does a manual refresh disturb the `:00`/`:30` schedule?** - **No**:
      `spawn_real` fetches now, then sleeps to the next boundary, so the
      schedule re-anchors exactly as on launch (EXP18). *(Assumption adopted.)*
- [ ] **Enabled under the mock seams?** - **No**: greyed whenever no listener is
      published. The mocks have no real fetch to re-run, and an enabled item
      that does nothing is a lie. *(Assumption adopted.)*
- [ ] **Feedback while the widget is hidden in the tray** (the actual use case:
      you notice the icon is stale) - muted square + `refreshing...`, and the
      last verdict is **kept** so a refresh that finds the same closed reading
      does not fire the EXP17 balloon twice. *(Assumption adopted.)*
- [ ] **A source that fails to *build*** (bad endpoint override, TLS init) -
      `spawn_real` keeps its early return, and **the refresh reports the build
      error exactly as today** instead of pretending to fetch. Retrying a
      build failure needs an app restart, not a button: rewritten from the plan,
      where the plan had folded it into the loop. Folding it in would have meant
      either `#[derive(Clone)]` on `core::SourceError` or a changed
      `apply_stack` contract for a path no user can reach. *(Assumption adopted,
      scope narrowed.)*
- [ ] **Also a Ctrl+R / on-screen Retry?** - **Not in this experiment.** The
      tray is the surface that is reachable while the widget is hidden. Noted as
      a follow-up. *(Assumption adopted.)*
- [ ] **Is the loopback-server E2E worth its harness code?** - **Yes**: it is the
      only proof that the button *recovers data*, not merely that it fires a
      callback. *(Assumption adopted.)*

## What the app shows

```text
tray right-click (real mode)
  Show / Hide
  Refresh            <- new
  Run at startup   [v]
  ---------------------
  Quit

tooltip while refreshing   AirQlality - Cairo, Egypt - refreshing...
widget after a failed round  (muted) COULDN'T FETCH ... Retrying automatically every 30 minutes.
widget after Refresh     the normal banner + timeline again
```

One menu state is photographed from a **real right-click** on the icon
(`capture-menu.ps1`, the EXP12 harness): `EXP22-tray-menu-real-zoom.png` (item
enabled). The greyed state is **not** photographed: the tray slot moves between
runs (a killed app's ghost icon, plus other apps' icons coming and going), so a
second capture kept landing on a neighbour's menu. That rule is pinned by the
unit test `tray::tests::refresh_item_is_greyed_only_without_a_listener` instead -
cheaper and not coordinates-dependent.

## Evidence (pixels + dumps)

The recovery run is the E2E `widget_e2e_tray_refresh_recovers`: the app's real
source is pointed at a loopback server that answers request #1 with **503** and
request #2 with the committed Open-Meteo fixture, and the tray seam fires the
menu's `refresh::request()` 3 s in.

```text
requests=2
--- fetch rounds (AIRQLALITY_FETCH_DUMP) ---
{"round":1,"ok":0,"failed":["Open-Meteo"]}
{"round":2,"ok":1,"failed":[]}
--- tray (AIRQLALITY_TRAY_DUMP) ---
{"color":"#d29922","tip":"AirQlality - Cairo, Egypt - VENT","ok":true}
curve pixels (theme::PAST) = 726..1777 across runs, assert > 200
```

- **Two requests, in that order.** The second one is the tray item, not the
  boundary timer (`AIRQLALITY_REFRESH_PERIOD_SECS=7200` in the run).
- **Round 1 failed, round 2 brought data.** The dump is written by the fetch loop
  itself, so a repaint of the old state could not fake it.
- **The tray left the muted state for a verdict** (`#d29922` = VENT) - i.e. it is
  not stuck on `refreshing...`/`no data`.
- **The frame is the data view, not the error screen:** `theme::PAST` (the
  observed part of the PM2.5 curve) exists only in the timeline.

`EXP22-before-stuck.png` and `EXP22-after-refreshed.png` are the same setup
captured for the eye: the red `COULDN'T FETCH` screen with the 503 in its detail
line, then the normal banner + timeline after the refresh. Their fixture
timestamps are re-anchored around "now" so the banner shows a real 24 h horizon
(the committed fixture is dated 2026-10-04, and a stale series legitimately
collapses the horizon to its last hour - true, but it reads like a bug in a
screenshot).

Suite, whole workspace: **core 64/64** (unchanged), **app 44/44** (+7: 5
`refresh`, 2 `tray`), **e2e 25/25** (+1), 5 live probes ignored.

## Conclusion (findings)

**Concluded `success` by the user on 2026-10-21.**

**Hypothesis upheld.** A tray item that wakes the fetch loop makes a broken
fetch recoverable in place, and every clause of the falsification test was
checked in dumps and pixels rather than prose: two requests served in order
(503, then the fixture), the dump showing `round 1` failed and `round 2` ok, the
recovered frame carrying the PM2.5 curve, and the tray icon leaving the muted
state for the recovered verdict - with no restart, and with the `:00`/`:30`
boundary timer explicitly out of the picture.

- **Plumbing.** `refresh.rs`: a `bounded(1)` `async-channel` + one static
  `Mutex<Option<Sender<()>>>`; `deliver` is pure and unit-tested (delivers once,
  coalesces, no listener / dead listener are no-ops). The widget publishes it in
  real mode only and awaits it in one idle bridge task; the tray thread calls
  `request()` from the menu. No new download (`async-channel` is in the graph via
  `gpui-ce`), no polling, no timer.
- **Behaviour.** `refresh_now` reuses `spawn_real`, so a manual refresh re-reads
  the keys (a key added to `config.json` is picked up), rebuilds every source,
  fetches now and re-anchors to the next boundary. A not-yet-loaded widget drops
  the old error first, so the screen says `FETCHING...` instead of repeating the
  failure.
- **Tray.** `Refresh` (renamed from `Refresh now` on the user's request) sits
  between `Show/Hide` and `Run at startup` and is `MF_GRAYED` when nothing is
  listening. Two new states - `refreshing...` in flight (keeps `last_verdict`, so
  a still-closed reading never fires the EXP17 balloon twice) and `no data` after
  a round that brought nothing back (a failed cold start used to leave a bare
  `AirQlality` tip). The icon ends on the verdict the graph shows, because the
  refresh lands in the same `apply_stack` -> `tray_set_verdict` path.
- **Numbers.** core 64/64 (untouched), app 37 -> 44 (+5 `refresh`, +2 `tray`),
  e2e 24 -> 25 (+1), 5 live probes ignored; the full windowed suite 25/25 in
  210 s. Release exe rebuilt (32.5 MB) and its strings carry the new labels.
- **Corrected on the way.** The plan proposed folding a *build* failure into the
  fetch loop; that would have needed `#[derive(Clone)]` on `core::SourceError` or
  a changed `apply_stack` contract for a path no user can reach. `spawn_real`
  keeps its early return and returns a `bool` instead, so a refresh reports the
  build error honestly rather than sitting on `refreshing...`.
- **Not proven, and out of scope.** Nobody clicked the real menu by hand: the E2E
  fires the same `refresh::request()` the command calls, and the menu is proven
  present/enabled by a real right-click screenshot. A *build* failure still needs
  an app restart. A refresh during an in-flight fetch cancels the old loop, but a
  blocking fetch already running can still deliver its result first - the same
  pre-existing behaviour as switching location. There is no `Ctrl+R` or on-screen
  retry yet.

**Recommended next:** a retry affordance on the error screen (`Ctrl+R` / button)
so a *visible* widget can retry without the tray; the EXP21 follow-up (real
sunrise/sunset, a tray toggle for the night layer); and the honest history layer
the aqi.in study argued for.

## Learnings
- `cargo test -p airqlality-core` / `-p airqlality --bin airqlality` while
  iterating (no window); the windowed E2E suite **once, at the end** - each E2E
  test spawns the real widget, so a full run flashes windows for minutes
  (AGENTS.md). The full suite here: 25/25 in 210 s.
- **Tray -> widget is a channel, not a window message.** The tray thread owns its
  own HWND and gpui's window proc drops unknown `WM_APP` messages, so anything the
  menu must *do to the UI* has to wake a gpui task. `async-channel` is already in
  the graph (gpui-ce depends on it): `bounded(1)` + `try_send` costs no download
  and coalesces a burst of clicks for free. The task must **only await** the
  receiver (idle = free), and the action should call the existing launch path
  (`spawn_real`) so refresh and launch cannot drift.
- **A test-seam counter must be process-wide, not per loop.** A refresh replaces
  the fetch loop, so a `round` counter inside it numbered both dump lines `1`; use
  a `static AtomicU32`.
- **Locating the tray icon for a screenshot:** flip the verdict colour (mock-OPEN
  green vs mock-CLOSED red) to identify our square, then crop from the
  bottom-right corner. The slot is **not stable across runs** (a killed app's
  ghost icon, other apps' icons), and a scan for our exact fill colour is not a
  shortcut - the first `#f85149` match was another app's red badge.
- **Offline failure-then-recovery tests:** a loopback `TcpListener` answering
  `HTTP/1.1 503` then the committed fixture is enough to make a real fetch fail
  and then succeed. Read the request head, always send `Content-Length` and
  `Connection: close`, take the port from `local_addr()`, and re-anchor fixture
  timestamps if the assertions or the screenshot care about the horizon.
- `rustfmt --check` reports pre-existing diffs in `tray.rs`/`ui.rs`, so do **not**
  run `cargo fmt` wholesale (it would bury the experiment in reformatting); match
  the surrounding style by hand.


