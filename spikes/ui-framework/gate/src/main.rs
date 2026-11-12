//! FNT-75's gate (R1): the CPU stack at the real pet window's worst case, to measure against the release iced pet.
//!
//! A wlr layer surface (smithay-client-toolkit) of 380 × 600 logical px, drawn by tiny-skia and cosmic-text into two
//! wl_shm buffers that are reused: a buffer the compositor still holds is never drawn into, and each buffer gets
//! what changed since it was last shown (the union of the frames since), copied from the composed window and
//! damaged alone. A frame that changes nothing commits nothing. Scale: wp_fractional_scale_v1 with wp_viewporter
//! (a buffer of round(logical × scale)), else the output's integer `buffer_scale`.
//!
//! GATE_PET=sprout|dewdrop, GATE_STATE=idle|working (and any other mood), GATE_SECONDS=<n> quits after n seconds,
//! GATE_DEBUG=1 logs the scale and a count of draws and damaged pixels every 5 s. The repository root is found from
//! the binary's place (spikes/ui-framework/target/release) or GATE_REPO. `spike-gate --png <file> [scale] [seconds]`
//! draws the scene without a window, for that many seconds of pet time, and saves the window as a PNG.

mod scene;
mod text;

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("this gate is Linux-only (Wayland with layer-shell)");
}

#[cfg(target_os = "linux")]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--png") {
        png(&args[2..]);
        return;
    }
    linux::run();
}

/// The scene without a window: `<file> [scale] [seconds]`, with the frames' damage summed.
fn png(args: &[String]) {
    let file = args.first().expect("--png <file> [scale] [seconds]");
    let scale: f32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(2.0);
    let seconds: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(2.0);
    let repo = std::env::var_os("GATE_REPO").map(std::path::PathBuf::from).unwrap_or_else(|| {
        let exe = std::env::current_exe().expect("the binary's path");
        exe.ancestors().nth(5).expect("spikes/ui-framework/target/release/<bin>").to_owned()
    });
    let pet = std::env::var("GATE_PET").unwrap_or_else(|_| "sprout".into());
    let state = std::env::var("GATE_STATE").unwrap_or_else(|_| "idle".into());
    let mut scene = scene::Scene::new(scene::pack(&pet, &repo), &state);
    let (mut frames, mut damaged, mut busy) = (0u32, 0u64, std::time::Duration::ZERO);
    let mut t = 0.0;
    while t <= seconds {
        let start = std::time::Instant::now();
        let damage = scene.frame(t, scale);
        if frames > 0 {
            busy += start.elapsed();
        }
        damaged += damage.iter().map(|r| u64::from(r.width()) * u64::from(r.height())).sum::<u64>();
        frames += 1;
        t += 0.033;
    }
    scene.master().save_png(file).expect("the PNG");
    eprintln!(
        "gate --png: {pet} {state} at {scale}: {frames} frames, {:.2} Mpx damaged, {:.2} ms per frame after the first",
        damaged as f64 / 1e6,
        busy.as_secs_f64() * 1000.0 / f64::from(frames.saturating_sub(1).max(1))
    );
}

#[cfg(target_os = "linux")]
mod linux {
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, FrameCallbackData, Region};
    use smithay_client_toolkit::dispatch2::Dispatch2;
    use smithay_client_toolkit::output::{OutputHandler, OutputState};
    use smithay_client_toolkit::reexports::calloop::EventLoop;
    use smithay_client_toolkit::reexports::calloop::timer::{TimeoutAction, Timer};
    use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
    use smithay_client_toolkit::reexports::client::globals::registry_queue_init;
    use smithay_client_toolkit::reexports::client::protocol::{wl_output, wl_shm, wl_surface};
    use smithay_client_toolkit::reexports::client::{Connection, QueueHandle};
    use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
    use smithay_client_toolkit::registry_handlers;
    use smithay_client_toolkit::shell::WaylandSurface;
    use smithay_client_toolkit::shell::wlr_layer::{
        Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
    };
    use smithay_client_toolkit::shm::slot::{Buffer, SlotPool};
    use smithay_client_toolkit::shm::{Shm, ShmHandler};
    use wayland_protocols::wp::fractional_scale::v1::client::{
        wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
        wp_fractional_scale_v1::{self, WpFractionalScaleV1},
    };
    use wayland_protocols::wp::viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter};

    use crate::scene::{self, Damage, Scene};

    /// The pet's calm pacing: a tick every 33 ms.
    const TICK: Duration = Duration::from_millis(33);

    pub fn run() {
        let debug = std::env::var_os("GATE_DEBUG").is_some();
        let quit_after = std::env::var("GATE_SECONDS").ok().and_then(|s| s.parse::<u64>().ok());
        let repo = std::env::var_os("GATE_REPO").map(PathBuf::from).unwrap_or_else(|| {
            let exe = std::env::current_exe().expect("the binary's path");
            exe.ancestors().nth(5).expect("spikes/ui-framework/target/release/<bin>").to_owned()
        });
        let pet = std::env::var("GATE_PET").unwrap_or_else(|_| "sprout".into());
        let state = std::env::var("GATE_STATE").unwrap_or_else(|_| "idle".into());

        let conn = Connection::connect_to_env().expect("no Wayland display");
        let (globals, queue) = registry_queue_init(&conn).unwrap();
        let qh = queue.handle();
        let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor");
        let layer_shell = LayerShell::bind(&globals, &qh).expect("this compositor has no layer-shell");
        let shm = Shm::bind(&globals, &qh).expect("wl_shm");
        let fractional: Option<WpFractionalScaleManagerV1> = globals.bind(&qh, 1..=1, Proto).ok();
        let viewporter: Option<WpViewporter> = globals.bind(&qh, 1..=1, Proto).ok();

        let surface = compositor.create_surface(&qh);
        let (fraction, viewport) = match (&fractional, &viewporter) {
            (Some(f), Some(v)) => (
                Some(f.get_fractional_scale(&surface, &qh, Proto)),
                Some(v.get_viewport(&surface, &qh, Proto)),
            ),
            _ => (None, None),
        };
        let layer = layer_shell.create_layer_surface(&qh, surface, Layer::Top, Some("aipet-gate"), None);
        // the top-left corner, away from the real pet in the bottom-right
        layer.set_anchor(Anchor::TOP | Anchor::LEFT);
        layer.set_margin(40, 0, 0, 40);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_size(scene::W as u32, scene::H as u32);
        layer.commit();

        let pool = SlotPool::new((scene::W * scene::H * 4.0 * 4.0) as usize, &shm).expect("shm pool");
        let mut app = App {
            registry: RegistryState::new(&globals),
            outputs: OutputState::new(&globals, &qh),
            shm,
            compositor,
            pool,
            buffers: Vec::new(),
            layer,
            viewport,
            _fraction: fraction,
            scale120: 120,
            configured: false,
            frame_pending: false,
            scene: Scene::new(scene::pack(&pet, &repo), &state),
            pending: Vec::new(),
            region: Vec::new(),
            region_at: Instant::now() - Duration::from_secs(1),
            exit: false,
            debug,
            started: Instant::now(),
            draws: 0,
            damaged_px: 0,
            commits: 0,
            report_at: Instant::now(),
            busy: Duration::ZERO,
        };

        let mut event_loop: EventLoop<App> = EventLoop::try_new().unwrap();
        WaylandSource::new(conn.clone(), queue).insert(event_loop.handle()).unwrap();
        event_loop
            .handle()
            .insert_source(Timer::from_duration(TICK), move |_, _, app: &mut App| {
                app.tick(&qh);
                TimeoutAction::ToDuration(TICK)
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
        eprintln!(
            "gate: {pet} {state}: {} frames, {} draws, {} commits, {:.1} Mpx damaged in {secs:.1} s",
            app.scene.frames,
            app.draws,
            app.commits,
            app.damaged_px as f64 / 1e6
        );
    }

    /// The user data of the protocols bound here: sctk has no helper for them.
    struct Proto;

    impl Dispatch2<WpFractionalScaleManagerV1, App> for Proto {
        fn event(&self, _: &mut App, _: &WpFractionalScaleManagerV1, _: <WpFractionalScaleManagerV1 as smithay_client_toolkit::reexports::client::Proxy>::Event, _: &Connection, _: &QueueHandle<App>) {}
    }

    impl Dispatch2<WpViewporter, App> for Proto {
        fn event(&self, _: &mut App, _: &WpViewporter, _: <WpViewporter as smithay_client_toolkit::reexports::client::Proxy>::Event, _: &Connection, _: &QueueHandle<App>) {}
    }

    impl Dispatch2<WpViewport, App> for Proto {
        fn event(&self, _: &mut App, _: &WpViewport, _: <WpViewport as smithay_client_toolkit::reexports::client::Proxy>::Event, _: &Connection, _: &QueueHandle<App>) {}
    }

    impl Dispatch2<WpFractionalScaleV1, App> for Proto {
        fn event(&self, app: &mut App, _: &WpFractionalScaleV1, event: wp_fractional_scale_v1::Event, _: &Connection, _: &QueueHandle<App>) {
            if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
                app.set_scale120(scale);
            }
        }
    }

    /// A reused wl_shm buffer and what changed since it was last shown.
    struct Slot {
        buffer: Buffer,
        size: (u32, u32),
        stale: Vec<Damage>,
    }

    struct App {
        registry: RegistryState,
        outputs: OutputState,
        shm: Shm,
        compositor: CompositorState,
        pool: SlotPool,
        buffers: Vec<Slot>,
        layer: LayerSurface,
        viewport: Option<WpViewport>,
        _fraction: Option<WpFractionalScaleV1>,
        /// The output's scale in 120ths.
        scale120: u32,
        configured: bool,
        frame_pending: bool,
        scene: Scene,
        /// What changed while the compositor was behind, to show with the next commit.
        pending: Vec<Damage>,
        region: Vec<(i32, i32, i32, i32)>,
        region_at: Instant,
        exit: bool,
        debug: bool,
        started: Instant,
        draws: u64,
        damaged_px: u64,
        commits: u64,
        report_at: Instant,
        /// Time spent in ticks: drawing, copying and committing.
        busy: Duration,
    }

    impl App {
        fn scale(&self) -> f32 {
            self.scale120 as f32 / 120.0
        }

        fn set_scale120(&mut self, s: u32) {
            if s != self.scale120 {
                self.scale120 = s.max(120 / 4);
                if self.debug {
                    eprintln!("gate: scale {}", self.scale());
                }
            }
        }

        fn tick(&mut self, qh: &QueueHandle<Self>) {
            if !self.configured {
                return;
            }
            let start = Instant::now();
            self.tick_inner(qh);
            self.busy += start.elapsed();
        }

        fn tick_inner(&mut self, qh: &QueueHandle<Self>) {
            let damage = self.scene.frame(self.scene_time(), self.scale());
            if !damage.is_empty() {
                self.draws += 1;
                self.pending.extend(damage);
                scene::merge(&mut self.pending);
            }
            if self.debug && self.report_at.elapsed() >= Duration::from_secs(5) {
                eprintln!(
                    "gate: {} draws, {} commits, {:.2} Mpx damaged so far, {:.1} % of a core in ticks",
                    self.draws,
                    self.commits,
                    self.damaged_px as f64 / 1e6,
                    self.busy.as_secs_f64() * 100.0 / self.started.elapsed().as_secs_f64()
                );
                self.report_at = Instant::now();
            }
            // calm: nothing changed, nothing to commit; behind: wait for the frame callback
            if self.pending.is_empty() || self.frame_pending {
                return;
            }
            self.update_region();
            self.present(qh);
        }

        fn scene_time(&self) -> f64 {
            self.started.elapsed().as_secs_f64()
        }

        /// The input region: the cards and the sprite's box, at most every 100 ms, as the iced pet does.
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
            self.region = rects;
            self.region_at = Instant::now();
        }

        /// Copies what changed into a free buffer and commits it, damaging only that.
        fn present(&mut self, qh: &QueueHandle<Self>) {
            let master = self.scene.master();
            let size = (master.width(), master.height());
            // a resized window starts its buffers afresh
            self.buffers.retain(|s| s.size == size);
            for slot in &mut self.buffers {
                slot.stale.extend(self.pending.iter().copied());
                scene::merge(&mut slot.stale);
            }
            // a buffer the compositor still holds has no canvas
            let free = (0..self.buffers.len()).find(|&i| self.buffers[i].buffer.canvas(&mut self.pool).is_some());
            let i = match free {
                Some(i) => i,
                None if self.buffers.len() < 2 => {
                    let (w, h) = (size.0 as i32, size.1 as i32);
                    let (buffer, _) = self
                        .pool
                        .create_buffer(w, h, w * 4, wl_shm::Format::Argb8888)
                        .expect("shm buffer");
                    self.buffers.push(Slot {
                        buffer,
                        size,
                        stale: vec![Damage::from_xywh(0, 0, size.0, size.1).unwrap()],
                    });
                    self.buffers.len() - 1
                }
                // both still with the compositor: try again next tick
                None => return,
            };
            let slot = &mut self.buffers[i];
            let Some(canvas) = slot.buffer.canvas(&mut self.pool) else {
                return;
            };
            let w = size.0 as usize;
            let src = master.data();
            for r in &slot.stale {
                let (x0, x1) = (r.x() as usize, r.x() as usize + r.width() as usize);
                for y in r.y() as usize..r.y() as usize + r.height() as usize {
                    let at = y * w * 4;
                    let (from, to) = (&src[at + x0 * 4..at + x1 * 4], &mut canvas[at + x0 * 4..at + x1 * 4]);
                    // premultiplied RGBA to wl_shm's ARGB8888 (BGRA in memory)
                    for (d, s) in to.chunks_exact_mut(4).zip(from.chunks_exact(4)) {
                        d[0] = s[2];
                        d[1] = s[1];
                        d[2] = s[0];
                        d[3] = s[3];
                    }
                }
            }
            let surface = self.layer.wl_surface();
            match &self.viewport {
                Some(viewport) => {
                    surface.set_buffer_scale(1);
                    viewport.set_destination(scene::W as i32, scene::H as i32);
                }
                None => surface.set_buffer_scale((self.scale120 / 120).max(1) as i32),
            }
            for r in &slot.stale {
                surface.damage_buffer(r.x(), r.y(), r.width() as i32, r.height() as i32);
                self.damaged_px += u64::from(r.width()) * u64::from(r.height());
            }
            slot.stale.clear();
            surface.frame(qh, FrameCallbackData(surface.clone()));
            slot.buffer.attach_to(surface).expect("attach");
            self.layer.commit();
            self.frame_pending = true;
            self.commits += 1;
            self.pending.clear();
        }
    }

    impl CompositorHandler for App {
        fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, f: i32) {
            // only without fractional scale: with it, the preferred scale says more
            if self.viewport.is_none() {
                self.set_scale120(f.max(1) as u32 * 120);
            }
        }
        fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
        fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {
            self.frame_pending = false;
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
        fn configure(&mut self, _: &Connection, qh: &QueueHandle<Self>, _: &LayerSurface, _: LayerSurfaceConfigure, _: u32) {
            if !self.configured {
                self.configured = true;
                self.tick(qh);
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
        registry_handlers![OutputState];
    }

    smithay_client_toolkit::delegate_dispatch2!(App);
}
