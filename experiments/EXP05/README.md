# EXP05 - Live ticker: advance the "now" post every 3 s, wrap at the end

**Status:** `success` (user-declared, 2026-10-04).

## Summary
Make the widget self-updating: every 3 s the "now" post advances one hour along
the timeline, the verdict/best-window re-evaluate, and the view repaints. At the
end of the 48 h series (the 11:59:59 PM case) the post wraps back to the start.

## Hypothesis
A `Context::spawn` loop that awaits `background_executor().timer(3 s)`, advances
`Series::now_index` by one (wrapping `last -> 0`), recomputes `Decision`, and
calls `cx.notify()` moves the post and updates the banner/timeline/ribbon live on
Windows, without blocking the UI thread and without changing `draw_timeline`.

**Falsified if:** the timer loop does not repaint, the post/wrap cannot be
unit-tested deterministically in `core`, or the EXP04 E2E can no longer produce a
stable frame.

## Methodology & blast radius
- Method:
  1. `core`: pure `Series::advance_now()` — advance one hour, wrap `last -> 0`;
     unit tests for both (deterministic pin of the midnight/end edge case).
  2. `app`: `Widget::new(cx)` spawns a 3 s loop (`cx.background_executor().timer`)
     that advances, re-evaluates, and `cx.notify()`s; skipped when
     `AIRQLALITY_FREEZE_NOW` is set (test seam).
  3. `app/tests/e2e_widget.rs`: set `AIRQLALITY_FREEZE_NOW=1` on the capture
     child so the pinned KEEP CLOSED frame stays byte-identical.
  4. Verify: `cargo test -p airqlality-core`; run the widget and capture two
     frames a few ticks apart (post moved + verdict/ribbon changed); re-run the
     EXP04 E2E.
- May touch: `core/src/model.rs`, `app/src/ui.rs`, `app/src/main.rs`,
  `app/tests/e2e_widget.rs`, `experiments/EXP05/**`, `experiments-overview.md`,
  branch `EXP05` from `e8d6c58`.
- Off-limits: `UI.md`, `AGENTS.md`, `experiments/EXP01..04/**` (frozen
  evidence), rule thresholds/logic in `decision.rs`, any network call,
  declaring a final status.
- Machine-level blast radius: none new — reuses the cached GNU-toolchain build;
  the window repaints periodically.

## Open questions (known unknowns)
- [x] Tick step — **1 hour per 3 s tick** (user chose accelerated clock).
- [x] Wrap target — **index 0** (agent decision, per user "you decide").
- [x] Clock source — **virtual accelerated**; becomes real with `OpenMeteoSource`.
- [x] E2E determinism — **`AIRQLALITY_FREEZE_NOW` freeze switch** (user chose).
- [x] Wrap assertion — **`core` unit test**, not pixels (a 48-step pixel cycle
      is flaky) (plan default).

## Conclusion (findings)
**Concluded `success` by the user on 2026-10-04.**

**Hypothesis upheld.** A gpui `cx.spawn` loop advances the "now" post on a 3 s
timer and the widget repaints live on Windows, with the end/restart edge case
pinned deterministically in `core` and the EXP04 E2E left byte-identical.

- **Ticker (app):** `Widget::new(cx)` spawns `cx.spawn(async { loop {
  background_executor().timer(3 s).await; advance; re-evaluate; cx.notify() } })`
  and `.detach()`s it; the loop breaks when the window closes. One tick = one
  hour, so the 48 h window plays out in ~2.4 min. `draw_timeline` was untouched;
  only `now_index` + `decision` change.
- **Post/wrap (core):** `Series::advance_now()` advances one hour and wraps
  `last -> 0` — the 11:59:59 PM case = the hour after the last is the first.
  `core` is **7/7**; `advance_now_walks_every_hour_then_restarts` walks a full
  cycle from hour 23 and asserts every hour is visited once, the end is reached,
  the next hour is `0`, and the cycle returns to the start.
- **Verdict updates live:** the banner, best-window line, ribbon and "now"
  divider all track the post (verified frames below).
- **E2E stays deterministic:** `AIRQLALITY_FREEZE_NOW` gates the spawn; the E2E
  sets it on the capture child and the screenshot is unchanged from EXP03/04
  (`closed=2043 vent=6293 open=790`, `top_closed=1131 top_open=0`), 1 passed.
- **Evidence:** `EXP05-tick-early.png` (VENT, hour 25) -> `EXP05-tick-late.png`
  (OPEN, hour ~35); end -> restart `EXP05-wrap-before.png` (hour 47: post far
  right, KEEP CLOSED, "no clean window") -> `EXP05-wrap-after.png` (hour 6: post
  back at the left, VENT, a clean window).
- **All choices landed:** user chose 1 h/tick and the freeze switch; the agent
  chose wrap-to-index-0. No question needed a reversal.
- **Not proven (out of scope):** the clock is a **virtual accelerated** one, not
  the real wall clock — using real time is a one-shot change once
  `OpenMeteoSource` lands; the interval is a compile-time const (no settings);
  the post jumps a whole hour per tick (no smooth tween).

**Recommended next:** implement `OpenMeteoSource` behind `AirQualitySource`, then
drive the post from the real current hour instead of the accelerated step.

## Learnings
- **Repeating gpui-ce ticker:** `cx.spawn(async move |this, cx| { loop {
  cx.background_executor().timer(Duration::from_secs(3)).await;
  if this.update(cx, |w, cx| { w.tick(); cx.notify(); }).is_err() { break; }
  } }).detach();` — `WeakEntity::update` returns `gpui::Result` (break on `Err`
  when the window closes); `timer(..)` is a one-shot `Task<()>` awaited in a loop.
  Spawn it from a constructor that takes `&mut Context<Self>`
  (`cx.new(|cx| Widget::new(cx))`).
- **Freeze the clock for deterministic screenshot tests:** gate the spawn on
  `std::env::var_os(..).is_none()`; set the var on the capture child
  (`.env(..)`), inherited through PowerShell to the exe.
- **Prove a wrap with a full cycle, not one step:** assert every index is
  visited once and the cycle returns to start; capture the exe before/after the
  end (the tick clock starts ~2 s after launch, so allow for that offset).
