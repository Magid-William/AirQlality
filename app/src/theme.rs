//! Shared palette. One place to tweak the look; `ui` never hardcodes a colour.

pub const BG: u32 = 0x0d1117;
pub const TEXT: u32 = 0xe6edf3;
pub const MUTED: u32 = 0x8b949e;
pub const GRID: u32 = 0x21262d;
pub const AXIS: u32 = 0x30363d;
/// Axis / threshold lines in the chart, which carry meaning (the vent boundary)
/// and so need WCAG 1.4.11 non-text contrast: 4.1:1 on `BG`. The decorative
/// `GRID` lines stay low-contrast on purpose. EXP10.
pub const CHART_AXIS: u32 = 0x6e7681;
/// Background of the location dropdown panel (distinct from `BG` so it reads as
/// a floating surface).
pub const PANEL: u32 = 0x161b22;

pub const PAST: u32 = 0x2f81f7;
pub const FUTURE: u32 = 0x58a6ff;
pub const NOWL: u32 = 0x8b949e;
/// Pointer-hover crosshair on the timeline (EXP13). Distinct from `NOWL` so the
/// hovered hour reads apart from the "now" divider; 4.6:1 on `BG`.
pub const HOVER: u32 = 0xc9d1d9;

/// Inferred noise curve (EXP16). Violet, deliberately distinct from the
/// air-quality blues; it is an estimate, so it reads as secondary to the PM2.5
/// curve. 7.6:1 on `BG`.
pub const NOISE: u32 = 0xbc8cff;

/// Keyboard focus ring (`:focus-visible`). 7.5:1 on `BG`, 6.9:1 on `PANEL`, so it
/// clears WCAG 2.4.11 non-text contrast. EXP10.
pub const FOCUS: u32 = 0x58a6ff;
/// Pointer-hover surface inside the panel (subtly lighter than `PANEL`).
pub const PANEL_HOVER: u32 = 0x1c2330;
/// Active (arrow-key-highlighted) result row.
pub const PANEL_ACTIVE: u32 = 0x223041;

pub const OPEN: u32 = 0x3fb950;
pub const VENT: u32 = 0xd29922;
pub const CLOSED: u32 = 0xf85149;

/// "Sources disagree" warning (EXP19). Orange, deliberately not one of the three
/// verdict colours, so a disagreement never reads as a verdict of its own;
/// 8.4:1 on `BG`.
pub const DISAGREE: u32 = 0xffa657;

/// The min–max spread envelope behind the fused PM2.5 curve (EXP19). A deep
/// blue that stays clearly subordinate to the measured `PAST`/`FUTURE` curve,
/// so the stack reads as a band around the value, not a second line.
pub const BAND: u32 = 0x1f3a5f;

/// The shaded night region behind the timeline (EXP21). Deliberately **darker
/// than `BG`**: a night band that is *lighter* than the canvas reads as
/// "highlighted" - daytime - which is the opposite of what it means. It is the
/// lowest layer of the chart: the grid, the spread band, the noise curve and the
/// PM2.5 curve are all painted over it, and the footer's `night` key carries an
/// outline so a darker-than-background swatch is still visible.
pub const NIGHT: u32 = 0x05070d;
