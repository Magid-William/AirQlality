//! EXP02 spike — a minimal gpui Windows widget that draws the `UI.md` timeline
//! graph with the built-in `canvas` / `PathBuilder` paint API. No data fetching,
//! no extra UI libraries: the point is only to prove gpui can render it on Windows.

use gpui::{
    App, Bounds, Context, PathBuilder, Render, Window, WindowBounds, WindowOptions, canvas, div,
    fill, point, prelude::*, px, rgb, size,
};
use gpui_platform::application;

/// Hourly-ish PM2.5 sample (ug/m3), hardcoded from EXP01's Cairo probe shape.
const DATA: [f32; 24] = [
    22.0, 24.0, 26.0, 25.0, 23.0, 21.0, 19.0, 20.0, 22.0, 19.0, 17.0, 16.0, // past
    18.0, 21.0, 24.0, 27.0, 25.0, 22.0, 20.0, 18.0, 17.0, 19.0, 22.0, 20.0, // forecast
];
const NOW: usize = 12;
const THRESHOLD: f32 = 15.0;
const Y_MAX: f32 = 32.0;

const BG: u32 = 0x0d1117;
const TEXT: u32 = 0xe6edf3;
const MUTED: u32 = 0x8b949e;
const GRID: u32 = 0x21262d;
const AXIS: u32 = 0x30363d;
const PAST: u32 = 0x2f81f7;
const FUTURE: u32 = 0x58a6ff;
const THRESH: u32 = 0xd29922;
const NOWL: u32 = 0x8b949e;

struct Widget;

impl Widget {
    fn new() -> Self {
        Widget
    }

    fn header() -> impl IntoElement {
        div()
            .flex()
            .justify_between()
            .items_center()
            .px_3()
            .py_2()
            .text_sm()
            .child(div().text_color(rgb(TEXT)).child("Window"))
            .child(div().text_color(rgb(MUTED)).child("Cairo, Egypt"))
    }

    fn footer() -> impl IntoElement {
        div()
            .flex()
            .justify_between()
            .items_center()
            .px_3()
            .py_1()
            .text_xs()
            .text_color(rgb(MUTED))
            .child("N  PM2.5 (µg/m³)")
            .child("threshold 15 · solid = past · dashed = forecast · T  time →")
    }
}

impl Render for Widget {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .child(Self::header())
            .child(
                div().flex().flex_1().child(
                    canvas(
                        move |_bounds, _window, _cx| {},
                        move |bounds, _state, window, _cx| {
                            let ox = f32::from(bounds.origin.x);
                            let oy = f32::from(bounds.origin.y);
                            let w = f32::from(bounds.size.width);
                            let h = f32::from(bounds.size.height);

                            let left = ox + 8.0;
                            let right = ox + w - 8.0;
                            let top = oy + 8.0;
                            let bottom = oy + h - 8.0;
                            let pw = (right - left).max(1.0);
                            let ph = (bottom - top).max(1.0);

                            let x_at = |i: usize| left + (i as f32 / (DATA.len() - 1) as f32) * pw;
                            let y_at = |v: f32| bottom - (v / Y_MAX) * ph;

                            // Horizontal grid lines.
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

                            // Axes.
                            window.paint_quad(fill(
                                Bounds {
                                    origin: point(px(left), px(bottom)),
                                    size: size(px(pw), px(1.0)),
                                },
                                rgb(AXIS),
                            ));
                            window.paint_quad(fill(
                                Bounds {
                                    origin: point(px(left), px(top)),
                                    size: size(px(1.0), px(ph)),
                                },
                                rgb(AXIS),
                            ));

                            // Threshold line (PM2.5 = 15 ug/m3).
                            let ty = y_at(THRESHOLD);
                            window.paint_quad(fill(
                                Bounds {
                                    origin: point(px(left), px(ty)),
                                    size: size(px(pw), px(1.5)),
                                },
                                rgb(THRESH),
                            ));

                            // Past curve, solid.
                            let mut past = PathBuilder::stroke(px(2.0));
                            for i in 0..=NOW {
                                let p = point(px(x_at(i)), px(y_at(DATA[i])));
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
                            for i in NOW..DATA.len() {
                                let p = point(px(x_at(i)), px(y_at(DATA[i])));
                                if i == NOW {
                                    future.move_to(p);
                                } else {
                                    future.line_to(p);
                                }
                            }
                            if let Ok(path) = future.build() {
                                window.paint_path(path, rgb(FUTURE));
                            }

                            // "Now" divider.
                            let nx = x_at(NOW);
                            window.paint_quad(fill(
                                Bounds {
                                    origin: point(px(nx), px(top)),
                                    size: size(px(1.0), px(ph)),
                                },
                                rgb(NOWL),
                            ));
                        },
                    )
                    .size_full(),
                ),
            )
            .child(Self::footer())
    }
}

fn main() {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(460.0), px(200.0)), cx);
        cx.open_window(
            WindowOptions {
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("AirQlality — open-window widget".into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_window, cx| cx.new(|_| Widget::new()),
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}
