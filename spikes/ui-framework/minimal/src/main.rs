//! The minimal stack: no UI framework. A wlr layer surface from smithay-client-toolkit, a CPU buffer in wl_shm
//! (premultiplied ARGB, which is what tiny-skia draws), tiny-skia for the art, cosmic-text for the bubble's text,
//! and a wl_region for the input region. Linux (Wayland with layer-shell) only; see ui-framework.md for X11 and
//! Windows.
//!
//! SPIKE_SECONDS=<n> quits after n seconds; SPIKE_DEBUG=1 logs pointer events and region updates; SPIKE_STATE picks
//! the pet's mood (sleep by default, as a calm pet is).

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("this prototype is Linux-only");
}

#[cfg(target_os = "linux")]
fn main() {
    linux::run();
}

#[cfg(target_os = "linux")]
mod linux {
    use std::time::{Duration, Instant};

    use cosmic_text::{Attrs, Buffer, Color as TColor, Family, FontSystem, Metrics, Shaping, SwashCache, Wrap};
    use smithay_client_toolkit::{
        compositor::{CompositorHandler, CompositorState, FrameCallbackData, Region},
        output::{OutputHandler, OutputState},
        reexports::{
            calloop::{
                EventLoop,
                timer::{TimeoutAction, Timer},
            },
            calloop_wayland_source::WaylandSource,
            client::{
                Connection, QueueHandle,
                globals::registry_queue_init,
                protocol::{wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
            },
        },
        registry::{ProvidesRegistryState, RegistryState},
        registry_handlers,
        seat::{
            Capability, SeatHandler, SeatState,
            pointer::{PointerEvent, PointerEventKind, PointerHandler},
        },
        shell::{
            WaylandSurface,
            wlr_layer::{
                Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
                LayerSurfaceConfigure,
            },
        },
        shm::{Shm, ShmHandler, slot::SlotPool},
    };
    use spike_common::{BUBBLE, BUBBLE_TEXT, BUBBLE_TEXT_SIZE, H, Scene, W};
    use tiny_skia::{Pixmap, PixmapPaint, Transform};

    const BTN_LEFT: u32 = 0x110;

    pub fn run() {
        let debug = std::env::var_os("SPIKE_DEBUG").is_some();
        let quit_after = std::env::var("SPIKE_SECONDS").ok().and_then(|s| s.parse::<u64>().ok());
        let conn = Connection::connect_to_env().expect("no Wayland display");
        let (globals, queue) = registry_queue_init(&conn).unwrap();
        let qh = queue.handle();
        let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor");
        let layer_shell = LayerShell::bind(&globals, &qh).expect("this compositor has no layer-shell");
        let shm = Shm::bind(&globals, &qh).expect("wl_shm");

        let surface = compositor.create_surface(&qh);
        let layer = layer_shell.create_layer_surface(&qh, surface, Layer::Top, Some("aipet-spike-minimal"), None);
        // top-left, away from the real pet in the bottom-right corner
        let margin = (300, 600);
        layer.set_anchor(Anchor::TOP | Anchor::LEFT);
        layer.set_margin(margin.0, 0, 0, margin.1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_size(W, H);
        layer.commit();

        let pool = SlotPool::new((W * H * 4) as usize, &shm).expect("shm pool");
        let text = Text::new();
        let mut app = App {
            registry: RegistryState::new(&globals),
            seat: SeatState::new(&globals, &qh),
            outputs: OutputState::new(&globals, &qh),
            shm,
            compositor,
            pool,
            layer,
            scale: 1,
            configured: false,
            frame_pending: false,
            scene: Scene::new(),
            text,
            pointer: None,
            press: None,
            dragging: false,
            margin,
            margin_pending: false,
            region: Vec::new(),
            region_at: Instant::now() - Duration::from_secs(1),
            exit: false,
            debug,
            draws: 0,
            started: Instant::now(),
        };

        let mut event_loop: EventLoop<App> = EventLoop::try_new().unwrap();
        WaylandSource::new(conn.clone(), queue).insert(event_loop.handle()).unwrap();
        // the pet's calm pacing: one tick and one frame every 33 ms
        event_loop
            .handle()
            .insert_source(Timer::from_duration(Duration::from_millis(33)), move |_, _, app: &mut App| {
                app.tick(&qh);
                TimeoutAction::ToDuration(Duration::from_millis(33))
            })
            .unwrap();
        let end = quit_after.map(|s| Instant::now() + Duration::from_secs(s));
        while !app.exit {
            event_loop.dispatch(Duration::from_millis(100), &mut app).unwrap();
            if end.is_some_and(|e| Instant::now() >= e) {
                break;
            }
        }
        let secs = app.started.elapsed().as_secs_f64();
        eprintln!("minimal: {} draws in {secs:.1} s, {} ticks", app.draws, app.scene.frames);
    }

    /// The bubble's text, shaped and rasterised once (it doesn't change in the prototype).
    struct Text {
        layer: Pixmap,
    }

    impl Text {
        fn new() -> Self {
            let t = Instant::now();
            let mut fonts = FontSystem::new();
            let mut cache = SwashCache::new();
            let (_, _, bw, bh) = BUBBLE;
            let mut buffer = Buffer::new(&mut fonts, Metrics::new(BUBBLE_TEXT_SIZE, BUBBLE_TEXT_SIZE * 1.33));
            buffer.set_wrap(Wrap::Word);
            buffer.set_size(Some(bw - 24.0), Some(bh));
            buffer.set_text(BUBBLE_TEXT, &Attrs::new().family(Family::Name("Noto Sans")), Shaping::Advanced, None);
            let mut layer = Pixmap::new(bw as u32, bh as u32).unwrap();
            let (lw, lh) = (layer.width() as i32, layer.height() as i32);
            let data = layer.data_mut();
            buffer.draw(&mut fonts, &mut cache, TColor::rgb(0x2a, 0x24, 0x40), |x, y, w, h, c| {
                for yy in y..y + h as i32 {
                    for xx in x..x + w as i32 {
                        if !(0..lw).contains(&xx) || !(0..lh).contains(&yy) {
                            continue;
                        }
                        let i = ((yy * lw + xx) * 4) as usize;
                        let a = u32::from(c.a());
                        // premultiplied source-over
                        for (k, s) in [c.r(), c.g(), c.b(), 255].into_iter().enumerate() {
                            let d = u32::from(data[i + k]);
                            data[i + k] = ((u32::from(s) * a + d * (255 - a)) / 255) as u8;
                        }
                    }
                }
            });
            eprintln!("minimal: text shaped and drawn in {:.0} ms", t.elapsed().as_secs_f64() * 1000.0);
            Text { layer }
        }
    }

    struct App {
        registry: RegistryState,
        seat: SeatState,
        outputs: OutputState,
        shm: Shm,
        compositor: CompositorState,
        pool: SlotPool,
        layer: LayerSurface,
        scale: i32,
        configured: bool,
        frame_pending: bool,
        scene: Scene,
        text: Text,
        pointer: Option<wl_pointer::WlPointer>,
        /// Where the left button went down (surface coordinates), while it is down.
        press: Option<(f64, f64)>,
        dragging: bool,
        /// top, left
        margin: (i32, i32),
        /// a margin change not yet shown: pointer positions are relative to the old place until it is
        margin_pending: bool,
        region: Vec<(i32, i32, i32, i32)>,
        region_at: Instant,
        exit: bool,
        debug: bool,
        draws: u64,
        started: Instant,
    }

    impl App {
        fn tick(&mut self, qh: &QueueHandle<Self>) {
            self.scene.tick();
            if !self.configured || self.frame_pending {
                // hidden or the compositor is behind: tick, but don't draw
                return;
            }
            self.update_region();
            self.draw(qh);
        }

        /// The input region: the bubble and the sprite's painted rows. At most every 100 ms, as the iced pet does.
        fn update_region(&mut self) {
            let rects = self.scene.input_rects();
            if rects == self.region || self.region_at.elapsed() < Duration::from_millis(100) {
                return;
            }
            let region = Region::new(&self.compositor).unwrap();
            for &(x, y, w, h) in &rects {
                region.add(x, y, w, h);
            }
            self.layer.wl_surface().set_input_region(Some(region.wl_region()));
            if self.debug {
                eprintln!("region: {} rects", rects.len());
            }
            self.region = rects;
            self.region_at = Instant::now();
        }

        fn draw(&mut self, qh: &QueueHandle<Self>) {
            let s = self.scale;
            let (w, h) = (W as i32 * s, H as i32 * s);
            let (buffer, canvas) =
                self.pool.create_buffer(w, h, w * 4, wl_shm::Format::Argb8888).expect("shm buffer");
            {
                // tiny-skia draws premultiplied RGBA; wl_shm's ARGB8888 is premultiplied BGRA in memory
                let mut pm = tiny_skia::PixmapMut::from_bytes(canvas, w as u32, h as u32).unwrap();
                self.scene.draw(&mut pm, s as f32);
                let (bx, by, _, _) = BUBBLE;
                pm.draw_pixmap(
                    0,
                    0,
                    self.text.layer.as_ref(),
                    &PixmapPaint::default(),
                    Transform::from_scale(s as f32, s as f32).pre_translate(bx + 12.0, by + 8.0),
                    None,
                );
                for px in canvas.chunks_exact_mut(4) {
                    px.swap(0, 2);
                }
            }
            let surface = self.layer.wl_surface();
            surface.set_buffer_scale(s);
            surface.damage_buffer(0, 0, w, h);
            surface.frame(qh, FrameCallbackData(surface.clone()));
            buffer.attach_to(surface).expect("attach");
            self.layer.commit();
            self.frame_pending = true;
            self.draws += 1;
        }

        fn pointer_at(&mut self, pos: (f64, f64)) {
            self.scene.set_pointer(Some(pos));
            let Some(grab) = self.press else { return };
            let (dx, dy) = (pos.0 - grab.0, pos.1 - grab.1);
            if !self.dragging && dx.hypot(dy) > 3.0 {
                self.dragging = true;
                self.scene.set_dragging(true);
                if self.debug {
                    eprintln!("drag: start");
                }
            }
            if self.dragging && !self.margin_pending && (dx.round() != 0.0 || dy.round() != 0.0) {
                // move the surface by the pointer's delta; until the change is shown, positions are stale
                self.margin.0 += dy.round() as i32;
                self.margin.1 += dx.round() as i32;
                self.layer.set_margin(self.margin.0, 0, 0, self.margin.1);
                self.margin_pending = true;
            }
        }
    }

    impl CompositorHandler for App {
        fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, f: i32) {
            self.scale = f.max(1);
        }
        fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
        fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {
            self.frame_pending = false;
            self.margin_pending = false;
        }
        fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
        fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    }

    impl OutputHandler for App {
        fn output_state(&mut self) -> &mut OutputState {
            &mut self.outputs
        }
        fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
        fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
        fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    }

    impl LayerShellHandler for App {
        fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
            self.exit = true;
        }
        fn configure(&mut self, _: &Connection, qh: &QueueHandle<Self>, _: &LayerSurface, c: LayerSurfaceConfigure, _: u32) {
            if self.debug {
                eprintln!("configure: {:?}", c.new_size);
            }
            if !self.configured {
                self.configured = true;
                self.update_region();
                self.draw(qh);
            }
        }
    }

    impl SeatHandler for App {
        fn seat_state(&mut self) -> &mut SeatState {
            &mut self.seat
        }
        fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
        fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, cap: Capability) {
            if cap == Capability::Pointer && self.pointer.is_none() {
                self.pointer = Some(self.seat.get_pointer(qh, &seat).expect("pointer"));
            }
        }
        fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, cap: Capability) {
            if cap == Capability::Pointer {
                if let Some(p) = self.pointer.take() {
                    p.release();
                }
            }
        }
        fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    }

    impl PointerHandler for App {
        fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_pointer::WlPointer, events: &[PointerEvent]) {
            for e in events {
                if &e.surface != self.layer.wl_surface() {
                    continue;
                }
                if self.debug && !matches!(e.kind, PointerEventKind::Motion { .. }) {
                    eprintln!("pointer: {:?} at {:?}", e.kind, e.position);
                }
                match e.kind {
                    PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => self.pointer_at(e.position),
                    PointerEventKind::Leave { .. } => {
                        if !self.dragging {
                            self.scene.set_pointer(None);
                        }
                    }
                    PointerEventKind::Press { button: BTN_LEFT, .. } => self.press = Some(e.position),
                    PointerEventKind::Release { button: BTN_LEFT, .. } => {
                        if self.dragging {
                            self.scene.set_dragging(false);
                            if self.debug {
                                eprintln!("drag: drop at margins {:?}", self.margin);
                            }
                        } else if self.press.is_some() {
                            // a poke
                            self.scene.input.poke_until = self.scene.now() + 0.6;
                        }
                        self.dragging = false;
                        self.press = None;
                    }
                    _ => {}
                }
            }
        }
    }

    impl ShmHandler for App {
        fn shm_state(&mut self) -> &mut Shm {
            &mut self.shm
        }
    }

    smithay_client_toolkit::delegate_registry!(App);

    impl ProvidesRegistryState for App {
        fn registry(&mut self) -> &mut RegistryState {
            &mut self.registry
        }
        registry_handlers![OutputState, SeatState];
    }

    smithay_client_toolkit::delegate_dispatch2!(App);
}
