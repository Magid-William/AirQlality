//! The gpui view: header, verdict banner, timeline chart, legend.
//!
//! All numbers come from `airqlality-core`; this module owns only presentation.
//! The chart is hand-drawn with the paint API (EXP02 finding) so the widget
//! stays dependency-free.

use airqlality_core::{
    AirQualityPoint, AirQualitySource, City, DAY_END_HOUR, DAY_START_HOUR, Decision, Kind, Location,
    MAX_DB, MIN_DB, MockSource, OpenAqSource, OpenMeteoSource, OpenWeatherMapSource, SEARCH_LIMIT,
    Series, SourceError, SourceSeries, Spread, Stack, Thresholds, Verdict, align_to_series,
    city_by_id, country_name, estimate_for_city, evaluate, mock_stack, night_curve, search,
    stack as fuse_stack,
};
use gpui::{
    AnyElement, Bounds, Context, FocusHandle, FontWeight, InteractiveElement, KeyDownEvent,
    MouseButton, MouseMoveEvent, PathBuilder, Pixels, Point, Render, Role, ScrollStrategy, Task,
    UniformListScrollHandle, Window, canvas, deferred, div, fill, point, prelude::*, px, rgb,
    rgba, size, uniform_list,
};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use crate::browser;
use crate::commands::{self, Command};
use crate::keys::{self, Step};
use crate::persist;
use crate::schedule::until_next_boundary;
use crate::theme::*;

/// How often the post moves in mock/demo mode. One tick = one hour of the
/// timeline, so the whole window plays out in ~2.4 min.
const TICK: Duration = Duration::from_secs(3);

/// How often the real source is re-fetched: the refresh loop sleeps to the next
/// wall-clock multiple of this period, so it ticks at `:00` and `:30` rather than
/// 30 minutes after whenever the app launched (EXP18).
const REFRESH: Duration = Duration::from_secs(30 * 60);

/// One dropdown row's height, and the results viewport (7 rows visible, the rest
/// scroll). A uniform row height is exactly what `uniform_list` measures.
const ROW_H: f32 = 26.0;
const RESULTS_H: f32 = 7.0 * ROW_H;
/// Header height, so the panel can be positioned just below it.
const HEADER_H: f32 = 36.0;

/// Timeline chart insets/ribbon, shared by the drawing and the hover hit-test so
/// the tooltip anchors to the exact hour the pointer is over.
const CHART_INSET_X: f32 = 6.0;
const CHART_INSET_TOP: f32 = 8.0;
const CHART_INSET_BOTTOM: f32 = 8.0;
const RIBBON_H: f32 = 8.0;
/// How far the day/night wave rises at local midnight, as a fraction of the plot
/// height. High enough to read as a wave, low enough never to reach the PM2.5
/// curve's usual band (EXP21).
const WAVE_AMPLITUDE: f32 = 0.45;
/// Tooltip panel width, used to clamp/flip it at the chart edges.
const TOOLTIP_W: f32 = 150.0;

/// Caret blink half-period. The browser/editor convention is ~1 s on/off total;
/// a caret is not "blinking content" under WCAG 2.2.2, but a steady caret is the
/// reduced-motion fallback.
const BLINK: Duration = Duration::from_millis(530);

/// The text-insertion caret glyph (a thin vertical bar).
const CARET: &str = "\u{258f}";

/// One time label in the format the user chose (EXP25).
///
/// The widget default is 12-hour - "6:00 PM", no leading zero - because that is
/// how the app's own advice reads. Clicking the best-window time switches the
/// whole frame to 24-hour for the session; the preference is deliberately not
/// persisted, so a relaunch is 12-hour again.
fn clock(time: &chrono::DateTime<chrono::FixedOffset>, use24: bool) -> String {
    if use24 {
        time.format("%H:%M").to_string()
    } else {
        // `%-I` is the 12-hour number without the leading zero chrono would add.
        time.format("%-I:%M %p").to_string()
    }
}

/// "1 source issue" / "3 source issues": the header chip's label (EXP25).
fn issue_label(count: usize) -> String {
    format!("{count} source issue{}", if count == 1 { "" } else { "s" })
}

/// When set, the app writes its accessibility tree (AccessKit JSON) to this path
/// each frame, so the E2E can assert names/roles. Requires the platform to have
/// activated accessibility (the E2E posts `WM_GETOBJECT`).
fn a11y_dump_path() -> Option<String> {
    std::env::var("AIRQLALITY_A11Y").ok()
}

/// Source selection seam: `AIRQLALITY_SOURCE=mock` uses the deterministic mock
/// (tests/demos); anything else — the default — uses the real Open-Meteo source.
fn use_mock() -> bool {
    matches!(
        std::env::var("AIRQLALITY_SOURCE").ok().as_deref(),
        Some("mock")
    )
}

/// Whether the inferred noise overlay is shown. `off`/`on` force it; the default
/// is on for the real app and off under the deterministic mock seam, so the
/// pre-EXP16 screenshots stay byte-identical unless a test opts in with
/// `AIRQLALITY_NOISE=on`.
fn noise_enabled() -> bool {
    match std::env::var("AIRQLALITY_NOISE").ok().as_deref() {
        Some("off") => false,
        Some("on") => true,
        _ => !use_mock(),
    }
}

/// Whether the day/night wave is painted on the timeline (EXP21). Same seam
/// discipline as the noise overlay: `off`/`on` force it, the default is on for
/// the real app and off under the deterministic mock, so every pre-EXP21
/// screenshot stays byte-identical unless a test opts in with
/// `AIRQLALITY_NIGHT=on`.
fn night_enabled() -> bool {
    match std::env::var("AIRQLALITY_NIGHT").ok().as_deref() {
        Some("off") => false,
        Some("on") => true,
        _ => !use_mock(),
    }
}

/// Stacking seam (EXP19): `AIRQLALITY_SOURCES=mock3` runs the three-source
/// deterministic mock (Open-Meteo + a ground station + a second model) instead
/// of the single mock, so the fused view can be screenshotted with no network
/// and no API keys. Absent/anything else leaves the single-source mock alone.
fn use_mock_stack() -> bool {
    matches!(
        std::env::var("AIRQLALITY_SOURCES").ok().as_deref(),
        Some("mock3")
    )
}

/// How often the demo ticker advances one hour (EXP27). Three seconds by
/// default; a test can speed it up so a crafted sequence plays in about a
/// second instead of minutes.
fn tick_period() -> Duration {
    std::env::var("AIRQLALITY_TICK_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|ms| *ms > 0)
        .map(Duration::from_millis)
        .unwrap_or(TICK)
}

/// Test seam (EXP27): `AIRQLALITY_MOCK_PM25="12,20,30,30,20,30,20"` builds a
/// synthetic hourly series with "now" on the first value, so the windowed E2E
/// can walk an exact verdict pattern through the real app and count balloons.
/// The other air-quality fields are filled from PM2.5; nothing in that test
/// reads them.
fn mock_pm2_5_series(location: &str) -> Option<Series> {
    let raw = std::env::var("AIRQLALITY_MOCK_PM25").ok()?;
    let values: Vec<f32> = raw
        .split(',')
        .filter_map(|v| v.trim().parse::<f32>().ok())
        .filter(|v| v.is_finite() && *v >= 0.0)
        .collect();
    if values.is_empty() {
        return None;
    }
    let start = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00+02:00").ok()?;
    let points = values
        .into_iter()
        .enumerate()
        .map(|(hour, pm2_5)| AirQualityPoint {
            time: start + chrono::Duration::hours(hour as i64),
            pm2_5,
            pm10: pm2_5 * 1.6,
            us_aqi: pm2_5 * 4.0,
            wind_speed: 10.0,
        })
        .collect();
    Some(Series::new(location, points, 0))
}

/// The inferred hourly noise estimate (dB(A)) for a location, or `None` when the
/// layer is disabled or the city has no baked road density.
fn noise_for(location: &Location) -> Option<[f32; 24]> {
    if !noise_enabled() {
        return None;
    }
    let id = location.id.parse::<u32>().ok()?;
    estimate_for_city(city_by_id(id)?)
}

/// The real refresh period, overridable so the wall-clock alignment can be
/// watched without waiting 30 minutes (`AIRQLALITY_REFRESH_PERIOD_SECS`). Zero
/// or absent falls back to [`REFRESH`].
fn refresh_period() -> Duration {
    std::env::var("AIRQLALITY_REFRESH_PERIOD_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .map(Duration::from_secs)
        .unwrap_or(REFRESH)
}

/// Seconds since the Unix epoch, the reference for the next wall-clock boundary.
fn now_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Test seam (EXP22): with `AIRQLALITY_FETCH_DUMP=<path>`, append one JSON line
/// per real fetch round, so the E2E can prove that a tray refresh caused a
/// *second* fetch rather than a repaint of the old state.
///
/// The counter is process-wide, not per loop: a refresh replaces the loop, so a
/// per-loop counter would number the first round of every restart `1`.
fn dump_fetch(path: &str, ok: usize, failures: &[(&'static str, SourceError)]) {
    static ROUNDS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let round = ROUNDS.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    let failed: Vec<String> = failures
        .iter()
        .map(|(label, _)| format!("\"{label}\""))
        .collect();
    let line = format!(
        "{{\"round\":{round},\"ok\":{ok},\"failed\":[{}]}}\n",
        failed.join(",")
    );
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = file.write_all(line.as_bytes());
    }
}

/// The concrete HTTP sources the app can build. An enum rather than a trait
/// object keeps the fetch loop cheap and `Send` for the background executor.
enum Fetcher {
    OpenMeteo(OpenMeteoSource),
    OpenAq(OpenAqSource),
    OpenWeatherMap(OpenWeatherMapSource),
}

impl Fetcher {
    fn fetch(&self) -> Result<Series, SourceError> {
        match self {
            Fetcher::OpenMeteo(source) => source.fetch(),
            Fetcher::OpenAq(source) => source.fetch(),
            Fetcher::OpenWeatherMap(source) => source.fetch(),
        }
    }
}

/// One buildable source plus the labels the stack carries.
struct EnabledSource {
    id: &'static str,
    label: &'static str,
    kind: Kind,
    fetcher: Fetcher,
}

pub struct Widget {
    /// Data being shown; `None` until the first successful fetch.
    series: Option<Series>,
    decision: Option<Decision>,
    /// The fused stack behind `series`/`decision` (EXP19): per-source series,
    /// per-hour spread and disagreement. `None` for the single-source mock, so
    /// the pre-EXP19 screenshots stay byte-identical.
    stack: Option<Stack>,
    thresholds: Thresholds,
    /// Last fetch error, if any: shown alone when there is no data yet, or as a
    /// "stale" note beside the last good data.
    error: Option<String>,
    /// The location currently shown / being fetched.
    selected: Location,
    /// Search text typed into the panel (empty = show a hint).
    query: String,
    /// Cities matching `query`, best first (capped at `SEARCH_LIMIT`).
    results: Vec<&'static City>,
    /// Whether the location panel is open.
    dropdown_open: bool,
    /// Scroll position of the results, preserved across frames.
    results_scroll: UniformListScrollHandle,
    /// Focus target for the location trigger (a tab stop; EXP10).
    trigger_focus: FocusHandle,
    /// Focus target for the header's "API keys" button (EXP24).
    keys_focus: FocusHandle,
    /// Focus target for the search field, so keystrokes reach `on_search_key`.
    search_focus: FocusHandle,
    /// Whether we have focused the search field for the current open state.
    search_focused: bool,
    /// Arrow-key-highlighted result row (the Enter target). Not the same as the
    /// persisted selection.
    active: usize,
    /// Caret visibility, flipped by the blink task while the field is focused.
    caret_on: bool,
    /// The caret-blink loop; dropped (cancelled) when the panel closes.
    blink_task: Option<Task<()>>,
    /// The running real-source refresh task. Replacing it drops (cancels) the
    /// previous loop, so switching locations never leaks a refresh.
    fetch_task: Option<Task<()>>,
    /// The hour the pointer is over the chart (`None` when it is off the plot).
    /// Drives the crosshair, dot and tooltip; mouse-only (EXP13).
    hover: Option<usize>,
    /// Bounds of the chart canvas from the last paint, in window space, so a
    /// mouse move can map `x -> hour`. `Cell` because `Bounds<Pixels>` is `Copy`.
    chart_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// Inferred hourly noise (dB(A)) for the selected location, if it has baked
    /// road density and the overlay is enabled. The same 24 values every day
    /// (EXP16).
    noise: Option<[f32; 24]>,
    /// Whether the day/night wave is painted behind the curve (EXP21).
    night: bool,
    /// Whether the a11y-tree dump has already been written this run.
    a11y_dumped: bool,
    /// Whether we have put initial focus on the trigger. The widget must keep
    /// something focused so key events reach the root handler (with nothing
    /// focused, the dispatch path is just the window root).
    initial_focus_done: bool,
    /// The open API-key prompt, if any (EXP24). Two steps: OpenAQ, then
    /// OpenWeatherMap, navigable in both directions with their own buttons.
    prompt: Option<keys::Prompt>,
    /// Which prompt control the keyboard is on: `None` is the text field,
    /// `Some(0)` Prev, `Some(1)` Next, `Some(2)` Remove, `Some(3)` Cancel,
    /// `Some(4)` Save. Buttons are reached with Tab; Enter activates them.
    prompt_action: Option<usize>,
    /// Per-step sequence numbers so a key check that finishes late cannot
    /// overwrite the note of a newer save on the same step.
    key_check_seq: [u64; 2],
    /// The keys currently in the config file (no environment override), so the
    /// prompt can say what is stored and offer Remove.
    file_keys: persist::Keys,
    /// Focus target for the prompt's key field.
    prompt_focus: FocusHandle,
    /// Whether the prompt's field has been focused for the current open state
    /// (focus is set on the next frame, which needs the window).
    prompt_focused: bool,
    /// Whether times render as 24-hour. Clicking the best-window time flips it
    /// for this session only (EXP25).
    clock24: bool,
    /// Focus target for that clock toggle; the only tab stop inside the banner.
    clock_focus: FocusHandle,
    /// How many sources failed in the last round. Non-zero (with data on
    /// screen) draws the header's issue chip instead of a banner line (EXP25).
    issue_count: usize,
    /// For a rejected key, the prompt step that fixes it: clicking the chip
    /// opens that step rather than retrying a request that will fail again.
    issue_step: Option<Step>,
    /// Focus target for the issue chip, so the detail is keyboard reachable.
    issue_focus: FocusHandle,
}

impl Widget {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // Test/demo seams: prefill a search, and/or open the panel at startup.
        let query = std::env::var("AIRQLALITY_SEARCH").unwrap_or_default();
        let results = search(&query, SEARCH_LIMIT);
        let open = std::env::var_os("AIRQLALITY_OPEN_DROPDOWN").is_some() || !query.is_empty();

        let selected = persist::load();
        let noise = noise_for(&selected);

        let mut widget = Self {
            series: None,
            decision: None,
            stack: None,
            thresholds: Thresholds::default(),
            error: None,
            selected,
            query,
            results,
            dropdown_open: open,
            results_scroll: UniformListScrollHandle::default(),
            // `tab_stop(true)` is what makes `focus_next` reach these; `tab_index`
            // fixes the order (trigger before the search field).
            trigger_focus: cx.focus_handle().tab_stop(true).tab_index(0),
            keys_focus: cx.focus_handle().tab_stop(true).tab_index(1),
            search_focus: cx.focus_handle().tab_stop(true).tab_index(2),
            search_focused: false,
            active: 0,
            caret_on: true,
            blink_task: None,
            fetch_task: None,
            hover: None,
            chart_bounds: Rc::new(Cell::new(Bounds::default())),
            noise,
            night: night_enabled(),
            a11y_dumped: false,
            initial_focus_done: false,
            prompt: None,
            prompt_action: None,
            key_check_seq: [0, 0],
            file_keys: persist::load_file_keys(),
            prompt_focus: cx.focus_handle(),
            prompt_focused: false,
            clock24: false,
            clock_focus: cx.focus_handle().tab_stop(true).tab_index(3),
            issue_count: 0,
            issue_step: None,
            issue_focus: cx.focus_handle().tab_stop(true).tab_index(4),
        };

        // Test/demo seam (EXP24): open a key prompt at launch, at step 1 or 2,
        // so the windowed E2E can screenshot each prompt without a tray click.
        if let Ok(which) = std::env::var("AIRQLALITY_KEYS_PROMPT") {
            let step = match which.trim() {
                "2" | "owm" | "openweathermap" => Step::Owm,
                _ => Step::Openaq,
            };
            widget.prompt = Some(keys::Prompt::at(step));
            widget.dump_prompt();
        }

        // The tray menu can reach the widget in every mode (EXP24): Refresh is
        // only enabled when there is a real fetch to re-run, the key prompts
        // always.
        widget.start_command_requests(cx);

        if use_mock() {
            if use_mock_stack() {
                // Three-source deterministic mock (EXP19). No accelerated
                // ticker here: the fusion is built once, so screenshots are
                // reproducible and the fused series never drifts from its
                // sources.
                widget.apply_stack(mock_stack(), Vec::new());
            } else {
                // Deterministic mock: fetch synchronously, then optionally play the
                // accelerated ticker. Set AIRQLALITY_FREEZE_NOW to stop it (the E2E
                // does, so its screenshot stays byte-identical). AIRQLALITY_MOCK_NOW
                // pins the starting hour (the EXP11 E2E uses it to reach an Open one).
                // AIRQLALITY_MOCK_PM25 replaces the mock with a crafted sequence
                // (EXP27, notification segments).
                let mut series = mock_pm2_5_series(&widget.selected.label)
                    .unwrap_or_else(|| MockSource.fetch().expect("mock source is infallible"));
                if let Some(i) = std::env::var("AIRQLALITY_MOCK_NOW")
                    .ok()
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    if i < series.len() {
                        series.now_index = i;
                    }
                }
                widget.set_series(series);
                if std::env::var_os("AIRQLALITY_FREEZE_NOW").is_none() {
                    let tick = tick_period();
                    cx.spawn(async move |this, cx| {
                        loop {
                            cx.background_executor().timer(tick).await;
                            let alive = this.update(cx, |widget, cx| {
                                widget.tick();
                                cx.notify();
                            });
                            if alive.is_err() {
                                break; // the window closed; stop ticking
                            }
                        }
                    })
                    .detach();
                }
            }
        } else {
            // Real mode: start the fetch loop. The mocks have no fetch to
            // re-run, so the tray's Refresh item stays greyed there.
            widget.spawn_real(cx);
        }

        widget
    }

    /// Start (or restart) the real sources for the selected location: Open-Meteo
    /// always, plus OpenAQ and OpenWeatherMap when their keys are configured.
    /// Fetch now, then refresh on a timer. The blocking fetches run on the
    /// background executor, never the UI thread.
    ///
    /// Returns whether a fetch loop is now running: a stack that cannot even be
    /// built (a bad endpoint override, TLS init) leaves the error on screen and
    /// no loop, which the tray's "Refresh" reports honestly instead of
    /// claiming to fetch (EXP22).
    fn spawn_real(&mut self, cx: &mut Context<Self>) -> bool {
        let location = self.selected.clone();
        let keys = persist::load_keys();

        // Open-Meteo is keyless and always on. The endpoint can be overridden so
        // a test can force a deterministic failure (error-state coverage).
        let openmeteo = match std::env::var("AIRQLALITY_OPENMETEO_URL").ok().as_deref() {
            Some(base) => OpenMeteoSource::from_location(&location, base),
            None => OpenMeteoSource::for_location(&location),
        };

        let mut sources: Vec<EnabledSource> = Vec::new();
        match openmeteo {
            Ok(source) => sources.push(EnabledSource {
                id: "open-meteo",
                label: "Open-Meteo",
                kind: Kind::Model,
                fetcher: Fetcher::OpenMeteo(source),
            }),
            Err(e) => {
                self.error = Some(e.to_string());
                return false;
            }
        }

        // A keyed source that fails to build is reported, not silently dropped:
        // the user asked for a stack and deserves to know it is smaller.
        if let Some(key) = keys.openaq.as_deref() {
            match OpenAqSource::for_location(&location, key) {
                Ok(source) => sources.push(EnabledSource {
                    id: "openaq",
                    label: "OpenAQ",
                    kind: Kind::Ground,
                    fetcher: Fetcher::OpenAq(source),
                }),
                Err(e) => {
                    self.error = Some(e.to_string());
                    return false;
                }
            }
        }
        if let Some(key) = keys.owm.as_deref() {
            match OpenWeatherMapSource::for_location(&location, key) {
                Ok(source) => sources.push(EnabledSource {
                    id: "openweathermap",
                    label: "OpenWeatherMap",
                    kind: Kind::Model,
                    fetcher: Fetcher::OpenWeatherMap(source),
                }),
                Err(e) => {
                    self.error = Some(e.to_string());
                    return false;
                }
            }
        }

        // Test seam (EXP22): one JSON line per round, so the E2E can prove that a
        // tray refresh caused a second fetch rather than a repaint.
        let dump = std::env::var("AIRQLALITY_FETCH_DUMP").ok();

        let task = cx.spawn(async move |this, cx| {
            let period = refresh_period().as_secs();
            loop {
                let mut series = Vec::new();
                let mut failures = Vec::new();
                for source in &sources {
                    match source.fetcher.fetch() {
                        Ok(s) => series.push(SourceSeries {
                            id: source.id,
                            label: source.label,
                            kind: source.kind,
                            series: s,
                        }),
                        Err(e) => failures.push((source.label, e)),
                    }
                }
                if let Some(path) = dump.as_deref() {
                    dump_fetch(path, series.len(), &failures);
                }
                let alive = this.update(cx, |widget, cx| {
                    widget.apply_stack(series, failures);
                    cx.notify();
                });
                if alive.is_err() {
                    break; // the window closed; stop refreshing
                }
                // Sleep to the next wall-clock boundary (:00 / :30), not a fixed
                // period after this fetch, so ticks stay aligned to the clock
                // whatever minute the app launched (EXP18). Re-read `now` each
                // time so the loop self-corrects.
                cx.background_executor()
                    .timer(until_next_boundary(now_unix_secs(), period))
                    .await;
            }
        });
        self.fetch_task = Some(task);
        true
    }

    /// Publish the tray's command channel and await it in one idle task (EXP22,
    /// EXP24). Not a timer: the task costs nothing until a menu item sends a
    /// request. Refresh is enabled only when there is a real fetch to re-run;
    /// the key prompts are always available.
    fn start_command_requests(&mut self, cx: &mut Context<Self>) {
        let (sender, receiver) = commands::channel();
        commands::publish(sender, !use_mock());
        cx.spawn(async move |this, cx| loop {
            let Ok(command) = receiver.recv().await else {
                break; // every sender is gone: nothing can ask again
            };
            commands::done(command);
            let alive = this.update(cx, |widget, cx| match command {
                Command::Refresh => {
                    if use_mock() {
                        cx.notify(); // no fetch to re-run; the item is greyed
                    } else {
                        widget.refresh_now(cx);
                    }
                }
                Command::ApiKeys => widget.open_keys(Step::Openaq, cx),
            });
            if alive.is_err() {
                break; // the window closed
            }
        })
        .detach();
    }

    /// Open the key prompts at `step` without touching a window: focus and the
    /// caret are put in place on the next frame, like the location panel's
    /// startup seam. Closes the location panel first, since both own the same
    /// overlay space and the same caret.
    fn open_keys(&mut self, step: Step, cx: &mut Context<Self>) {
        self.dropdown_open = false;
        self.search_focused = false;
        self.file_keys = persist::load_file_keys();
        self.prompt = Some(keys::Prompt::at(step));
        self.prompt_action = None;
        self.prompt_focused = false;
        self.dump_prompt();
        cx.notify();
    }

    /// Close the prompts. `apply` refetches (real mode only), so keys saved in
    /// either step take effect immediately, without a restart.
    fn close_keys(&mut self, apply: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.prompt = None;
        self.prompt_action = None;
        self.prompt_focused = false;
        self.caret_on = true;
        self.blink_task = None;
        window.focus(&self.trigger_focus, cx);
        if apply && !use_mock() {
            self.refresh_now(cx);
        } else {
            cx.notify();
        }
    }

    /// The stored (file) key for `step`, if any.
    fn file_key(&self, step: Step) -> Option<&str> {
        match step {
            Step::Openaq => self.file_keys.openaq.as_deref(),
            Step::Owm => self.file_keys.owm.as_deref(),
        }
    }

    /// Test seam (EXP24): after every prompt state change, write which step is
    /// showing and which sources have a stored key, so the windowed E2E can
    /// prove navigation and saving without reading pixels as text.
    fn dump_prompt(&self) {
        let Ok(path) = std::env::var("AIRQLALITY_KEYS_DUMP") else {
            return;
        };
        let Some(prompt) = self.prompt.as_ref() else {
            return;
        };
        let saved: Vec<&str> = [
            ("openaq", self.file_keys.openaq.is_some()),
            ("owm", self.file_keys.owm.is_some()),
        ]
        .into_iter()
        .filter(|(_, on)| *on)
        .map(|(name, _)| name)
        .collect();
        let saved = saved
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(",");
        let json = format!(
            "{{\"step\":{},\"saved\":[{saved}]}}",
            prompt.step().position()
        );
        let _ = std::fs::write(path, json);
    }

    /// Save the current step (Enter / Save). It stays on this step: navigating
    /// is its own pair of buttons, so saving one key never pushes the user into
    /// the other. An empty field changes nothing; a partial paste, or the other
    /// source's key, is refused with a reason.
    fn save_prompt(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.prompt.as_ref() else {
            return;
        };
        let step = prompt.step();
        let value = match prompt.validate() {
            Ok(value) => value,
            Err(reason) => {
                if let Some(prompt) = self.prompt.as_mut() {
                    prompt.set_note(keys::Note::warning(reason));
                }
                self.dump_prompt();
                cx.notify();
                return;
            }
        };
        let Some(key) = value else {
            if let Some(prompt) = self.prompt.as_mut() {
                prompt.set_note(keys::Note::info(
                    "nothing entered \u{2014} the stored key is unchanged",
                ));
            }
            self.dump_prompt();
            cx.notify();
            return;
        };

        // The other slot's key: the mistake that sent the OpenAQ key to
        // OpenWeatherMap and produced a 401 an hour later (EXP24 report).
        let (other, other_title) = match step {
            Step::Openaq => (self.file_keys.owm.clone(), Step::Owm.title()),
            Step::Owm => (self.file_keys.openaq.clone(), Step::Openaq.title()),
        };
        if keys::duplicate_of(&key, other.as_deref()) {
            let text = format!("{other_title} already holds this key \u{2014} each source needs its own");
            if let Some(prompt) = self.prompt.as_mut() {
                prompt.set_note(keys::Note::warning(&text));
            }
            self.dump_prompt();
            cx.notify();
            return;
        }

        let mut keys = self.file_keys.clone();
        match step {
            Step::Openaq => keys.openaq = Some(key.clone()),
            Step::Owm => keys.owm = Some(key.clone()),
        }
        persist::save_keys(&keys);
        self.file_keys = keys;
        self.prompt_action = None;
        if let Some(prompt) = self.prompt.as_mut() {
            prompt.clear();
            prompt.set_note(if use_mock() {
                keys::Note::saved(&keys::tail(&key))
            } else {
                keys::Note::checking()
            });
        }
        if !use_mock() {
            self.start_key_check(step, &key, cx);
        }
        self.dump_prompt();
        cx.notify();
    }

    /// One cheap request with the just-saved key, on the background executor.
    /// The result lands on *that* step's note, even if the user has navigated
    /// away; a newer save invalidates it through the per-step sequence number.
    fn start_key_check(&mut self, step: Step, key: &str, cx: &mut Context<Self>) {
        let location = self.selected.clone();
        let key = key.to_string();
        self.key_check_seq[step.index()] += 1;
        let seq = self.key_check_seq[step.index()];
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    match step {
                        Step::Openaq => OpenAqSource::for_location(&location, &key)
                            .and_then(|source| source.check_key()),
                        Step::Owm => OpenWeatherMapSource::for_location(&location, &key)
                            .and_then(|source| source.check_key()),
                    }
                })
                .await;
            let _ = this.update(cx, |widget, cx| {
                if widget.key_check_seq[step.index()] != seq {
                    return; // a newer save for this step owns the note
                }
                let note = match result {
                    Ok(()) => keys::Note::accepted(),
                    Err(SourceError::Rejected(status)) => keys::Note::rejected(&status),
                    Err(error) => keys::Note::warning(&format!("could not check: {error}")),
                };
                if let Some(prompt) = widget.prompt.as_mut() {
                    prompt.set_note_at(step, note);
                }
                widget.dump_prompt();
                cx.notify();
            });
        })
        .detach();
    }

    /// Delete the stored key for the current step (Remove). Stays on the step.
    fn remove_prompt_key(&mut self, cx: &mut Context<Self>) {
        let Some(step) = self.prompt.as_ref().map(|p| p.step()) else {
            return;
        };
        let mut keys = self.file_keys.clone();
        match step {
            Step::Openaq => keys.openaq = None,
            Step::Owm => keys.owm = None,
        }
        persist::save_keys(&keys);
        self.file_keys = keys;
        self.key_check_seq[step.index()] += 1; // cancel any check in flight
        self.prompt_action = None;
        if let Some(prompt) = self.prompt.as_mut() {
            prompt.clear();
            prompt.set_note(keys::Note::info(
                "removed \u{2014} this source is off until a key is saved",
            ));
        }
        self.dump_prompt();
        cx.notify();
    }

    /// Switch the prompt between steps. Never saves and never clears: each step
    /// keeps its own field.
    fn nav_prompt(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(prompt) = self.prompt.as_mut() else {
            return;
        };
        let moved = if forward { prompt.next() } else { prompt.prev() };
        if !moved {
            return;
        }
        self.prompt_action = None;
        self.dump_prompt();
        cx.notify();
    }

    /// Keys that belong to the prompt. Tab cycles the field and the enabled
    /// buttons; typing only reaches the field.
    fn on_prompt_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(prompt) = self.prompt.as_ref() else {
            return;
        };
        let step = prompt.step();
        let can_prev = prompt.can_prev();
        let can_next = prompt.can_next();
        let ctrl = ev.keystroke.modifiers.control || ev.keystroke.modifiers.platform;
        let action = self.prompt_action;
        let has_key = self.file_key(step).is_some();
        match ev.keystroke.key.as_str() {
            "tab" => {
                self.prompt_action = keys::next_action(action, can_prev, can_next, has_key);
                cx.stop_propagation();
                cx.notify();
            }
            "escape" => {
                self.close_keys(true, window, cx);
                cx.stop_propagation();
            }
            // Ctrl+Left / Ctrl+Right: the same Prev/Next the buttons offer.
            "left" if ctrl => {
                self.nav_prompt(false, cx);
                cx.stop_propagation();
            }
            "right" if ctrl => {
                self.nav_prompt(true, cx);
                cx.stop_propagation();
            }
            "enter" | "space" if action.is_some() => {
                self.activate_prompt_action(window, cx);
                cx.stop_propagation();
            }
            "enter" => {
                self.save_prompt(cx);
                cx.stop_propagation();
            }
            "backspace" if action.is_none() => {
                if let Some(prompt) = self.prompt.as_mut() {
                    prompt.backspace();
                }
                cx.notify();
            }
            // Ctrl+V, the path nearly every key takes: copy it from the
            // provider's page. The platform hands us the whole clipboard.
            "v" if ctrl && action.is_none() => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    if let Some(prompt) = self.prompt.as_mut() {
                        prompt.push_text(&text);
                    }
                }
                cx.notify();
            }
            // No other chord types into a key field (Ctrl+K must not leave a
            // stray "k" in the buffer).
            _ if ctrl => {
                cx.stop_propagation();
            }
            _ => {
                // Typing always goes to the field, even when Tab has put the
                // keyboard on a button: a character means "edit the key".
                if let Some(ch) = ev.keystroke.key_char.as_deref() {
                    if !ch.is_empty() && !ch.chars().any(char::is_control) {
                        self.prompt_action = None;
                        if let Some(prompt) = self.prompt.as_mut() {
                            prompt.push_text(ch);
                        }
                        cx.notify();
                    }
                }
            }
        }
    }

    /// Enter/Space on a focused prompt button.
    fn activate_prompt_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.prompt_action {
            Some(0) => self.nav_prompt(false, cx),
            Some(1) => self.nav_prompt(true, cx),
            Some(2) => {
                if let Some(step) = self.prompt.as_ref().map(|prompt| prompt.step()) {
                    // Same rule as the mouse path: the shell call must not run
                    // inside gpui's dispatch (it pumps messages). EXP24 crash.
                    let url = step.url().to_string();
                    std::thread::spawn(move || {
                        browser::open(&url);
                    });
                    self.dump_prompt();
                }
            }
            Some(3) => self.remove_prompt_key(cx),
            Some(4) => self.close_keys(true, window, cx),
            _ => self.save_prompt(cx),
        }
    }

    /// Rebuild and re-fetch everything for the selected location right now: the
    /// tray menu's "Refresh" (EXP22).
    ///
    /// Re-running [`Self::spawn_real`] is deliberate - it re-reads the keys,
    /// rebuilds every source (so a source that failed to build is retried) and
    /// re-anchors the timer to the next `:00`/`:30`, so a manual refresh never
    /// moves the schedule. A not-yet-loaded widget drops the old error first, so
    /// the window says `FETCHING...` instead of repeating the failure the user
    /// is trying to clear.
    fn refresh_now(&mut self, cx: &mut Context<Self>) {
        if self.series.is_none() {
            self.error = None;
            self.issue_count = 0;
            self.issue_step = None;
        }
        if self.spawn_real(cx) {
            tray_set_refreshing(&self.selected.label);
        } else {
            // Nothing could be built; `spawn_real` already stored the reason.
            tray_set_no_data(&self.selected.label);
        }
        cx.notify();
    }

    /// Toggle the panel (trigger click / Enter / Ctrl+K).
    fn toggle_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dropdown_open {
            self.close_panel(window, cx);
        } else {
            self.open_search(window, cx);
        }
    }

    /// Open the panel and focus the search field, ready for typing. Starts the
    /// caret blink loop.
    fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dropdown_open = true;
        self.search_focused = true;
        self.caret_on = true;
        window.focus(&self.search_focus, cx);
        self.start_blink(cx);
        cx.notify();
    }

    /// Close the panel and hand focus back to the trigger, so a keyboard user
    /// does not land on nothing when the panel disappears.
    fn close_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dropdown_open = false;
        self.search_focused = false;
        self.caret_on = true;
        self.blink_task = None; // dropping the task cancels the loop
        window.focus(&self.trigger_focus, cx);
        cx.notify();
    }

    /// Start the caret blink loop if it is not already running. The task stops
    /// toggling when the panel closes; `close_panel` drops it to cancel.
    fn start_blink(&mut self, cx: &mut Context<Self>) {
        // Test/demo seam: a steady caret makes the screenshot deterministic.
        if std::env::var_os("AIRQLALITY_NO_BLINK").is_some() {
            self.caret_on = true;
            return;
        }
        if self.blink_task.is_some() {
            return;
        }
        let task = cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(BLINK).await;
            let alive = this.update(cx, |widget, cx| {
                if widget.dropdown_open {
                    widget.caret_on = !widget.caret_on;
                    cx.notify();
                } else {
                    widget.caret_on = true;
                }
            });
            if alive.is_err() {
                break; // the window closed
            }
        });
        self.blink_task = Some(task);
    }

    /// Global key handling, on the root element so it fires wherever focus is
    /// (key events bubble from the focused node to the root).
    fn on_root_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let shift = ev.keystroke.modifiers.shift;
        let ctrl = ev.keystroke.modifiers.control || ev.keystroke.modifiers.platform;
        // While the key prompts are open they own the keyboard: Esc, Enter, Tab
        // and every character belong to the step, not to the widget (EXP24).
        if self.prompt.is_some() {
            self.on_prompt_key(ev, window, cx);
            return;
        }
        match ev.keystroke.key.as_str() {
            // Move the keyboard focus across the tab stops. gpui does not bind
            // Tab itself, so this is the app's job.
            "tab" => {
                if shift {
                    window.focus_prev(cx);
                } else {
                    window.focus_next(cx);
                }
                cx.stop_propagation();
            }
            "escape" if self.dropdown_open => {
                self.close_panel(window, cx);
                cx.stop_propagation();
            }
            "k" if ctrl && shift => {
                self.open_keys(Step::Openaq, cx);
                cx.stop_propagation();
            }
            "k" if ctrl => {
                self.toggle_open(window, cx);
                cx.stop_propagation();
            }
            "q" if ctrl => {
                cx.quit();
                cx.stop_propagation();
            }
            _ => {}
        }
    }

    /// Recompute `results` from `query`; typing resets the active row to the top.
    fn refresh_results(&mut self) {
        self.results = search(&self.query, SEARCH_LIMIT);
        self.active = 0;
        self.results_scroll.scroll_to_item(0, ScrollStrategy::Top);
    }

    /// Move the arrow-key highlight by `delta`, clamped to the results.
    fn move_active(&mut self, delta: isize) {
        if self.results.is_empty() {
            return;
        }
        let n = self.results.len() as isize;
        let next = (self.active as isize + delta).clamp(0, n - 1);
        self.set_active(next as usize);
    }

    /// Set the highlighted row and scroll it into view.
    fn set_active(&mut self, index: usize) {
        if self.results.is_empty() {
            return;
        }
        self.active = index.min(self.results.len() - 1);
        self.results_scroll
            .scroll_to_item(self.active, ScrollStrategy::Center);
    }

    /// Handle a key in the search field (the combobox keys). Tab is deliberately
    /// not consumed: it bubbles to the root, which moves focus.
    fn on_search_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev.keystroke.key.as_str() {
            // Focus management is global; let Tab bubble.
            "tab" => {}
            "escape" => {
                self.close_panel(window, cx);
                cx.stop_propagation();
                return;
            }
            "enter" => {
                if let Some(city) = self.results.get(self.active).copied() {
                    self.select_city(city, window, cx);
                    return;
                }
            }
            "up" => self.move_active(-1),
            "down" => self.move_active(1),
            "home" => self.set_active(0),
            "end" => {
                if !self.results.is_empty() {
                    self.set_active(self.results.len() - 1);
                }
            }
            "backspace" => {
                self.query.pop();
                self.refresh_results();
            }
            _ => {
                // Append a typed character (key_char is None for Ctrl/Cmd chords).
                if let Some(ch) = ev.keystroke.key_char.as_deref() {
                    if !ch.is_empty() && !ch.chars().any(char::is_control) {
                        self.query.push_str(ch);
                        self.refresh_results();
                    }
                }
            }
        }
        cx.notify();
    }

    /// Choose a city: persist it, close the panel, and (real mode only) re-point
    /// the source and refetch. In mock mode the label changes only, so the
    /// deterministic screenshot stays reproducible.
    fn select_city(&mut self, city: &City, window: &mut Window, cx: &mut Context<Self>) {
        self.dropdown_open = false;
        self.search_focused = false;
        self.caret_on = true;
        self.blink_task = None;
        self.query.clear();
        self.results.clear();
        self.active = 0;
        window.focus(&self.trigger_focus, cx);

        let location = Location::from_city(city);
        if location == self.selected {
            cx.notify();
            return;
        }
        self.selected = location;
        persist::save(&self.selected);
        self.noise = noise_for(&self.selected);

        if !use_mock() {
            // Drop stale data so the previous location's numbers never linger.
            self.series = None;
            self.decision = None;
            self.stack = None;
            self.error = None;
            self.issue_count = 0;
            self.issue_step = None;
            tray_set_loading(&self.selected.label);
            self.spawn_real(cx);
        }
        cx.notify();
    }

    fn set_series(&mut self, series: Series) {
        let decision = evaluate(&series, &self.thresholds);
        tray_set_verdict(
            decision.headline,
            &self.selected.label,
            series.now().pm2_5,
            hour_bucket(&series.now().time),
        );
        self.decision = Some(decision);
        self.series = Some(series);
    }

    /// Apply a fetch round: fuse whatever came back.
    ///
    /// A source can fail in two very different ways, and the user should only be
    /// told about one of them:
    ///
    /// - [`SourceError::Unavailable`] is a *coverage* fact ("OpenAQ has no fresh
    ///   station within 25 km of Cairo"). It is not an outage, it will not fix
    ///   itself, and repeating it on every launch is noise - so it is dropped
    ///   when other data is showing, and only surfaced when there is nothing at
    ///   all to show.
    /// - anything else (network, parse) is a real failure: the last good fused
    ///   data is kept and the failure becomes the header's issue chip (EXP25).
    fn apply_stack(&mut self, sources: Vec<SourceSeries>, failures: Vec<(&'static str, SourceError)>) {
        let mut outages: Vec<String> = Vec::new();
        let mut all: Vec<String> = Vec::new();
        // The first refused key decides where the issue chip sends the user: a
        // retry cannot fix a refused key, but the prompt can (EXP25).
        let mut rejected_step: Option<Step> = None;
        for (label, error) in failures {
            if matches!(error, SourceError::Rejected(_)) && rejected_step.is_none() {
                rejected_step = match label {
                    "OpenAQ" => Some(Step::Openaq),
                    "OpenWeatherMap" => Some(Step::Owm),
                    _ => None,
                };
            }
            let text = match &error {
                // A refused key is not an outage and will not fix itself; say
                // what it is and where to fix it (EXP24). A brand-new
                // OpenWeatherMap key is invalid until it activates.
                SourceError::Rejected(status) => {
                    let hint = if label == "OpenWeatherMap" {
                        "check the key; a new key can take up to 2 hours to activate"
                    } else {
                        "check the key in the API-key prompt"
                    };
                    format!("{label}: key rejected ({status}) \u{2014} {hint}")
                }
                _ => format!("{label}: {error}"),
            };
            if !matches!(error, SourceError::Unavailable(_)) {
                outages.push(text.clone());
            }
            all.push(text);
        }

        if sources.is_empty() {
            // Nothing at all: surface every reason, coverage included, so an
            // unusable location does not look like an empty screen.
            self.error = Some(if all.is_empty() {
                "no data sources are configured".to_string()
            } else {
                all.join(" \u{b7} ")
            });
            // Without data the message screen owns the story, not a chip.
            self.issue_count = 0;
            self.issue_step = None;
            // The tray must not sit on "refreshing..." after a round that brought
            // nothing back (EXP22) - but only when there is no data on screen to
            // keep: with stale data the last verdict is still the honest icon.
            if self.series.is_none() {
                tray_set_no_data(&self.selected.label);
            }
            return;
        }

        match fuse_stack(sources, &self.thresholds) {
            Ok(stack) => {
                let pm2_5 = stack.fusion.series.now().pm2_5;
                let hour = hour_bucket(&stack.fusion.series.now().time);
                tray_set_verdict(stack.decision.headline, &self.selected.label, pm2_5, hour);
                self.series = Some(stack.fusion.series.clone());
                self.decision = Some(stack.decision.clone());
                self.stack = Some(stack);
                self.issue_count = outages.len();
                self.issue_step = rejected_step;
                self.error = if outages.is_empty() {
                    None
                } else {
                    Some(outages.join(" \u{b7} "))
                };
            }
            Err(e) => {
                self.error = Some(e.to_string());
                self.issue_count = 1;
                self.issue_step = None;
            }
        }
    }

    /// The sources named in the footer credit. The strip below the banner lists
    /// every source with its value, so the credit only needs the count - three
    /// names would crowd the caption. With a single source it is exactly
    /// "Open-Meteo", as before (EXP19, moved to the footer in EXP25).
    fn source_label(&self) -> String {
        match &self.stack {
            Some(stack) if stack.multi() => format!("{} sources", stack.fusion.sources.len()),
            _ => "Open-Meteo".to_string(),
        }
    }

    /// Advance the mock post one hour and re-evaluate, ready for the next repaint.
    fn tick(&mut self) {
        if let Some(series) = self.series.as_mut() {
            series.advance_now();
        }
        if let Some(series) = self.series.as_ref() {
            let decision = evaluate(series, &self.thresholds);
            tray_set_verdict(
                decision.headline,
                &self.selected.label,
                series.now().pm2_5,
                hour_bucket(&series.now().time),
            );
            self.decision = Some(decision);
        }
    }

    /// Map a pointer position (window space) to the chart hour under it and
    /// remember it. Called from the chart's `on_mouse_move`.
    fn set_hover_from_pointer(&mut self, pos: &Point<Pixels>, cx: &mut Context<Self>) {
        let next = self.series.as_ref().and_then(|series| {
            let n = series.len();
            ChartGeom::new(self.chart_bounds.get(), n, 0.0).index_at_x(f32::from(pos.x))
        });
        if next != self.hover {
            self.hover = next;
            cx.notify();
        }
    }

    /// Clear the hover highlight when the pointer leaves the chart.
    fn clear_hover(&mut self, cx: &mut Context<Self>) {
        if self.hover.take().is_some() {
            cx.notify();
        }
    }

    /// The verdict banner: what the air is now, the best window (the time is the
    /// clock toggle), the day's census and the one-line why. The verdict word
    /// lives in the header, and a failed source is the header chip, so neither is
    /// repeated here (EXP25).
    fn banner(
        &self,
        series: &Series,
        decision: &Decision,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let pm_now = series.now().pm2_5;
        let use24 = self.clock24;

        let first: AnyElement = match decision.best_window {
            Some(w) => {
                let start = clock(&series.points[w.start].time, use24);
                let end = clock(&series.points[w.end].time, use24);
                let hours = w.hours();
                let toggle_label = format!(
                    "Best window {start} to {end}, {hours} hours. Switch to the {} clock.",
                    if use24 { "12-hour" } else { "24-hour" }
                );
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_sm()
                    .text_color(rgb(TEXT))
                    .child(format!(
                        "PM2.5 {pm_now:.0} \u{b5}g/m\u{b3} now  \u{b7}  best window"
                    ))
                    .child(
                        div()
                            .id("clock-toggle")
                            .role(Role::Button)
                            .aria_label(toggle_label)
                            .whitespace_nowrap()
                            .cursor_pointer()
                            .border_b_1()
                            .border_color(rgb(AXIS))
                            .track_focus(&self.clock_focus)
                            .focus_visible(|s| s.border_color(rgb(FOCUS)).text_color(rgb(TEXT)))
                            .hover(|s| s.text_color(rgb(TEXT)).border_color(rgb(FOCUS)))
                            .on_click(cx.listener(|this, _ev, _window, cx| {
                                this.clock24 = !this.clock24;
                                cx.notify();
                            }))
                            .child(format!("{start} \u{2013} {end}")),
                    )
                    .child(format!("({hours} h)"))
                    .into_any_element()
            }
            None => div()
                .text_sm()
                .text_color(rgb(TEXT))
                .child(format!(
                    "PM2.5 {pm_now:.0} \u{b5}g/m\u{b3} now  \u{b7}  no clean window in the next 24 h"
                ))
                .into_any_element(),
        };

        let mut banner = div()
            .flex()
            .flex_col()
            .gap_1()
            .px_4()
            .py_3()
            .child(first)
            // The day's shape (EXP20): a census, not a run, so a long clean
            // stretch can never be reported without the closed hours around it.
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(day_shape_line(series, decision, use24)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(decision.headline.advice()),
            );

        // Only a real stack adds a row, so single-source rendering is unchanged.
        if let Some(stack) = self.stack.as_ref().filter(|stack| stack.multi()) {
            banner = banner.child(sources_strip(stack));
        }

        banner
    }

    /// The header. Its left side is the verdict status: a coloured square plus
    /// the status word, moved up from the banner so the label reads as a single
    /// row. Its right side is the location trigger: click to open the panel. The
    /// label comes from the selected location, not the fetched series, so it is
    /// correct even while loading.
    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let label = &self.selected.label;
        let caret = if self.dropdown_open { "\u{25b4}" } else { "\u{25be}" };

        // Verdict status, shown once data has been evaluated; empty while
        // loading or after a fetch error (the message screen covers those).
        // Given a stable id + Label role so a screen reader announces the state
        // as one node (the colour square alone is meaningless to AT).
        let status: AnyElement = match &self.decision {
            Some(decision) => div()
                .id("verdict-status")
                .role(Role::Label)
                .aria_label(decision.headline.label())
                .flex()
                .items_center()
                .gap_2()
                .text_xl()
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(verdict_color(decision.headline)))
                .child(
                    div()
                        .w(px(12.0))
                        .h(px(12.0))
                        .bg(rgb(verdict_color(decision.headline))),
                )
                .child(decision.headline.label())
                .into_any_element(),
            None => div().into_any_element(),
        };

        // While data is on screen, a failed source is a small chip beside the
        // verdict instead of a banner line: the detail is still one hover or
        // keypress away, and the chart keeps the row it used to cost (EXP25).
        let issue: AnyElement = match (&self.error, self.series.is_some(), self.issue_count) {
            (Some(message), true, count) if count > 0 => {
                let tip = message.clone();
                div()
                    .id("source-issue")
                    .role(Role::Button)
                    .aria_label(format!("{} {}", issue_label(count), message))
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_1()
                    .py(px(1.0))
                    .text_xs()
                    .text_color(rgb(VENT))
                    .cursor_pointer()
                    .border_1()
                    .border_color(rgb(AXIS))
                    .rounded_sm()
                    .track_focus(&self.issue_focus)
                    .focus_visible(|s| s.border_color(rgb(FOCUS)).text_color(rgb(TEXT)))
                    .hover(|s| s.bg(rgb(PANEL_HOVER)).text_color(rgb(TEXT)))
                    .tooltip(move |_, cx| {
                        let text = tip.clone();
                        cx.new(|_| Tip { text }).into()
                    })
                    .on_click(cx.listener(|this, _ev, _window, cx| {
                        match this.issue_step {
                            // A refused key will keep refusing: go where it is fixed.
                            Some(step) => this.open_keys(step, cx),
                            None => this.refresh_now(cx),
                        }
                    }))
                    .child(div().w(px(6.0)).h(px(6.0)).bg(rgb(VENT)))
                    .child(issue_label(count))
                    .into_any_element()
            }
            _ => div().into_any_element(),
        };

        div()
            .flex()
            .justify_between()
            .items_center()
            .px_4()
            .h(px(HEADER_H))
            .text_sm()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(status)
                    .child(issue),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    // The keys entry point lives in the header so it is visible
                    // without knowing the shortcut (EXP24): the tray item and
                    // Ctrl+Shift+K are not discoverable on their own.
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .px_1()
                            .py(px(2.0))
                            .text_color(rgb(MUTED))
                            .cursor_pointer()
                            .border_2()
                            .border_color(rgb(BG))
                            .rounded_sm()
                            .track_focus(&self.keys_focus)
                            .focus_visible(|s| s.border_color(rgb(FOCUS)).text_color(rgb(TEXT)))
                            .hover(|s| s.text_color(rgb(TEXT)))
                            .id("api-keys")
                            .role(Role::Button)
                            .aria_label("API keys")
                            .aria_keyshortcuts("Ctrl+Shift+K")
                            .on_click(cx.listener(|this, _ev, _window, cx| {
                                this.open_keys(Step::Openaq, cx)
                            }))
                            .child("API keys"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .text_color(rgb(MUTED))
                            .cursor_pointer()
                            // A 2px background-coloured border keeps the layout
                            // stable while giving a thick ring when focus swaps
                            // its colour in.
                            .border_2()
                            .border_color(rgb(BG))
                            .rounded_sm()
                            .track_focus(&self.trigger_focus)
                            .focus_visible(|s| s.border_color(rgb(FOCUS)).text_color(rgb(TEXT)))
                            .hover(|s| s.text_color(rgb(TEXT)))
                            .id("location-trigger")
                            .role(Role::Button)
                            .aria_label("Change location")
                            .aria_expanded(self.dropdown_open)
                            .aria_keyshortcuts("Ctrl+K")
                            .on_click(cx.listener(|this, _ev, window, cx| this.toggle_open(window, cx)))
                            .child(label.clone())
                            .child(caret),
                    ),
            )
    }

    /// The open panel: a click-away backdrop plus a search field and a bounded,
    /// virtualized result list at the top-right. Deferred so it draws above the
    /// chart; `occlude` keeps clicks off whatever is behind each layer.
    /// `focused` is whether the search field actually holds focus this frame (so
    /// the caret only shows when it does).
    fn dropdown(&self, focused: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.weak_entity();
        let selected_id = self.selected.id.clone();
        let results = self.results.clone();
        let active = self.active;
        let caret_on = focused && self.caret_on;
        let placeholder = "Search a city\u{2026}";

        let field = div()
            .flex()
            .items_center()
            .px_2()
            .h(px(ROW_H))
            .border_b_1()
            .border_color(rgb(AXIS))
            .track_focus(&self.search_focus)
            .cursor_text()
            .hover(|s| s.bg(rgb(PANEL_HOVER)))
            .focus_visible(|s| s.bg(rgb(PANEL_HOVER)).border_color(rgb(FOCUS)))
            .id("location-search")
            .role(Role::TextInput)
            .aria_label("Search a city")
            .aria_placeholder(placeholder)
            .aria_value(self.query.clone())
            .on_key_down(cx.listener(|this, ev, window, cx| this.on_search_key(ev, window, cx)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _ev, window, cx| window.focus(&this.search_focus, cx)),
            )
            .child(if self.query.is_empty() {
                div()
                    .flex()
                    .items_center()
                    .text_sm()
                    .text_color(rgb(MUTED))
                    .child(placeholder)
                    .when(caret_on, |d| d.child(CARET))
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .items_center()
                    .text_sm()
                    .text_color(rgb(TEXT))
                    .child(self.query.clone())
                    .when(caret_on, |d| d.child(CARET))
                    .into_any_element()
            });

        let body: AnyElement = if self.query.trim().chars().count() < 2 {
            hint_row("Type at least 2 letters").into_any_element()
        } else if results.is_empty() {
            hint_row("No matching city").into_any_element()
        } else {
            let list = uniform_list(
                "location-results",
                results.len(),
                move |range, _window, _cx| {
                    range
                        .map(|i| {
                            let weak = weak.clone();
                            let city = results[i];
                            let is_selected = city.id.to_string() == selected_id;
                            let is_active = i == active;
                            div()
                                .id(("location-row", i))
                                .w_full()
                                .h(px(ROW_H))
                                .px_3()
                                .flex()
                                .items_center()
                                .cursor_pointer()
                                .text_sm()
                                .text_color(rgb(if is_selected || is_active { TEXT } else { MUTED }))
                                .when(is_active, |d| d.bg(rgb(PANEL_ACTIVE)))
                                .hover(|s| s.bg(rgb(PANEL_HOVER)))
                                .role(Role::ListItem)
                                .aria_label(format!(
                                    "{}, {}",
                                    city.name,
                                    country_name(&city.country)
                                ))
                                .aria_selected(is_selected || is_active)
                                .on_click(move |_ev, window, cx: &mut gpui::App| {
                                    let _ = weak
                                        .update(cx, |this, cx| this.select_city(city, window, cx));
                                })
                                .child(format!("{}, {}", city.name, country_name(&city.country)))
                        })
                        .collect::<Vec<_>>()
                },
            )
            .track_scroll(&self.results_scroll)
            .h(px(RESULTS_H))
            .w_full();
            div()
                .id("location-results-box")
                .role(Role::ListBox)
                .aria_label("Search results")
                .w_full()
                .child(list)
                .into_any_element()
        };

        deferred(
            div()
                .absolute()
                .inset_0()
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .occlude()
                        .id("location-backdrop")
                        .on_click(cx.listener(|this, _ev, window, cx| this.close_panel(window, cx))),
                )
                .child(
                    div()
                        .absolute()
                        .top(px(HEADER_H))
                        .right(px(12.0))
                        .w(px(260.0))
                        .bg(rgb(PANEL))
                        .border_1()
                        .border_color(rgb(AXIS))
                        .rounded_md()
                        .overflow_hidden()
                        .occlude()
                        .child(field)
                        .child(body)
                        .child(
                            div()
                                .px_3()
                                .py_1()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child(
                                    "city data \u{a9} GeoNames (CC BY 4.0)  \u{b7}  noise \
                                     modelled from GRIP4 (ODbL)",
                                ),
                        ),
                ),
        )
    }

    /// The two-step API-key prompt (EXP24): a modal over the widget, one source
    /// per step, a masked field, its own Prev/Next row, and a separate
    /// Remove/Cancel/Save row. Deferred and occluding like the location panel,
    /// so it draws above the chart and eats clicks meant for what is behind it.
    fn keys_prompt(&self, caret: bool, cx: &mut Context<Self>) -> AnyElement {
        let Some(prompt) = self.prompt.as_ref() else {
            return div().into_any_element();
        };
        let step = prompt.step();
        let current = self.file_key(step);
        let has_key = current.is_some();
        let can_prev = prompt.can_prev();
        let can_next = prompt.can_next();
        let action = self.prompt_action;
        let on_field = action.is_none();
        // An environment variable cannot be changed from here, so say so rather
        // than let a saved key silently lose to it.
        let env_set = std::env::var(step.env_var())
            .ok()
            .is_some_and(|v| !v.trim().is_empty());

        let display: AnyElement = if prompt.is_empty() && !caret {
            div()
                .text_sm()
                .text_color(rgb(MUTED))
                .child("paste or type the key")
                .into_any_element()
        } else {
            div()
                .flex()
                .items_center()
                .text_sm()
                .text_color(rgb(TEXT))
                .child(prompt.masked())
                .when(caret, |d| d.child(CARET))
                .into_any_element()
        };

        let field = div()
            .flex()
            .items_center()
            .justify_between()
            .px_2()
            .h(px(28.0))
            .bg(rgb(BG))
            .border_1()
            .border_color(rgb(if on_field { FOCUS } else { AXIS }))
            .rounded_sm()
            .cursor_text()
            .track_focus(&self.prompt_focus)
            .id("keys-field")
            .role(Role::TextInput)
            .aria_label(step.field_label())
            .aria_placeholder("paste or type the key")
            // Never the key itself, not even masked differently: the same
            // bullets the eye sees.
            .aria_value(prompt.masked())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _ev, window, cx| {
                    this.prompt_action = None;
                    window.focus(&this.prompt_focus, cx);
                    cx.notify();
                }),
            )
            .child(display)
            .child(
                div()
                    .id("keys-clear")
                    .role(Role::Button)
                    .aria_label("Clear what has been typed")
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .cursor_pointer()
                    .hover(|s| s.text_color(rgb(TEXT)))
                    .on_click(cx.listener(|this, _ev, _window, cx| {
                        if let Some(prompt) = this.prompt.as_mut() {
                            prompt.clear();
                        }
                        cx.notify();
                    }))
                    .child("clear"),
            );

        // A note (saved / accepted / rejected) outranks the stored-key hint.
        let hint: AnyElement = match prompt.note() {
            Some(note) => div()
                .text_xs()
                .text_color(rgb(tone_color(note.tone)))
                .child(note.text.clone())
                .into_any_element(),
            None => div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child(keys::existing_hint(current))
                .into_any_element(),
        };

        // Navigation is its own row, above the field: it moves between the two
        // sources and never saves, so nobody has to walk OpenAQ -> OWM to reach
        // the key they came to change.
        let nav = div()
            .flex()
            .items_center()
            .gap_1()
            .child(nav_button(
                "\u{2039} Prev",
                can_prev,
                action == Some(0),
                cx.listener(|this, _ev, _window, cx| this.nav_prompt(false, cx)),
            ))
            .child(nav_button(
                "Next \u{203a}",
                can_next,
                action == Some(1),
                cx.listener(|this, _ev, _window, cx| this.nav_prompt(true, cx)),
            ));

        let mut buttons = div().flex().items_center().gap_1();
        if has_key {
            buttons = buttons.child(prompt_button(
                "Remove",
                false,
                action == Some(3),
                cx.listener(|this, _ev, _window, cx| this.remove_prompt_key(cx)),
            ));
        }
        buttons = buttons
            .child(prompt_button(
                "Cancel",
                false,
                action == Some(4),
                cx.listener(|this, _ev, window, cx| this.close_keys(true, window, cx)),
            ))
            .child(prompt_button(
                "Save",
                true,
                action == Some(5),
                cx.listener(|this, _ev, _window, cx| this.save_prompt(cx)),
            ));

        deferred(
            div()
                .absolute()
                .inset_0()
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .occlude()
                        // Dim the widget so the dialog reads as a modal, not a
                        // panel laid over the chart.
                        .bg(rgba(0x000000b0))
                        .id("keys-backdrop")
                        .on_click(cx.listener(|this, _ev, window, cx| {
                            this.close_keys(true, window, cx)
                        })),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .id("keys-dialog")
                                .role(Role::Dialog)
                                .aria_label("API keys")
                                .flex()
                                .flex_col()
                                .gap_1()
                                .w(px(404.0))
                                .p_4()
                                .bg(rgb(PANEL))
                                .border_1()
                                .border_color(rgb(AXIS))
                                .rounded_md()
                                .occlude()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(rgb(MUTED))
                                                .child(prompt.label()),
                                        )
                                        .child(nav),
                                )
                                .child(
                                    div()
                                        .text_lg()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(step.title()),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(rgb(MUTED))
                                        .child(step.what()),
                                )
                                .child(
                                    div()
                                        .id("keys-link")
                                        .role(Role::Link)
                                        .aria_label(format!(
                                            "Open {} in the default browser",
                                            step.url()
                                        ))
                                        .self_start()
                                        .px_1()
                                        .py(px(1.0))
                                        .rounded_sm()
                                        .text_sm()
                                        .text_color(rgb(FOCUS))
                                        .underline()
                                        .cursor_pointer()
                                        .when(action == Some(2), |d| {
                                            d.bg(rgb(PANEL_ACTIVE)).text_color(rgb(TEXT))
                                        })
                                        .hover(|s| s.text_color(rgb(TEXT)))
                                        .on_click(move |_ev, _window, _cx| {
                                            // Never call the shell on the UI
                                            // thread: `ShellExecuteW` pumps
                                            // messages, and re-entering gpui
                                            // while it is dispatching a click
                                            // panics ("RefCell already
                                            // borrowed"). One short thread.
                                            let url = step.url().to_string();
                                            std::thread::spawn(move || {
                                                browser::open(&url);
                                            });
                                        })
                                        .child(step.where_to_get()),
                                )
                                .child(field)
                                .child(hint)
                                .children(env_set.then(|| {
                                    div()
                                        .text_xs()
                                        .text_color(rgb(VENT))
                                        .child(format!(
                                            "{} is set \u{2014} it wins over this file",
                                            step.env_var()
                                        ))
                                }))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .mt_2()
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(rgb(MUTED))
                                                .child("Enter saves \u{b7} Esc closes"),
                                        )
                                        .child(buttons),
                                ),
                        ),
                ),
        )
        .into_any_element()
    }

    /// Footer: verdict legend on the left, source credit on the right (EXP25).
    ///
    /// The credit used to sit in the header, sharing the location trigger's
    /// hover; a source name the user cannot click does not belong in the row of
    /// things they can. As a footer caption it stays visible for the CC BY
    /// attribution and carries the explanation in a tooltip.
    fn footer(&self) -> impl IntoElement {
        let show_noise = self.series.is_some() && self.noise.is_some();
        let show_night = self.night;

        let mut legend = div()
            .flex()
            .items_center()
            .gap_2()
            .child(legend_item(OPEN, "open \u{2264}15"))
            .child(legend_item(VENT, "vent \u{2264}25"))
            .child(legend_item(CLOSED, "closed >25"));
        if show_noise {
            legend = legend.child(legend_item(NOISE, "noise est."));
        }
        if show_night {
            legend = legend.child(legend_item_outlined(NIGHT, "night"));
        }

        let credit = self.source_label();
        let tip = self.credit_tip();

        div()
            .flex()
            .justify_between()
            .items_center()
            .px_4()
            .py_2()
            .text_xs()
            .text_color(rgb(MUTED))
            .child(legend)
            .child(
                div()
                    .id("source-credit")
                    .text_color(rgb(MUTED))
                    .tooltip(move |_, cx| {
                        let text = tip.clone();
                        cx.new(|_| Tip { text }).into()
                    })
                    .child(format!("data \u{a9} {credit}")),
            )
    }

    /// The footer credit's tooltip: what Open-Meteo is, and which sources fed
    /// the fused stack (EXP25). The visible caption names the stack; this says
    /// which kind of evidence each one is.
    fn credit_tip(&self) -> String {
        match self.stack.as_ref().filter(|stack| stack.multi()) {
            Some(stack) => {
                let list = stack
                    .fusion
                    .sources
                    .iter()
                    .map(|source| match source.kind {
                        Kind::Ground => format!("{} (ground station)", source.label),
                        Kind::Model => format!("{} (model)", source.label),
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "Air-quality data from {list}. Each provider's licence is listed in NOTICE."
                )
            }
            None => "Air-quality data from Open-Meteo, a free keyless weather and \
                     air-quality API (CAMS model forecast). Open-Meteo data \
                     \u{a9} Open-Meteo, CC BY 4.0; see NOTICE."
                .to_string(),
        }
    }
}

/// A one-paragraph tooltip view (EXP25). The widget's only hover text before it
/// was the chart's hand-positioned tooltip; this one is placed by gpui itself,
/// so the header chip and the footer credit need no bounds bookkeeping.
struct Tip {
    text: String,
}

impl Render for Tip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .max_w(px(340.0))
            .px_2()
            .py_1()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(AXIS))
            .rounded_md()
            .text_xs()
            .text_color(rgb(TEXT))
            .child(self.text.clone())
    }
}

/// A non-interactive message row inside the panel.
fn hint_row(text: &str) -> impl IntoElement {
    div()
        .h(px(ROW_H))
        .px_3()
        .flex()
        .items_center()
        .text_sm()
        .text_color(rgb(MUTED))
        .child(text.to_string())
}

/// The colour a prompt note is drawn in.
fn tone_color(tone: keys::Tone) -> u32 {
    match tone {
        keys::Tone::Muted => MUTED,
        keys::Tone::Good => OPEN,
        keys::Tone::Warn => VENT,
    }
}

/// A Prev/Next button: same shape as the action buttons but greyed and inert at
/// the ends of the list, so the row reads as navigation rather than an action.
fn nav_button(
    label: &'static str,
    enabled: bool,
    selected: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let text = if !enabled {
        AXIS
    } else if selected {
        TEXT
    } else {
        MUTED
    };
    div()
        .id(label)
        .role(Role::Button)
        .aria_label(label)
        .px_2()
        .py_1()
        .text_xs()
        .rounded_sm()
        .border_1()
        .border_color(rgb(if selected && enabled { FOCUS } else { AXIS }))
        .text_color(rgb(text))
        .when(enabled, |d| {
            d.cursor_pointer()
                .hover(|s| s.bg(rgb(PANEL_HOVER)).text_color(rgb(TEXT)))
                .on_click(on_click)
        })
        .child(label)
}

/// A button in the key prompt (EXP24): clickable, announced as a button, and
/// highlighted while the Tab cycle is on it. `primary` marks the default action
/// (Save) even when it does not hold keyboard focus.
fn prompt_button(
    label: &'static str,
    primary: bool,
    selected: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let border = if primary || selected { FOCUS } else { AXIS };
    div()
        .id(label)
        .role(Role::Button)
        .aria_label(label)
        .px_2()
        .py_1()
        .text_xs()
        .rounded_sm()
        .border_1()
        .border_color(rgb(border))
        .text_color(rgb(if primary || selected { TEXT } else { MUTED }))
        .cursor_pointer()
        .when(selected, |d| d.bg(rgb(PANEL_ACTIVE)))
        .hover(|s| s.bg(rgb(PANEL_HOVER)).text_color(rgb(TEXT)))
        .on_click(on_click)
        .child(label)
}

fn legend_item(color: u32, label: &'static str) -> impl IntoElement {
    legend_row(div().w(px(8.0)).h(px(8.0)).bg(rgb(color)), label)
}

/// A legend swatch for a fill that is *darker than the background* - the night
/// wave (EXP21). Without an outline a near-black square on the dark footer reads
/// as nothing at all, so it borrows the axis hairline. The border sits inside the
/// 8x8 box, so the row's width does not change.
fn legend_item_outlined(color: u32, label: &'static str) -> impl IntoElement {
    legend_row(
        div()
            .w(px(8.0))
            .h(px(8.0))
            .bg(rgb(color))
            .border_1()
            .border_color(rgb(AXIS)),
        label,
    )
}

fn legend_row(square: impl IntoElement, label: &'static str) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(square)
        .child(label)
}

/// Push the current verdict to the Windows tray icon (no-op elsewhere). Kept
/// behind a helper so the Windows-only call is invisible to the view code.
///
/// `hour` is the unix-hour bucket of the reading the verdict came from (EXP27):
/// the notification gate must see a repeated hour as the same red segment, not
/// as a new one.
#[cfg(windows)]
fn tray_set_verdict(verdict: Verdict, location: &str, pm2_5: f32, hour: i64) {
    crate::tray::set_verdict(verdict, location, pm2_5, hour);
}

#[cfg(not(windows))]
fn tray_set_verdict(_verdict: Verdict, _location: &str, _pm2_5: f32, _hour: i64) {}

/// The unix-hour bucket of one series point, the unit the notification gate
/// segments red runs by (EXP27).
fn hour_bucket(time: &chrono::DateTime<chrono::FixedOffset>) -> i64 {
    time.timestamp() / 3600
}

/// Show the tray's muted "fetching" state while a new location loads.
#[cfg(windows)]
fn tray_set_loading(location: &str) {
    crate::tray::set_loading(location);
}

#[cfg(not(windows))]
fn tray_set_loading(_location: &str) {}

/// Show the tray's muted "refreshing..." state while a manual refresh runs
/// (EXP22). Keeps the last verdict, so it never re-fires the warning.
#[cfg(windows)]
fn tray_set_refreshing(location: &str) {
    crate::tray::set_refreshing(location);
}

#[cfg(not(windows))]
fn tray_set_refreshing(_location: &str) {}

/// Muted "no data" tray state after a round that brought nothing back (EXP22).
#[cfg(windows)]
fn tray_set_no_data(location: &str) {
    crate::tray::set_no_data(location);
}

#[cfg(not(windows))]
fn tray_set_no_data(_location: &str) {}

fn verdict_color(verdict: Verdict) -> u32 {
    match verdict {
        Verdict::Open => OPEN,
        Verdict::Vent => VENT,
        Verdict::KeepClosed => CLOSED,
    }
}

impl Render for Widget {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Put initial focus on the trigger so key events reach the root handler
        // (with nothing focused the dispatch path is just the window root). The
        // focus ring stays hidden until the first keyboard input, because
        // `focus_visible` only styles keyboard-focused elements.
        if !self.initial_focus_done {
            self.initial_focus_done = true;
            window.focus(&self.trigger_focus, cx);
        }

        // When the panel starts open (env seam), focus the field on first frame.
        if self.dropdown_open && !self.search_focused {
            self.search_focused = true;
            window.focus(&self.search_focus, cx);
            self.start_blink(cx);
        }

        // The key prompt can be opened by the tray or the test seam, with no
        // window in hand, so it focuses its field here, on the next frame.
        if self.prompt.is_some() && !self.prompt_focused {
            self.prompt_focused = true;
            window.focus(&self.prompt_focus, cx);
            if self.blink_task.is_none() {
                self.start_blink(cx);
            }
        }

        // Whether the field actually holds focus right now (drives the caret).
        let search_focused = self.dropdown_open && self.search_focus.is_focused(window);
        let prompt_caret = self.caret_on
            && self.prompt_action.is_none()
            && self.prompt_focus.is_focused(window);

        let mut root = div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            // Global key handling: Tab focus cycling, Esc, Ctrl+K.
            .on_key_down(cx.listener(|this, ev, window, cx| this.on_root_key(ev, window, cx)))
            .child(self.header(cx));

        match (&self.series, &self.decision) {
            (Some(series), Some(decision)) => {
                // Inferred noise aligned to each point's local hour (EXP16).
                let noise_values = self.noise.map(|est| align_to_series(series, &est));
                let noise_slice = noise_values.as_deref();
                // Day/night wave (EXP21): one night weight per point, from that
                // point's own local hour.
                let night_values = self.night.then(|| night_curve(series));
                let night_slice = night_values.as_deref();
                // The stack is only relevant when it is a real stack (EXP19).
                let stack = self.stack.as_ref().filter(|s| s.multi());
                root = root.child(self.banner(series, decision, cx));
                root = root.child(render_canvas(
                    series,
                    decision,
                    self.thresholds,
                    &self.selected.label,
                    self.hover,
                    self.chart_bounds.clone(),
                    noise_slice,
                    night_slice,
                    stack.map(|s| s.fusion.spread.as_slice()),
                    self.clock24,
                    cx,
                ));
                if let Some(hover) = self.hover.filter(|&i| i < series.len()) {
                    root = root.child(render_tooltip(
                        series,
                        decision,
                        hover,
                        self.chart_bounds.get(),
                        noise_slice,
                        stack,
                        self.clock24,
                    ));
                }
            }
            _ => {
                root = root.child(render_message(
                    self.error.as_deref(),
                    &self.selected.label,
                ));
            }
        }

        root = root.child(self.footer());

        if self.dropdown_open {
            root = root.child(self.dropdown(search_focused, cx));
        }

        if self.prompt.is_some() {
            root = root.child(self.keys_prompt(prompt_caret, cx));
        }

        // Test seam: dump the accessibility tree once the platform has activated
        // accessibility (the E2E posts WM_GETOBJECT). No-op without the env var.
        if !self.a11y_dumped {
            if let Some(path) = a11y_dump_path() {
                if let Some(json) = window.debug_a11y_tree_json() {
                    let _ = std::fs::write(path, json);
                    self.a11y_dumped = true;
                }
            }
        }

        root
    }
}

/// The day's shape (EXP20), one line: how much of the next 24 h is openable, and
/// when the next clean hour starts. The words match the headline and the legend
/// (open / vent / closed) rather than the old `clean / mediocre / closed` (EXP25).
///
/// Deliberately MUTED and never in the verdict colours: the banner sits in the
/// top third of the window, where the E2E asserts that a `KEEP CLOSED` verdict
/// paints **no** green at all. Colouring the counts would break that guarantee
/// for no real information - the numbers are in the words.
fn day_shape_line(series: &Series, decision: &Decision, use24: bool) -> String {
    let shape = decision.shape;
    let mut line = format!(
        "next {} h: {} open \u{b7} {} vent \u{b7} {} closed",
        shape.hours_covered, shape.clean, shape.mediocre, shape.closed
    );
    // "clean from 6 PM" only says something when that hour is still ahead.
    if let Some(first) = shape.first_clean.filter(|&i| i > series.now_index) {
        line.push_str(&format!(
            " \u{b7} clean from {}",
            clock(&series.points[first].time, use24)
        ));
    }
    line
}

/// The same census as a sentence, for the chart's text alternative (WCAG 1.1.1).
/// Spoken form keeps the explanation "vent" only implies, and uses the chosen
/// clock (EXP25).
fn day_shape_sentence(series: &Series, decision: &Decision, use24: bool) -> String {
    let shape = decision.shape;
    let mut sentence = format!(
        "In the next {} hours: {} open, {} when short ventilation is fine, and {} to keep the window closed.",
        shape.hours_covered, shape.clean, shape.mediocre, shape.closed
    );
    if let Some(first) = shape.first_clean.filter(|&i| i > series.now_index) {
        sentence.push_str(&format!(
            " The first clean hour is at {}.",
            clock(&series.points[first].time, use24)
        ));
    }
    sentence
}

/// The sources strip (EXP19): what each source reports for the "now" hour, the
/// fused value, and whether they straddle a verdict threshold. Ground sources
/// are marked, because a model forecast and a physical station are different
/// evidence.
fn sources_strip(stack: &Stack) -> impl IntoElement {
    let readings = stack
        .readings_now()
        .iter()
        .map(|r| {
            let kind = match r.kind {
                Kind::Ground => "ground",
                Kind::Model => "model",
            };
            match r.pm2_5 {
                Some(v) => format!("{} {v:.0} ({kind})", r.label),
                None => format!("{} \u{2014} ({kind})", r.label),
            }
        })
        .collect::<Vec<_>>()
        .join("  \u{b7}  ");

    let spread = stack.now_spread();
    let disagree = stack.now_disagrees();
    let fused = stack.series().now().pm2_5;
    let chip = if disagree {
        format!(
            "sources disagree ({:.0}\u{2013}{:.0})",
            spread.min, spread.max
        )
    } else {
        "sources agree".to_string()
    };

    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child(format!("stack: {readings}")),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_xs()
                .child(
                    div()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(if disagree { DISAGREE } else { MUTED }))
                        .child(chip),
                )
                .child(
                    div()
                        .text_color(rgb(MUTED))
                        .child(format!("fused {fused:.0} \u{b5}g/m\u{b3}")),
                ),
        )
}

/// The timeline canvas (delegates to `draw_timeline`). The chart is invisible to
/// assistive technology, so the wrapper carries a text alternative (WCAG 1.1.1).
fn render_canvas(
    series: &Series,
    decision: &Decision,
    thresholds: Thresholds,
    location: &str,
    hover: Option<usize>,
    chart_bounds: Rc<Cell<Bounds<Pixels>>>,
    noise: Option<&[f32]>,
    night: Option<&[f32]>,
    spread: Option<&[Spread]>,
    use24: bool,
    cx: &mut Context<Widget>,
) -> impl IntoElement {
    let values: Vec<f32> = series.points.iter().map(|p| p.pm2_5).collect();
    let verdicts = decision.hourly.clone();
    let now_index = series.now_index;

    let now = series.now().pm2_5;
    let label = match decision.best_window {
        Some(w) => format!(
            "Air quality timeline for {location}. Now PM2.5 {now:.0} micrograms per cubic \
             metre. Best window {} to {} ({} hours).",
            clock(&series.points[w.start].time, use24),
            clock(&series.points[w.end].time, use24),
            w.hours()
        ),
        None => format!(
            "Air quality timeline for {location}. Now PM2.5 {now:.0} micrograms per cubic \
             metre. No clean window in the next 24 hours."
        ),
    };

    // The curve is an estimate; say so in the text alternative too (EXP16).
    let label = match noise {
        Some(n) if !n.is_empty() => {
            let lo = n.iter().copied().fold(f32::MAX, f32::min);
            let hi = n.iter().copied().fold(f32::MIN, f32::max);
            format!("{label} Estimated noise {lo:.0} to {hi:.0} decibels.")
        }
        _ => label,
    };

    // The day's shape, in words (EXP20).
    let label = format!("{label} {}", day_shape_sentence(series, decision, use24));

    // ...and what the shading behind the curve means (EXP21).
    let label = match night {
        Some(_) => format!(
            "{label} Night is shaded, from {DAY_END_HOUR}:00 to {DAY_START_HOUR}:00."
        ),
        None => label,
    };

    // The canvas paint closure must be `'static`; own the noise slice for it.
    let noise_owned = noise.map(|n| n.to_vec());
    // Same for the night weights (EXP21) and the spread band (EXP19).
    let night_owned = night.map(|n| n.to_vec());
    let spread_owned = spread.map(|s| s.to_vec());

    div()
        .id("timeline")
        .role(Role::Image)
        .aria_label(label)
        .flex()
        .flex_1()
        .px_4()
        .cursor_crosshair()
        // Hover is mouse-only (EXP13): map the pointer's x to the nearest hour.
        // The chart keeps its `Role::Image` text alternative for AT.
        .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _window, cx| {
            this.set_hover_from_pointer(&ev.position, cx);
        }))
        .on_hover(cx.listener(|this, hovered: &bool, _window, cx| {
            if !*hovered {
                this.clear_hover(cx);
            }
        }))
        .child(
            canvas(
                move |_bounds, _window, _cx| {},
                move |bounds, _state, window, _cx| {
                    // Remember the canvas bounds so the next mouse move can hit-test.
                    chart_bounds.set(bounds);
                    draw_timeline(
                        window,
                        bounds,
                        &values,
                        &verdicts,
                        now_index,
                        hover,
                        thresholds,
                        night_owned.as_deref(),
                        noise_owned.as_deref(),
                        spread_owned.as_deref(),
                    );
                },
            )
            .size_full(),
        )
}

/// The hover tooltip: the hour under the pointer, its PM2.5 and verdict. An
/// absolute child of the root, so `bounds` is in window space (origin 0,0).
fn render_tooltip(
    series: &Series,
    decision: &Decision,
    hover: usize,
    bounds: Bounds<Pixels>,
    noise: Option<&[f32]>,
    stack: Option<&Stack>,
    use24: bool,
) -> AnyElement {
    let point = &series.points[hover];
    let verdict = decision
        .hourly
        .get(hover)
        .copied()
        .unwrap_or(Verdict::KeepClosed);
    // Only the x position matters here; y_max is unused by `x_at`.
    let geom = ChartGeom::new(bounds, series.len(), 0.0);
    let x = geom.x_at(hover);
    let min_left = geom.left.max(0.0);
    let max_left = (geom.right - TOOLTIP_W).max(min_left);
    let left = (x - TOOLTIP_W / 2.0).clamp(min_left, max_left);
    let top = (f32::from(bounds.origin.y) + 10.0).max(0.0);

    let when = format!(
        "{} {}",
        point.time.format("%a"),
        clock(&point.time, use24)
    );
    let phase = if hover >= series.now_index {
        "forecast"
    } else {
        "observed"
    };

    div()
        .id("timeline-tooltip")
        .absolute()
        .left(px(left))
        .top(px(top))
        .w(px(TOOLTIP_W))
        .bg(rgb(PANEL))
        .border_1()
        .border_color(rgb(AXIS))
        .rounded_md()
        .px_2()
        .py_1()
        .child(
            div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child(format!("{when}  \u{b7}  {phase}")),
        )
        .child(
            div()
                .text_sm()
                .text_color(rgb(TEXT))
                .child(format!("PM2.5 {:.0} \u{b5}g/m\u{b3}", point.pm2_5)),
        )
        .when_some(noise.filter(|n| hover < n.len()), |d, n| {
            d.child(
                div()
                    .text_xs()
                    .text_color(rgb(NOISE))
                    .child(format!("Noise ~{:.0} dB(A) est.", n[hover])),
            )
        })
        .when_some(stack, |d, stack| {
            // Per-source values for this hour, so the tooltip is where the
            // "stack" is actually inspectable (EXP19).
            stack.fusion.sources.iter().fold(d, |d, source| {
                let value = source
                    .value_at(point.time)
                    .map(|v| format!("{v:.0}"))
                    .unwrap_or_else(|| "\u{2014}".to_string());
                d.child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(format!("{}: {value}", source.label)),
                )
            })
        })
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(verdict_color(verdict)))
                .child(verdict.label()),
        )
        .into_any_element()
}

/// The loading (no error yet) or failure (error and no data) screen.
fn render_message(error: Option<&str>, location: &str) -> impl IntoElement {
    let (color, title, detail) = match error {
        Some(msg) => (CLOSED, "COULDN'T FETCH", msg.to_string()),
        None => (
            MUTED,
            "FETCHING...",
            format!("Contacting Open-Meteo for current {location} air quality."),
        ),
    };

    div()
        .flex()
        .flex_1()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .px_4()
        .child(
            div()
                .text_xl()
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(color))
                .child(title),
        )
        .child(div().text_sm().text_color(rgb(MUTED)).child(detail))
        .child(
            div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child("Retrying automatically every 30 minutes."),
        )
}

/// Shared timeline geometry, so the drawing and the pointer hit-test agree on
/// where each hour sits. All coordinates are in window space.
#[derive(Clone, Copy)]
struct ChartGeom {
    left: f32,
    right: f32,
    top: f32,
    chart_bottom: f32,
    ribbon_h: f32,
    pw: f32,
    ph: f32,
    y_max: f32,
    n: usize,
}

impl ChartGeom {
    fn new(bounds: Bounds<Pixels>, n: usize, y_max: f32) -> Self {
        let ox = f32::from(bounds.origin.x);
        let oy = f32::from(bounds.origin.y);
        let w = f32::from(bounds.size.width);
        let h = f32::from(bounds.size.height);
        let left = ox + CHART_INSET_X;
        let right = ox + w - CHART_INSET_X;
        let top = oy + CHART_INSET_TOP;
        let chart_bottom = oy + h - RIBBON_H - CHART_INSET_BOTTOM;
        Self {
            left,
            right,
            top,
            chart_bottom,
            ribbon_h: RIBBON_H,
            pw: (right - left).max(1.0),
            ph: (chart_bottom - top).max(1.0),
            y_max,
            n,
        }
    }

    fn x_at(&self, i: usize) -> f32 {
        self.left + (i as f32 / (self.n.saturating_sub(1).max(1)) as f32) * self.pw
    }

    fn y_at(&self, v: f32) -> f32 {
        self.chart_bottom - (v / self.y_max) * self.ph
    }

    /// Nearest hour index for a window-space `x`, or `None` when the pointer is
    /// outside the plotted span. Pure, so the mapping is unit-tested.
    fn index_at_x(&self, x: f32) -> Option<usize> {
        if self.n < 2 || x < self.left || x > self.right {
            return None;
        }
        let t = (x - self.left) / self.pw;
        let i = (t * (self.n - 1) as f32).round() as isize;
        Some(i.clamp(0, self.n as isize - 1) as usize)
    }
}

fn draw_timeline(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    values: &[f32],
    verdicts: &[Verdict],
    now_index: usize,
    hover: Option<usize>,
    thresholds: Thresholds,
    night: Option<&[f32]>,
    noise: Option<&[f32]>,
    spread: Option<&[Spread]>,
) {
    let n = values.len();
    if n < 2 || values.len() != verdicts.len() {
        return;
    }

    let y_max = values.iter().copied().fold(thresholds.vent_max, f32::max) * 1.15;
    let g = ChartGeom::new(bounds, n, y_max);
    let ChartGeom {
        left,
        right,
        top,
        chart_bottom,
        ribbon_h,
        pw,
        ph,
        ..
    } = g;

    let x_at = |i: usize| g.x_at(i);
    let y_at = |v: f32| g.y_at(v);

    // Day/night wave (EXP21): the night hours shaded from the axis up, with a
    // smooth cosine boundary - flat at local noon, peaking at local midnight.
    // Painted first, so it is the lowest layer: the grid, the spread band, the
    // noise curve and the PM2.5 curve all read on top of it. Purely visual - no
    // number, verdict or geometry depends on it.
    if let Some(night) = night.filter(|w| w.len() == n) {
        let amplitude = ph * WAVE_AMPLITUDE;
        let mut path = PathBuilder::fill();
        path.move_to(point(px(left), px(chart_bottom)));
        for (i, &weight) in night.iter().enumerate() {
            let y = chart_bottom - weight.clamp(0.0, 1.0) * amplitude;
            path.line_to(point(px(x_at(i)), px(y)));
        }
        path.line_to(point(px(right), px(chart_bottom)));
        path.close();
        if let Ok(path) = path.build() {
            window.paint_path(path, rgb(NIGHT));
        }
    }

    // Horizontal grid.
    for k in 0..=4 {
        let gy = top + (k as f32 / 4.0) * ph;
        window.paint_quad(fill(
            Bounds {
                origin: point(px(left), px(gy)),
                size: size(px(pw), px(1.0)),
            },
            rgb(GRID),
        ));
    }

    // Min–max spread of the stack (EXP19): one column per hour, from the lowest
    // to the highest source reading, so the fused curve reads as the middle of a
    // band. Drawn only where at least two sources covered the hour, and behind
    // everything else so the measured curve stays on top.
    if let Some(spread) = spread.filter(|s| s.len() == n) {
        let step = pw / (n - 1) as f32;
        for (i, h) in spread.iter().enumerate() {
            if h.count < 2 {
                continue;
            }
            let x0 = (x_at(i) - step * 0.5).max(left);
            let x1 = (x_at(i) + step * 0.5).min(right);
            let y_hi = y_at(h.max);
            let y_lo = y_at(h.min);
            window.paint_quad(fill(
                Bounds {
                    origin: point(px(x0), px(y_hi)),
                    size: size(px((x1 - x0).max(1.0)), px((y_lo - y_hi).max(1.0))),
                },
                rgb(BAND),
            ));
        }
    }

    // Verdict ribbon: one cell per hour, aligned under the curve.
    let step = pw / (n - 1) as f32;
    for (i, verdict) in verdicts.iter().enumerate() {
        let x0 = (x_at(i) - step * 0.5).max(left);
        let x1 = (x_at(i) + step * 0.5).min(right);
        window.paint_quad(fill(
            Bounds {
                origin: point(px(x0), px(chart_bottom + 6.0)),
                size: size(px((x1 - x0).max(1.0)), px(ribbon_h)),
            },
            rgb(verdict_color(*verdict)),
        ));
    }

    // Threshold lines: vent (neutral, but high enough contrast to read) and open
    // (amber). Both carry meaning, so they must clear WCAG 1.4.11.
    for (value, color, thickness) in [
        (thresholds.vent_max, CHART_AXIS, 1.0_f32),
        (thresholds.open_max, VENT, 1.0_f32),
    ] {
        let ty = y_at(value);
        window.paint_quad(fill(
            Bounds {
                origin: point(px(left), px(ty)),
                size: size(px(pw), px(thickness)),
            },
            rgb(color),
        ));
    }

    // Axes.
    window.paint_quad(fill(
        Bounds {
            origin: point(px(left), px(chart_bottom)),
            size: size(px(pw), px(1.0)),
        },
        rgb(CHART_AXIS),
    ));
    window.paint_quad(fill(
        Bounds {
            origin: point(px(left), px(top)),
            size: size(px(1.0), px(ph)),
        },
        rgb(CHART_AXIS),
    ));

    // Inferred noise estimate (EXP16): a subordinate dotted violet curve on a
    // fixed dB(A) scale, so it is comparable across days and cities. Drawn
    // before the PM2.5 curve so the measured air data stays on top.
    if let Some(n) = noise.filter(|n| n.len() == values.len()) {
        let span = (MAX_DB - MIN_DB).max(1.0);
        let ny = |v: f32| chart_bottom - ((v - MIN_DB) / span).clamp(0.0, 1.0) * ph;
        let mut path = PathBuilder::stroke(px(1.5)).dash_array(&[px(2.0), px(3.0)]);
        for (i, &v) in n.iter().enumerate() {
            let p = point(px(x_at(i)), px(ny(v)));
            if i == 0 {
                path.move_to(p);
            } else {
                path.line_to(p);
            }
        }
        if let Ok(path) = path.build() {
            window.paint_path(path, rgb(NOISE));
        }
    }

    // Observed curve, solid.
    let mut past = PathBuilder::stroke(px(2.0));
    for i in 0..=now_index {
        let p = point(px(x_at(i)), px(y_at(values[i])));
        if i == 0 {
            past.move_to(p);
        } else {
            past.line_to(p);
        }
    }
    if let Ok(path) = past.build() {
        window.paint_path(path, rgb(PAST));
    }

    // Forecast curve, dashed.
    let mut future = PathBuilder::stroke(px(2.0)).dash_array(&[px(4.0), px(3.0)]);
    for i in now_index..n {
        let p = point(px(x_at(i)), px(y_at(values[i])));
        if i == now_index {
            future.move_to(p);
        } else {
            future.line_to(p);
        }
    }
    if let Ok(path) = future.build() {
        window.paint_path(path, rgb(FUTURE));
    }

    // "Now" divider, full height including the ribbon.
    let nx = x_at(now_index);
    window.paint_quad(fill(
        Bounds {
            origin: point(px(nx), px(top)),
            size: size(px(1.0), px(ph + ribbon_h + 4.0)),
        },
        rgb(NOWL),
    ));
    // "Now" marker on the curve.
    window.paint_quad(fill(
        Bounds {
            origin: point(px(nx - 2.0), px(y_at(values[now_index]) - 2.0)),
            size: size(px(4.0), px(4.0)),
        },
        rgb(TEXT),
    ));

    // Hover highlight (EXP13): a full-height crosshair at the hour under the
    // pointer, plus a verdict-coloured dot on the curve.
    if let Some(i) = hover.filter(|&i| i < n) {
        let hx = x_at(i);
        window.paint_quad(fill(
            Bounds {
                origin: point(px(hx), px(top)),
                size: size(px(1.0), px(ph + ribbon_h + 4.0)),
            },
            rgb(HOVER),
        ));
        window.paint_quad(fill(
            Bounds {
                origin: point(px(hx - 3.0), px(y_at(values[i]) - 3.0)),
                size: size(px(6.0), px(6.0)),
            },
            rgb(verdict_color(verdicts[i])),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geom(n: usize) -> ChartGeom {
        ChartGeom::new(
            Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(200.0), px(100.0)),
            },
            n,
            30.0,
        )
    }

    #[test]
    fn index_at_x_maps_edges_and_middle() {
        let g = geom(48);
        assert_eq!(g.index_at_x(g.left), Some(0));
        assert_eq!(g.index_at_x(g.right), Some(47));
        // Halfway across 48 points -> round(0.5 * 47) = round(23.5) = 24.
        assert_eq!(g.index_at_x((g.left + g.right) / 2.0), Some(24));
    }

    #[test]
    fn index_at_x_is_none_outside_the_plot() {
        let g = geom(48);
        assert_eq!(g.index_at_x(g.left - 1.0), None);
        assert_eq!(g.index_at_x(g.right + 1.0), None);
    }

    #[test]
    fn index_at_x_rounds_to_the_nearest_hour() {
        let g = geom(48);
        // 40% of the way -> round(0.4 * 47) = round(18.8) = 19.
        assert_eq!(g.index_at_x(g.left + 0.4 * g.pw), Some(19));
    }

    #[test]
    fn index_at_x_needs_at_least_two_points() {
        let g = geom(1);
        assert_eq!(g.index_at_x(g.left), None);
    }

    /// The mock series with its verdicts, for the banner-text tests (EXP20).
    fn mock_decision() -> (Series, Decision) {
        use airqlality_core::{AirQualitySource, MockSource, Thresholds, evaluate};
        let series = MockSource.fetch().unwrap();
        let decision = evaluate(&series, &Thresholds::default());
        (series, decision)
    }

    #[test]
    fn the_day_shape_line_names_the_bands_and_the_first_clean_hour() {
        // Mock "now" is 14:00 (26 ug/m3, closed); the clean run opens at 02:00.
        let (series, decision) = mock_decision();
        assert_eq!(
            day_shape_line(&series, &decision, false),
            "next 24 h: 5 open \u{b7} 17 vent \u{b7} 2 closed \u{b7} clean from 2:00 AM"
        );
    }

    #[test]
    fn the_day_shape_sentence_is_what_a_screen_reader_hears() {
        let (series, decision) = mock_decision();
        assert_eq!(
            day_shape_sentence(&series, &decision, false),
            "In the next 24 hours: 5 open, 17 when short ventilation is fine, and 2 to keep \
             the window closed. The first clean hour is at 2:00 AM."
        );
    }

    #[test]
    fn a_clean_hour_right_now_is_not_announced_as_upcoming() {
        // Every hour clean: "clean from 14:00" would be noise on top of an OPEN
        // headline, so the tail is dropped (EXP20).
        use airqlality_core::{AirQualityPoint, HORIZON_HOURS, Series, Verdict, evaluate_hourly};
        use chrono::{Duration, FixedOffset, TimeZone};

        let tz = FixedOffset::east_opt(2 * 3600).unwrap();
        let start = tz.with_ymd_and_hms(2026, 10, 4, 14, 0, 0).single().unwrap();
        let points = (0..HORIZON_HOURS + 1)
            .map(|i| AirQualityPoint {
                time: start + Duration::hours(i as i64),
                pm2_5: 5.0,
                pm10: 10.0,
                us_aqi: 20.0,
                wind_speed: 5.0,
            })
            .collect();
        let series = Series::new("test", points, 0);
        let decision = evaluate_hourly(vec![Verdict::Open; HORIZON_HOURS + 1], 0);

        let line = day_shape_line(&series, &decision, false);
        assert_eq!(line, "next 24 h: 24 open \u{b7} 0 vent \u{b7} 0 closed");
        assert!(!line.contains("clean from"));
    }

    #[test]
    fn a_day_with_no_clean_hour_says_so_by_counting_zero() {
        use airqlality_core::{AirQualityPoint, HORIZON_HOURS, Series, Verdict, evaluate_hourly};
        use chrono::{Duration, FixedOffset, TimeZone};

        let tz = FixedOffset::east_opt(2 * 3600).unwrap();
        let start = tz.with_ymd_and_hms(2026, 10, 4, 14, 0, 0).single().unwrap();
        let points = (0..HORIZON_HOURS + 1)
            .map(|i| AirQualityPoint {
                time: start + Duration::hours(i as i64),
                pm2_5: 60.0,
                pm10: 90.0,
                us_aqi: 130.0,
                wind_speed: 5.0,
            })
            .collect();
        let series = Series::new("test", points, 0);
        let decision = evaluate_hourly(vec![Verdict::KeepClosed; HORIZON_HOURS + 1], 0);

        let line = day_shape_line(&series, &decision, false);
        assert_eq!(line, "next 24 h: 0 open \u{b7} 0 vent \u{b7} 24 closed");
        assert!(!line.contains("clean from"));
    }

    #[test]
    fn clock_is_twelve_hour_by_default_and_switches_to_24() {
        use chrono::{FixedOffset, TimeZone};

        let tz = FixedOffset::east_opt(2 * 3600).unwrap();
        let at = |hour, minute| {
            tz.with_ymd_and_hms(2026, 10, 4, hour, minute, 0)
                .single()
                .unwrap()
        };
        // 12-hour is the default; no leading zero, and the two edges are 12.
        assert_eq!(clock(&at(18, 0), false), "6:00 PM");
        assert_eq!(clock(&at(9, 5), false), "9:05 AM");
        assert_eq!(clock(&at(0, 5), false), "12:05 AM");
        assert_eq!(clock(&at(12, 0), false), "12:00 PM");
        // The same instants after the click.
        assert_eq!(clock(&at(18, 0), true), "18:00");
        assert_eq!(clock(&at(9, 5), true), "09:05");
        assert_eq!(clock(&at(0, 5), true), "00:05");
    }

    #[test]
    fn issue_label_agrees_with_the_count() {
        assert_eq!(issue_label(1), "1 source issue");
        assert_eq!(issue_label(3), "3 source issues");
    }
}
