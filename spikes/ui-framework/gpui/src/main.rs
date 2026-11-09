//! gpui (gpui-pre 0.3.12, a crates.io snapshot of Zed's gpui) on Wayland layer-shell: a transparent layer surface,
//! gpui's own text for the bubble, the sprite as one quad per painted cell, and Window::set_input_region for the
//! clickable area.
//!
//! gpui can't change a layer surface's margins after it is made, so the pet can't move the surface. Instead the
//! surface is a fixed 700 x 500 "playground" and a drag moves the pet inside it (the input region follows it).
//!
//! SPIKE_SECONDS=<n> quits after n seconds; SPIKE_DEBUG=1 logs presses and region updates; SPIKE_STATE picks the
//! pet's mood (sleep by default).

use std::time::{Duration, Instant};

use gpui::{
    App, Bounds, Context, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Render, Size, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, canvas, div, fill, point, prelude::*, px,
    rgb, rgba,
};
use spike_common::{BUBBLE, BUBBLE_TEXT, BUBBLE_TEXT_SIZE, H, Scene, W};

const PLAY: (f32, f32) = (700.0, 500.0);

struct Pet {
    scene: Scene,
    /// where the scene's top-left sits in the surface
    at: (f32, f32),
    press: Option<(f32, f32)>,
    dragging: bool,
    region: Vec<(i32, i32, i32, i32)>,
    region_at: Instant,
    debug: bool,
}

impl Pet {
    fn new(cx: &mut Context<Self>) -> Self {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(33)).await;
                let alive = this.update(cx, |pet, cx| {
                    pet.scene.tick();
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
        if let Some(secs) = std::env::var("SPIKE_SECONDS").ok().and_then(|s| s.parse::<u64>().ok()) {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_secs(secs)).await;
                let _ = this.update(cx, |pet, cx| {
                    eprintln!("gpui: {} ticks", pet.scene.frames);
                    cx.quit();
                });
            })
            .detach();
        }
        Pet {
            scene: Scene::new(),
            at: (220.0, PLAY.1 - H as f32 - 20.0),
            press: None,
            dragging: false,
            region: Vec::new(),
            region_at: Instant::now() - Duration::from_secs(1),
            debug: std::env::var_os("SPIKE_DEBUG").is_some(),
        }
    }

    fn local(&self, p: gpui::Point<Pixels>) -> (f64, f64) {
        (f64::from(f32::from(p.x) - self.at.0), f64::from(f32::from(p.y) - self.at.1))
    }

    fn update_region(&mut self, window: &mut Window) {
        if self.dragging {
            // while dragging the whole surface takes the pointer, so a fast drag doesn't fall off the art
            if !self.region.is_empty() {
                window.set_input_region(None);
                self.region.clear();
            }
            return;
        }
        let (ox, oy) = (self.at.0 as i32, self.at.1 as i32);
        let rects: Vec<_> = self.scene.input_rects().into_iter().map(|(x, y, w, h)| (x + ox, y + oy, w, h)).collect();
        if rects == self.region || self.region_at.elapsed() < Duration::from_millis(100) {
            return;
        }
        let bounds: Vec<Bounds<Pixels>> = rects
            .iter()
            .map(|&(x, y, w, h)| Bounds::new(point(px(x as f32), px(y as f32)), Size::new(px(w as f32), px(h as f32))))
            .collect();
        window.set_input_region(Some(&bounds));
        if self.debug {
            eprintln!("region: {} rects", rects.len());
        }
        self.region = rects;
        self.region_at = Instant::now();
    }
}

impl Render for Pet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.update_region(window);
        let (bx, by, bw, bh) = BUBBLE;
        let body = self.scene.body_rgba().to_vec();
        let transform = self.scene.sprite_transform();
        let shadow = self.scene.shadow();
        div()
            .size_full()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|pet, e: &MouseDownEvent, _, _| {
                    let (x, y) = pet.local(e.position);
                    if pet.scene.hit(x, y) {
                        pet.press = Some((x as f32, y as f32));
                        if pet.debug {
                            eprintln!("press at {x:.0},{y:.0}");
                        }
                    }
                }),
            )
            .on_mouse_move(cx.listener(|pet, e: &MouseMoveEvent, _, _| {
                let (x, y) = pet.local(e.position);
                pet.scene.set_pointer(Some((x, y)));
                if let Some(grab) = pet.press {
                    let (dx, dy) = (x as f32 - grab.0, y as f32 - grab.1);
                    if !pet.dragging && dx.hypot(dy) > 3.0 {
                        pet.dragging = true;
                        pet.scene.set_dragging(true);
                    }
                    if pet.dragging {
                        pet.at.0 = (pet.at.0 + dx).clamp(0.0, PLAY.0 - W as f32);
                        pet.at.1 = (pet.at.1 + dy).clamp(0.0, PLAY.1 - H as f32);
                    }
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|pet, _: &MouseUpEvent, _, _| {
                    if pet.dragging {
                        pet.scene.set_dragging(false);
                    } else if pet.press.is_some() {
                        pet.scene.input.poke_until = pet.scene.now() + 0.6;
                    }
                    pet.dragging = false;
                    pet.press = None;
                }),
            )
            .child(
                div()
                    .absolute()
                    .left(px(self.at.0))
                    .top(px(self.at.1))
                    .w(px(W as f32))
                    .h(px(H as f32))
                    .child(
                        canvas(
                            |_, _, _| {},
                            move |bounds, _, window, _| {
                                let o = bounds.origin;
                                let (cx_, cy_, rx, ry, a) = shadow;
                                window.paint_quad(
                                    fill(
                                        Bounds::new(
                                            point(o.x + px(cx_ - rx), o.y + px(cy_ - ry)),
                                            Size::new(px(rx * 2.0), px(ry * 2.0)),
                                        ),
                                        rgba((((a * 255.0) as u32).min(255)) & 0xFF),
                                    )
                                    .corner_radii(px(ry)),
                                );
                                // one quad per painted cell, after squash and stretch: crisp at any scale
                                for cy in 0..24usize {
                                    for cx in 0..26usize {
                                        let i = (cy * 26 + cx) * 4;
                                        let a = body[i + 3];
                                        if a == 0 {
                                            continue;
                                        }
                                        let un = |c: u8| (u32::from(c) * 255 / u32::from(a)).min(255);
                                        let colour = (un(body[i]) << 24) | (un(body[i + 1]) << 16) | (un(body[i + 2]) << 8) | u32::from(a);
                                        let mut p = [
                                            tiny_skia_point(cx as f32, cy as f32),
                                            tiny_skia_point(cx as f32 + 1.0, cy as f32 + 1.0),
                                        ];
                                        transform.map_points(&mut p);
                                        window.paint_quad(fill(
                                            Bounds::from_corners(
                                                point(o.x + px(p[0].x), o.y + px(p[0].y)),
                                                point(o.x + px(p[1].x), o.y + px(p[1].y)),
                                            ),
                                            rgba(colour),
                                        ));
                                    }
                                }
                            },
                        )
                        .absolute()
                        .size_full(),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(bx))
                            .top(px(by))
                            .w(px(bw))
                            .h(px(bh))
                            .px(px(12.0))
                            .py(px(8.0))
                            .bg(rgba(std::env::var("SPIKE_BG").ok().and_then(|s| u32::from_str_radix(&s, 16).ok()).unwrap_or(0xfffffff5)))
                            .border(px(1.5))
                            .border_color(rgb(0x786ec8))
                            .rounded(px(10.0))
                            .text_size(px(BUBBLE_TEXT_SIZE))
                            .text_color(rgb(0x2a2440))
                            .font_family("Noto Sans")
                            .child(BUBBLE_TEXT),
                    ),
            )
    }
}

fn tiny_skia_point(x: f32, y: f32) -> spike_common::tiny_skia::Point {
    spike_common::tiny_skia::Point::from_xy(x, y)
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        use gpui::layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions};
        cx.open_window(
            WindowOptions {
                titlebar: None,
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: Size::new(px(PLAY.0), px(PLAY.1)),
                })),
                app_id: Some("aipet-spike-gpui".to_string()),
                window_background: WindowBackgroundAppearance::Transparent,
                kind: WindowKind::LayerShell(LayerShellOptions {
                    namespace: "aipet-spike-gpui".to_string(),
                    layer: Layer::Top,
                    anchor: Anchor::TOP | Anchor::LEFT,
                    margin: Some((px(250.), px(0.), px(0.), px(1000.))),
                    keyboard_interactivity: KeyboardInteractivity::None,
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(Pet::new),
        )
        .unwrap();
    });
}
