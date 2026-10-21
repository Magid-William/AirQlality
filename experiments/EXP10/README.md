# EXP10 - Keyboard & assistive-tech access for the location control

**Status:** `success` (user-declared, 2026-10-04).

## Summary
Make the widget usable without a mouse: Tab/Shift+Tab cycle focus, a shortcut opens
and focuses the search field, the caret blinks, the field looks typeable on hover,
results navigate by arrow keys, and every component gets a name/role for screen
readers - guided by WCAG 2.2 AA/AAA where it applies.

## Hypothesis
gpui-ce 0.2.2's focus/tab-stop system (`window.focus_next/focus_prev` +
`tab_stop`/`tab_index`) plus its AccessKit surface (`role`, `aria_label`,
`aria_value`, `aria_expanded`, `aria_selected`, `aria_active_descendant`,
`aria_keyshortcuts`, `focus_visible`, `hover`, `cursor_text`) is enough to make the
location control fully keyboard-operable and screen-reader-announced, with a visible
focus ring and a blinking caret, **without regressing the pixel E2E.**

**Falsified if:** Tab focus can't be driven from the app (gpui does *not* auto-bind
Tab), the a11y tree can't be observed in the test build, or the closed-state pixel
asserts change.

## Methodology & blast radius
- Method (app-only):
  1. `trigger_focus` + existing `search_focus` handles; trigger and field are tab
     stops with an explicit order.
  2. Root `on_key_down`: `tab` -> `focus_next`, `shift-tab` -> `focus_prev`,
     `escape` -> close, `ctrl-k` opens/focuses search, `ctrl-q` quits.
  3. Combobox key map in the field: Up/Down/Home/End move the active row, Enter
     selects it, Esc closes, Tab moves focus (never types a tab). New `active` index.
  4. Blinking caret (~530 ms) driven by a `cx.spawn` timer while the panel is
     open+focused; `AIRQLALITY_NO_BLINK` seam keeps the E2E deterministic.
  5. Styling: `focus_visible` ring, `hover`/`active` states, `cursor_text()` on the
     field, `cursor_pointer()` on trigger/rows.
  6. AccessKit: names/roles/states (trigger ComboBox + expanded + keyshortcuts;
     field TextInput + value/placeholder; list ListBox; rows Option + selected;
     verdict Label). `aria_label` text alternative on the canvas.
  7. New `CHART_AXIS` (`0x6e7681`, 4.12:1) replaces `AXIS` (`0x30363d`, 1.55:1)
     for the chart axis and threshold lines (meaningful graphics, WCAG 1.4.11);
     `AXIS` stays for panel/field borders and `GRID` stays decorative. (Text
     contrast already passes AA; audited in the plan.)
  8. `AIRQLALITY_A11Y` seam dumps `window.debug_a11y_tree_json()` so the E2E can
     assert names/roles.
- Verify: `cargo test -p airqlality-core` (21/21 unchanged); `cargo test -p
  airqlality` (persist 2/2 + e2e). Extend `capture.ps1` to post Tab / arrows /
  Ctrl+K; add E2E cases for the focus ring and the a11y tree. Closed-state counts
  (`closed=2043 open=790`, `panel=0`) must stay green. Evidence into
  `experiments/EXP10/`.
- May touch: `app/src/ui.rs`, `app/src/theme.rs`, `app/tests/e2e_widget.rs`,
  `app/tests/capture.ps1`, `experiments/EXP10/**`, `experiments-overview.md`,
  branch `EXP10` (from `dae50c7`).
- Off-limits: `core/**`, `decision.rs` rule/thresholds, `UI.md`, `AGENTS.md`,
  `experiments/EXP01..09/**` (frozen evidence), declaring a final status.
- Machine-level blast radius: no new crate dependencies; adds a repaint timer only
  while the panel is open.

## Open questions (known unknowns)
- [x] Branch base - `EXP10` from `EXP09` HEAD (`dae50c7`).
- [x] Tab scope - trigger <-> search field only; rows use arrow keys (APG
      combobox), not Tab.
- [x] Search shortcut - `Ctrl+K` open/focus, Esc closes; `aria_keyshortcuts`.
- [x] Blink + reduced motion - 530 ms blink, focused-only; gpui-ce 0.2.2 exposes no
      reduced-motion flag (documented gap).
- [x] Focus ring - 2px `FOCUS`/`FUTURE` blue (7.5:1 on BG), keyboard-only via
      `focus_visible`.
- [x] Startup focus - **assumption changed**: the trigger *is* focused on the first
      render. Without focus the dispatch path is only the window root, so a root
      key listener never fires (see Learnings). The ring stays hidden until the
      first key press, so the closed E2E is unaffected.
- [x] Chart contrast - new `CHART_AXIS` `0x6e7681` (4.12:1) for the meaningful
      chart lines; `AXIS`/`GRID` left as-is.
- [x] SR verification - `debug_a11y_tree_json()` asserted in the E2E (activated by
      posting `WM_GETOBJECT`), no manual Narrator check needed.

## Conclusion (findings)
**Concluded `success` by the user on 2026-10-04.**

**Hypothesis upheld.** gpui-ce 0.2.2's focus/tab-stop system plus its AccessKit
surface were enough to make the location control fully keyboard-operable and
screen-reader-announced, with a visible focus ring and a blinking caret, with no
new crate dependencies and no change to the closed-state E2E.

- **Keyboard.** Root `on_key_down` drives Tab/Shift+Tab (`window.focus_next` /
  `focus_prev`; gpui binds no Tab itself), Esc, `Ctrl+K` (open+focus search) and
  `Ctrl+Q` (quit). The trigger activates on Enter/Space; the search field is a
  proper combobox: Up/Down/Home/End move the active row, Enter selects it, Esc
  closes. Focus returns to the trigger on close/select. `focus_visible` styles key
  focus only, so the idle window is visually unchanged.
- **Affordance.** I-beam cursor + hover tint on the field; pointer + hover on
  trigger/rows; a ~530 ms caret while focused (`AIRQLALITY_NO_BLINK` seam);
  result rows are `.w_full()` so their hitbox fills the panel (137 px -> 323 px).
- **Screen reader.** AccessKit roles/names/states are exposed: verdict `Label`,
  trigger `Button`+`expanded`+`keyboard_shortcut`, field `TextInput`+value/
  placeholder, results `ListBox`/`ListItem`+`selected`, chart `Image` with a text
  alternative. `aria_active_descendant` was **not** used: the list is a sibling of
  the focused field, so the relationship would not be honored; `aria_selected` on
  the active row carries the state instead.
- **Contrast.** New `CHART_AXIS` (`0x6e7681`, 4.12:1) fixes the meaningful chart
  lines (WCAG 1.4.11); text contrast already passed AA.
- **Tests.** `core` 21/21; app persist 2/2 + e2e **8/8**, the 3 new ones being:
  Tab paints the focus ring (`FOCUS` pixels in the header band), arrow+Enter
  selects the 3rd `"san"` hit and persists Sanaa (`71137`), and the AccessKit tree
  (activated by posting `WM_GETOBJECT`) names everything with
  `tab_stop_count = 2`.
- **Evidence:** `EXP10-{closed,focus-tab,open-caret,open-arrow,select-enter,trigger-enter}.png`
  + `EXP10-a11y.{png,json}`. Closed verdict counts unchanged (`closed=2043
  open=790 top_closed=1131 panel=0`).
- **Not proven (out of scope):** no reduced-motion or system text-scale support
  (gpui-ce 0.2.2 exposes neither); `Ctrl+K`/`Ctrl+Q`/`Shift+Tab` are implemented
  but not black-box E2E'd (Windows cannot post modifiers; `Ctrl+Q` was verified
  manually with `keybd_event`, exit code 0); no IME/non-Latin input; no UIA
  live-region announcement of fresh data.

**Recommended next:** disk cache so a restart shows the last good series offline,
then always-on-top + tray (the frontier EXP01 flagged); a follow-up could add
reduced-motion and system text scaling if gpui gains the APIs.

## Learnings
- **Tab focus is app-driven.** gpui-ce has `Window::focus_next/focus_prev` but
  binds no Tab; a `FocusHandle` is only a tab stop if built with
  `.tab_stop(true)` — a div's `.tab_stop`/`.tab_index` is ignored when an
  explicit handle is passed via `track_focus`.
- **A root key listener needs the widget to be focused.** With nothing focused
  gpui's dispatch path is just the window root, so the view's root `on_key_down`
  never fires; focus the trigger on the first render (ring stays hidden until the
  first key press).
- **`focus_visible` == CSS `:focus-visible`** (`window.last_input_was_keyboard`),
  which is why ordinary/mouse interaction leaves the idle window unchanged.
- **In tests, press keys AFTER the last window raise** (`capture.ps1`): a later
  raise's mouse event flips input modality back to Mouse and hides the ring.
- **Full-width list rows need `.w_full()`** or the hitbox is text-width only.
- **Modifiers can't be posted** (Windows reads real key state); post unmodified
  keys with `capture.ps1 -Press "down down enter"`. Real-state chords need
  `keybd_event` (foreground-dependent, so not a committed test).
- **AccessKit dumps for tests:** `debug_a11y_tree_json()` becomes `Some` only
  after activation on Windows, i.e. a posted `WM_GETOBJECT` with
  `lParam = -25` (`capture.ps1 -A11y`).
- **Commands / numbers:** PATH += MinGW, `cargo test -p airqlality-core` (21/21),
  `cargo test -p airqlality` (persist 2/2 + e2e 8/8); closed counts unchanged
  (`closed=2043 open=790 panel=0`); focus band `1280` px vs `0`; `Ctrl+Q` exits 0.
