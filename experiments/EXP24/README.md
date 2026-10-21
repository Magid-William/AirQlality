# EXP24 - Two prompts that record the source API keys

## Summary
Replace hand-editing `%APPDATA%\AirQlality\config.json` with a two-step prompt in the widget: OpenAQ, then OpenWeatherMap. Free-text keys are pasted or typed, masked, saved one at a time, and a single refetch applies them.

## Hypothesis
A user who has never opened `config.json` can add both keyed sources from inside the widget in under a minute: the keys land in `config.json`, the header names 3 sources, and no source without a key changes — every pre-EXP24 frame and test count stays identical.

## Methodology & blast radius
- Method: a pure `app/src/keys.rs` owns the two-step model (step order, per-step fields and notes, navigation, sanitising, masking, duplicate detection, per-step copy); `ui.rs` renders it as a modal over the widget in the same deferred/scrim style as the EXP07 dropdown; `persist.rs` gains a merge-and-rename write; the tray->widget channel (EXP22) is generalised from "refresh" to `Command::{Refresh, ApiKeys}` so the tray can open the flow. `core` gains `SourceError::Rejected` (HTTP 401/403) and a one-request `check_key()` per keyed source, so the dialog can answer "key accepted" / "rejected: HTTP 401" at save time.
- Triggers: tray menu `API keys...` (shows the widget first) and `Ctrl+Shift+K`; test seams `AIRQLALITY_KEYS_PROMPT=1|2` (open a step at launch) and `AIRQLALITY_KEYS_DUMP=<path>` (step + stored slots after every change) for the windowed E2E.
- Keys stay outside the repo, in the same plaintext `config.json`; env vars still win, and the prompt says so. No new crate: clipboard and `async-channel` are already in the dependency graph.
- May touch: `app/src/{keys,persist,commands,ui,tray,main}.rs`, `core/src/{source,http,openaq,openweathermap}.rs`, `app/tests/e2e_widget.rs`, `README.md`, `assets/tray.png`, `experiments/EXP24/`, `experiments-overview.md`.
- Off-limits: the tray icon and notification behaviour, `Cargo.toml`, existing screenshots and baselines.

## Open questions (known unknowns)
- [x] Auto-prompt for keys on first run? - proposed default: no. The keyless app is complete; a startup nag would also change every first-run frame. On demand only.
- [x] One modal with two steps, or two windows? - proposed default: one modal, two sequential prompts (OpenAQ then OpenWeatherMap), one refetch at the end.
- [x] Reveal toggle? - proposed default: no. Always masked in the UI and in the accessibility value; the key is readable in `config.json` for anyone who needs it.
- [x] Storage: keep plaintext `config.json` or use the Windows Credential Manager? - proposed default: keep the file (atomic write) in this experiment; a credential store is its own experiment.
- [x] Empty field semantics? - proposed default: empty + Save keeps the stored key; `Remove` (shown only when a stored key exists) deletes it. Esc cancels the current step without writing it; earlier saved steps stay.
- [x] Navigation? - **revised after the first real report**: Prev/Next are their own row and never save; each step keeps its own field; Save stays on its step (no forced OpenAQ -> OWM walk). This replaced the original "Save advances" flow.
- [x] Offline validation? - proposed default: trim/normalise only, plus a "too short" guard; a wrong key shows up through the existing stale-note error, with the key scrubbed.
- [x] Env var set for a source? - proposed default: still allow saving, with a muted note that the environment wins until the variable is unset.
- [x] Paste? - proposed default: `Ctrl+V`; `capture.ps1` cannot post modifier chords, so the chord itself is a manual check while the sanitiser is unit-tested.
- [x] Remove button placement/scope? - proposed default: a small text button in the dialog footer, only for a step whose key is currently stored.

## Conclusion (findings)
**Success** (user, 2026-10-21). Keys can be entered, verified and removed entirely
from the widget: two prompts (OpenAQ then OpenWeatherMap), each recording exactly
one key, reachable from the tray's `API keys...`, the header's `API keys` button,
or `Ctrl+Shift+K`. A keyless install is unchanged.

- **Flow**: one modal, one source per step. Each step keeps its own field;
  `Prev / Next` are their own row and never save; `Save` stores only the current
  key and stays; `Esc` / `Cancel` / the backdrop close with one refetch;
  `Remove` turns a source off. Empty + Save changes nothing.
- **Safety**: masked field (also the a11y value); normalised on save; a partial
  paste is refused; a key identical to the other source's stored key is refused;
  `config.json` is merged and written via temp+rename (unknown fields survive);
  an env override is stated on the step and still wins.
- **Feedback**: one cheap request per saved key, in the background -
  `checking the key…` -> `key accepted` / `rejected: HTTP 401`.
  `SourceError::Rejected` (401/403) makes the stale note say
  `key rejected (HTTP 401) — …` instead of "network error"/"offline".
- **Links**: the free-key hint is a real link (`Role::Link`, Tab reachable) that
  opens the provider page in the default browser (off the UI thread - see
  Learnings).
- **First real attempt** (user report): the OpenAQ key was accepted (`200`) while
  OpenWeatherMap answered `401`; `config.json` held the **OpenAQ key in both
  slots**. Cause not proven (same clipboard entry pasted at step 2, or the old
  save-and-advance flow), but the four key E2E tests now pin each slot. The OWM
  key supplied with the report is refused by OpenWeatherMap itself
  (`401 Invalid API key`), so a live three-source run with a valid OWM key is
  still pending.
- **Suite**: core 64/64, app 65/65 (44 -> 65: keys, persist merge, commands,
  tray), windowed E2E 29/29 in 254 s (24 -> 29: four key-prompt tests + the
  refresh test now isolates `%APPDATA%`; the a11y tab-stop count 2 -> 3 for the
  new header button).
- **Evidence**: `EXP24-prompt1.png`, `EXP24-prompt2.png`
  (step 1 with Prev greyed, step 2 with Next greyed), `EXP24-header-keys.png`
  (the header entry point), `assets/tray.png` (README, under the hero shot).
- **Not proven / manual**: the `Ctrl+V` chord (capture.ps1 cannot post
  modifiers; the sanitiser is unit-tested, paste was used by hand); the link
  click (verified by posted mouse messages: app alive + Chrome started, but it
  stays manual so tests never launch a browser); a live three-source frame.

## Learnings
- `$env:PATH = "C:\Users\MW\AppData\Local\mingw64\bin;" + $env:PATH; cargo build -p airqlality` - the MinGW-W64 toolchain is under `%LOCALAPPDATA%\mingw64` (not `C:\mingw64`); without it the link fails on `-lktmw32`. `cargo check --all-targets` needs no linker.
- Never call `ShellExecuteW` from a gpui handler: the shell pumps messages, gpui re-enters with its context borrowed, and the app panics (`RefCell already borrowed`). Use a detached `std::thread`.
- Any windowed E2E that runs in real mode must set `APPDATA` to an empty temp dir (and remove the key env vars): a developer's real `config.json` now adds sources that the test does not expect.
- `capture.ps1` posts keys without modifiers, so `Ctrl+V` cannot be driven by the E2E - test the sanitiser instead and check paste by hand.
- Chat attachments live in `~/.local/share/opencode/opencode.db`, `session_message.data.files[].data` (base64), not on disk.
