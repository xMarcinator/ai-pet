# iced_exwlshell 0.20.1: source-verified API notes for the AiPet Wayland shell

Scope: `iced_exwlshell` 0.20.1, `exwlshellev` 0.20.1, `iced_exwlshell_macros` 0.20.1, `iced_wayland_subscriber` 0.20.1,
on iced 0.14 (iced 0.14.0, iced_runtime 0.14.0, iced_futures 0.14.0, iced_widget 0.14.2, iced_wgpu 0.14.0,
iced_tiny_skia 0.14.1, iced_renderer 0.14.0). The git checkout at tag v0.20.1 is byte-identical to the registry
sources (`diff -r` of all three `src/` trees came back clean), so registry paths are cited.

## How to read the citations

| Alias | Path |
|---|---|
| `IE/` | `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/iced_exwlshell-0.20.1/src/` |
| `EV/` | `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/exwlshellev-0.20.1/src/` |
| `MAC` | `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/iced_exwlshell_macros-0.20.1/src/lib.rs` |
| `SUB/` | `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/iced_wayland_subscriber-0.20.1/src/` |
| `EX/` | `exwlshelleventloop/iced_examples/` in a clone of https://github.com/waycrate/exwlshelleventloop at tag v0.20.1 |
| `REPO/` | that clone |
| `iced_*-0.14.x/` | the matching crate under the same registry directory |
| `CHECK/` | `rust/notes/prototypes/exwlshell-check/` (compile-checked prototype, see below) |

Crate sources: `~/.cargo/registry/src/index.crates.io-*/` after a `cargo fetch` in `rust/`.

**Verification level.** Every snippet in this file comes from `CHECK/src/main.rs` (the full recommended design,
about 600 lines) or `CHECK/src/bin/snippets.rs`. Both pass `cargo check --offline` and `cargo clippy --offline` with
zero warnings on rustc/cargo/clippy 1.89.0 (edition 2024; iced features `tokio`, `image`, `canvas`). **Nothing was
run on screen.** Runtime behaviour on Hyprland (margins, implicit grab, popup grab serials) is inferred from source
and from the Wayland protocols, and is flagged as such.

Toolchain note: the crates use let-chains (e.g. `IE/multi_window.rs:399-401`, `EV/lib.rs:579-581`) and
`Vec::extract_if` (`EV/lib.rs:1928`, `EV/lib.rs:2804`). Those need Rust 1.88+ with edition 2024 and 1.87+
respectively, and iced 0.14 declares `rust-version = "1.88"` (`iced-0.14.0/Cargo.toml:14`). Rust 1.89 is fine,
confirmed by the compile check.

---

## 1. Entry points and builders

There are three builders. All of them end up in the same runtime,
`crate::multi_window::run(program, namespace, settings, renderer_settings, lock=false, hook, policy)`
(`IE/build_pattern/layershell.rs:411-419`, `IE/build_pattern/daemon.rs:711-719`).

| Builder | Signature | View gets the window id? |
|---|---|---|
| `iced_exwlshell::layershell::application` | `application(boot, namespace, update, view) -> SingleApplication<impl Program>` (`IE/build_pattern/layershell.rs:122-208`) | No. `view: Fn(&State) -> Element` (`layershell.rs:33-48`), and the id is dropped (`layershell.rs:179-185`) |
| `iced_exwlshell::layershell::timed` | `timed(boot, namespace, update: Fn(&mut S, M, Instant) -> Task, subscription: Fn(&S) -> Subscription, view)` (`IE/build_pattern/layershell/time.rs:18-126`) | No |
| `iced_exwlshell::daemon` | `daemon(boot, namespace, update, view) -> Daemon<impl Program>` (`IE/build_pattern/daemon.rs:157-244`) | **Yes.** `view: Fn(&State, window::Id) -> Element` (`daemon.rs:64-87`) |
| `iced_exwlshell::sessionlock` | lock screens only (`IE/build_pattern.rs:15-16`) | n/a |

Common argument traits (`IE/build_pattern/daemon.rs`):
- `boot`: `Fn() -> State` or `Fn() -> (State, Task<Message>)` (`daemon.rs:89-120`).
- `namespace`: `&'static str` or `Fn() -> String` (`daemon.rs:17-35`). This becomes the layer-shell namespace
  (`EV/lib.rs:2528`, `EV/lib.rs:3175`).
- `update`: `Fn(&mut State, Message) -> impl Into<Task<Message>>` (`daemon.rs:41-58`).
- Message bounds: `'static + TryInto<ExwlShellCustomActionWithId, Error = Message> + Send + Debug`
  (`daemon.rs:163-166`), plus `MaybeClone` for `run()` (`daemon.rs:662-669`). The macro provides the `TryInto`.
- The executor is `iced_futures::backend::default::Executor` (`layershell.rs:161`, `daemon.rs:197`). That is tokio
  if iced's `tokio` feature is on, else smol, else thread-pool (`iced_futures-0.14.0/src/backend/default.rs:10-30`).

### `SingleApplication<P>` methods (`IE/build_pattern/layershell.rs:365-541`)
```rust
pub fn run(self) -> iced_exwlshell::Result                                 // 366-420
pub fn settings(self, settings: Settings) -> Self                           // 422-424
pub fn antialiasing(self, antialiasing: bool) -> Self                       // 427-435
pub fn default_font(self, default_font: Font) -> Self                       // 438-446
pub fn layer_settings(self, layer_settings: LayerShellSettings) -> Self     // 449-457
pub fn font(self, font: impl Into<Cow<'static, [u8]>>) -> Self              // 460-463
pub fn default_text_size<P: Into<Pixels>>(self, size: P) -> Self            // 466-477
pub fn style(self, f: impl Fn(&State, &Theme) -> theme::Style) -> SingleApplication<impl Program>   // 479-489
pub fn subscription(self, f: impl Fn(&State) -> Subscription<Message>) -> SingleApplication<..>    // 491-501
pub fn theme(self, f: impl ThemeFn<State, Theme>) -> SingleApplication<..>  // 504-514; ThemeFn = Theme | Fn(&State) -> impl Into<Option<Theme>> (91-113)
pub fn scale_factor(self, f: impl Fn(&State) -> f32) -> SingleApplication<..>   // 517-527
pub fn executor<E: Executor>(self) -> SingleApplication<..>                 // 529-540
```
`run()` returns `Err(InvalidSettings)` for `StartMode::AllScreens | StartMode::Background` (`layershell.rs:402-410`).
`SingleApplication` has **no** `title`, `on_new_shell` or `redraw_scope`.

### `Daemon<P>` methods (`IE/build_pattern/daemon.rs:661-879`)
Everything `SingleApplication` has, with these differences and additions:
```rust
pub fn theme(self, f: impl ThemeFn<State, Theme>) -> Daemon<..>   // 830-841; ThemeFn = Theme | Fn(&State, window::Id) -> impl Into<Option<Theme>> (122-144)
pub fn scale_factor(self, f: impl Fn(&State, window::Id) -> f32) -> Daemon<..>   // 844-855
pub fn title(self, f: impl Fn(&State, window::Id) -> Option<String>) -> Daemon<..>   // 807-818
pub fn on_new_shell(self, f: impl Fn(ShellInfo) -> Option<Message> + 'static) -> Self   // 822-827
pub fn redraw_scope(self, scope: impl Fn(&Message) -> redraw::Scope + 'static) -> Self  // 875-878
pub fn style(self, f: impl Fn(&State, &Theme) -> theme::Style) -> Daemon<..>   // 780-791 (NOTE: no window id)
```
`Daemon::run()` accepts every `StartMode` (`daemon.rs:662-720`).

### `Settings` (`IE/settings.rs:20-86`)
```rust
pub struct Settings {
    pub id: Option<String>,                          // unused by the runtime (grep)
    pub layer_settings: LayerShellSettings,
    pub fonts: Vec<Cow<'static, [u8]>>,
    pub default_font: Font,
    pub default_text_size: Pixels,                   // default 16.0
    pub antialiasing: bool,                          // MSAAx4 when true (layershell.rs:395-399)
    pub virtual_keyboard_support: Option<VirtualKeyboardSettings>,
    pub with_connection: Option<WithConnection>,     // From<wayland_client::Connection> (EV/lib.rs:1039-1043)
    pub shell_broadcast: shell::ShellSender,         // see section 7
    pub keep_compositor_alive: bool,                 // default true (settings.rs:83)
}
```

### `LayerShellSettings` (`IE/settings.rs:88-115`) describes the *initial* surface
```rust
pub struct LayerShellSettings {
    pub anchor: Anchor,                     // default Anchor::all()
    pub layer: Layer,                       // default Layer::Top
    pub exclusive_zone: i32,                // default -1
    pub size: LayerSize,                    // default LayerSize::FILL
    pub margin: (i32, i32, i32, i32),       // (top, right, bottom, left); default zeros
    pub keyboard_interactivity: KeyboardInteractivity,   // default OnDemand
    pub start_mode: StartMode,              // default Active
    pub blur_option: BlurOption,            // default None
    pub events_transparent: bool,           // default false
}
```
These are wired into `exwlshellev::WindowState` at `IE/multi_window.rs:126-139`.

`StartMode` (`EV/lib.rs:1259-1275`):
- `Active` lets the compositor pick the output.
- `Background` creates no layer surface, only a role-less `wl_surface` (`EV/lib.rs:2507-2514`).
- `AllScreens` creates one surface per output, and more on hotplug (`EV/lib.rs:2588-2662`, `EV/lib.rs:2880-2948`).
- `TargetScreen(String)` binds by output name (`EV/lib.rs:2516-2520`).
- `TargetOutput(WlOutput)` needs the same connection.

`LayerSize`, `Extent` and `PixelSize` (`EV/size.rs:13-164`) are "surface-local pixels", i.e. logical:
- `LayerSize::px(w, h)` panics on 0 (`size.rs:69-75`).
- `LayerSize::fill_width(h)` / `fill_height(w)` and `LayerSize::FILL` exist.
- Missing anchor edges required by `Fill` are added automatically (`size.rs:105-113`, applied at
  `EV/lib.rs:788`, `EV/lib.rs:3162`).
- `PixelSize::px(w, h)` panics on 0 (`size.rs:131-138`); `try_px` returns `Option` instead.

Runtime-created layer surfaces use `NewLayerShellSettings` (`EV/events.rs:97-114`, default at `events.rs:187-202`):
```rust
pub struct NewLayerShellSettings {
    pub size: LayerSize, pub layer: Layer, pub anchor: Anchor,
    pub exclusive_zone: Option<i32>,               // None: set_exclusive_zone is never sent (EV/lib.rs:3184-3187)
    pub margin: Option<(i32, i32, i32, i32)>,      // (top, right, bottom, left) (EV/lib.rs:3189-3191); default Some((0,0,0,0))
    pub keyboard_interactivity: KeyboardInteractivity,   // default OnDemand
    pub output_option: OutputOption,               // Active | LastOutput | OutputName(String) | GlobalName(u32) | Output(WlOutput) (events.rs:77-95)
    pub events_transparent: bool,                  // empty input region at creation (EV/lib.rs:3193-3197)
    pub namespace: Option<String>,
    pub blur_option: BlurOption,
}
```

**Conclusion for AiPet:** use `daemon` (per-window `view`, `title`, `redraw_scope`, `StartMode::Background`) with
`#[to_layer_message(multi)]`. `EX/multi_window/src/main.rs:22-34` is the upstream model: a daemon with
`StartMode::Background` that opens its first layer surface from `boot` via `NewLayerShell`.

---

## 2. `#[to_layer_message]`, the variants it adds, and how actions are dispatched

Re-exported as `iced_exwlshell::{to_layer_message, to_exwlshell_message, to_sessionlock_message}`
(`IE/lib.rs:42-43`, feature `macros`, on by default: `iced_exwlshell-0.20.1/Cargo.toml.orig:14`). There is
**no** `AnchorChange`, `SizeChange` or `SetMargin`; anchor and size change together through `LayoutChange`.

### Single form: `#[to_layer_message]` (`MAC:335-377`)
Every action targets **the first window** (`id = None`, handled at `IE/multi_window.rs:829-843`).

| Variant | Payload |
|---|---|
| `LayoutChange { anchor, size }` | `Anchor`, `LayerSize` |
| `SetInputRegion(ActionCallback)` | `Arc<dyn Fn(&WlRegion) + Send + Sync>` wrapper (`IE/actions.rs:125-144`) |
| `LayerChange(Layer)` | |
| `MarginChange((i32, i32, i32, i32))` | (top, right, bottom, left), see the doc bug below |
| `ExclusiveZoneChange(i32)` | |
| `KeyboardInteractivityChange(KeyboardInteractivity)` | |
| `VirtualKeyboardPressed { key: u32 }` | panics if no virtual keyboard (`IE/multi_window.rs:912`) |
| `BlurOptionChange(BlurOption)` | |

The single form has **no** `NewPopUp`, `NewMenu`, `NewBaseWindow`, `NewLayerShell` or `RemoveWindow`.

### Multi form: `#[to_layer_message(multi)]` (`MAC:241-334`)
The per-surface variants carry `id: iced::window::Id`.

| Variant | Payload | Routed with id (`MAC:308-329`) |
|---|---|---|
| `LayoutChange { id, anchor: Anchor, size: LayerSize }` | | `Some(id)` |
| `SetInputRegion { id, callback: ActionCallback }` | | `Some(id)` |
| `LayerChange { id, layer: Layer }` | | `Some(id)` |
| `MarginChange { id, margin: (i32, i32, i32, i32) }` | | `Some(id)` |
| `BlurOptionChange { id, option: BlurOption }` | | `Some(id)` |
| `ExclusiveZoneChange { id, zone_size: i32 }` | | `Some(id)` |
| `KeyboardInteractivityChange { id, keyboard_interactivity }` | | `Some(id)` |
| `VirtualKeyboardPressed { key: u32 }` | | `None` |
| `NewLayerShell { settings: NewLayerShellSettings, id }` | `id` = the new surface's id | `None` |
| `NewBaseWindow { settings: IcedXdgWindowSettings, id }` | | `None` |
| `NewPopUp { settings: IcedNewPopupSettings, id }` | | `None` |
| `PopUpReposition { settings: IcedNewPopupSettings, id }` | `id` = the popup | `Some(id)` |
| `NewMenu { settings: IcedNewMenuSettings, id }` | | `None` |
| `NewInputPanel { settings: NewInputPanelSettings, id }` | | `None` |
| `RemoveWindow(window::Id)` | same as `window::close(id)` (`IE/actions.rs:188-189`) | `Some(id)` |
| `ForgetLastOutput` | | `None` |

The multi form also generates private inherent helpers that mint an id and a task (`MAC:266-300`):
`Message::layershell_open(NewLayerShellSettings)`, `popup_open(IcedNewPopupSettings)`, `menu_open(IcedNewMenuSettings)`
and `base_window_open(IcedXdgWindowSettings)`, each returning `(window::Id, Task<Message>)`.

`#[to_exwlshell_message]` (`MAC:16-159`) is the multi form plus `Lock` and `UnLock` (session lock), used in
`build_pattern/daemon.md:94`.

**Doc bug:** the macro comments say "Margin: top, left, bottom, right" (`MAC:44`, `MAC:247`, `MAC:340`). The tuple
actually goes to `zwlr_layer_surface_v1.set_margin(top, right, bottom, left)` (`EV/lib.rs:800-806`, `EV/lib.rs:1581-1584`).
**Use (top, right, bottom, left).**

### How to send them
Two paths, both verified in source:
1. **`Task::done(Message::X { .. })`** from `update` or `boot`. The task's output comes back as
   `Action::Output(msg)` and `run_action` tries `msg.try_into()`. On success it is pushed onto
   `waiting_layer_shell_actions` without ever reaching `update` (`IE/multi_window.rs:1329-1337`). The waiting
   actions run at the end of every event-loop callback (`IE/multi_window.rs:428-436`). This path costs one executor
   hop (runtime → proxy channel → calloop).
2. **Return it from a widget** (`button(..).on_press(Message::MarginChange { .. })`). Widget messages are diverted
   the same way in `update()` before the app's `update` is called (`IE/multi_window.rs:1276-1289`), within the same
   loop iteration.

An action whose `id` is not yet a live window is **re-queued** until the window exists (`IE/multi_window.rs:853-861`).
So `NewLayerShell` followed by `SetInputRegion` for the same id is safe. `update` never sees these variants, so a
`_ => Task::none()` arm is enough (`EX/multi_window/src/main.rs:170`).

```rust
// CHECK/src/main.rs (open_pet_surface)
let open = Task::done(Message::NewLayerShell {
    settings: NewLayerShellSettings {
        size: LayerSize::px(SURFACE_W, self.surface_height()),
        layer: Layer::Top,
        anchor: Anchor::Bottom | Anchor::Left,
        exclusive_zone: None,
        margin: Some((0, 0, self.bottom, self.left)), // (top, right, bottom, left)
        keyboard_interactivity: KeyboardInteractivity::None,
        output_option: OutputOption::Active,
        events_transparent: true,
        namespace: Some("aipet".into()),
        ..Default::default()
    },
    id: self.pet_id,
});
open.chain(self.input_region()) // queued until the surface exists
```

---

## 3. Input regions

**Mechanics** (`IE/multi_window.rs:887-909`):
```rust
let window_size = exshell_window.get_size();          // last configured size
region.subtract(0, 0, width, height);                 // clear the shared region over the surface
set_region(region);                                   // your ActionCallback: region.add(...)
exshell_window.get_wlsurface().set_input_region(self.wl_input_region.as_ref());
exshell_window.get_wlsurface().commit();
```
- **One `WlRegion` is shared by all surfaces.** It is created once at bind time (`IE/multi_window.rs:201-208`) and
  stored at `IE/multi_window.rs:414`. `subtract` clears only `(0, 0, w, h)` of the target surface. Rectangles you add
  *outside* a surface's bounds stay in the shared region forever. That is harmless (the compositor ignores input
  region outside the surface) but untidy, so **clamp your rectangles to the surface**. `wl_surface.set_input_region`
  copies the region, so sharing it is otherwise safe (Wayland protocol semantics).
- `get_size()` is the size from the **last configure** (`EV/lib.rs:862-865`; set at `EV/lib.rs:1999` for layer
  surfaces). If you send `LayoutChange` to grow the surface and then `SetInputRegion` before the configure lands,
  the grown area is not cleared. **Re-send the region on `window::Event::Resized` for the pet**, which the prototype
  does. `Resized` is emitted on size or scale change at `IE/multi_window.rs:460-473`.

**Units.** `wl_region.add(x, y, w, h)` takes `i32` **surface-local coordinates, which are logical pixels.**
- The runtime renders at `ceil(logical × fractional_scale)` physical pixels and maps the buffer back to the logical
  size with `wp_viewport.set_destination(logical_w, logical_h)` (`IE/multi_window/state.rs:232-250`). The surface
  coordinate space therefore stays logical at any fractional scale.
- iced layout coordinates equal surface-local coordinates divided by the app `scale_factor()` (`IE/conversion.rs:16-26`,
  `IE/multi_window/state.rs:163-172`). **With the default app `scale_factor` of 1.0, iced layout rectangles can be
  passed straight to `region.add`** (round outward to integers). If you set `.scale_factor(..) != 1.0`, multiply by it.
- Upstream uses layer-size units directly: `LayerSize::px(400, 400)` with `region.add(0, 0, 400, 70)`
  (`EX/input_regions/src/main.rs:17-20,48-57`).

**Default input region.**
- No `set_input_region` is issued unless `events_transparent` is true. The Wayland default is the whole surface, so
  **transparent pixels eat clicks until you set a region.**
- `events_transparent: true` sets an empty region at creation: `EV/lib.rs:2547-2551` (initial surface),
  `EV/lib.rs:3193-3197` (`NewLayerShell`), `EV/lib.rs:2618-2622` / `EV/lib.rs:2915-2919` (all-screens).
  This makes the pet click-through from its first frame. A later `SetInputRegion` replaces it.
- Popups and xdg windows never get a region, so they stay fully clickable.

**Cost, and whether to send it every frame.** Each call is one `subtract`, N `add`s, one `set_input_region` and one
`wl_surface.commit`. There is no roundtrip, and the region object is reused, not recreated. Going through
`Task::done` adds one executor hop. It *can* be sent every frame. Two reasons to send only on change:
- Every call does an **extra `wl_surface.commit` outside the render path** (`IE/multi_window.rs:908`).
- Each `Task::done` message goes through the runtime.

An exact per-pixel region from the sprite's alpha is about 24 rows × a few runs, i.e. tens of rectangles. That is cheap.

```rust
// CHECK/src/main.rs (input_region + alpha_runs)
let mut rects = alpha_runs(&self.sprite_rgba, SPRITE_W, SPRITE_H, ZOOM, origin.x as i32, origin.y as i32);
for i in 0..self.bubbles.len() as u32 {
    rects.push((0, (i * (BUBBLE_H + GAP)) as i32, BUBBLE_W as i32, BUBBLE_H as i32));
}
Task::done(Message::SetInputRegion {
    id: self.pet_id,
    callback: ActionCallback::new(move |region| {
        for &(x, y, w, h) in &rects { region.add(x, y, w, h); }
    }),
})
```
Rounded bubble corners are rectangles in the region. Approximate each corner with 2-3 stair rectangles if the
corner pixels must pass clicks through.

---

## 4. Moving the surface, pointer coordinates, and dragging

**`MarginChange`** calls `layer_surface.set_margin(top, right, bottom, left)` and then **`wl_surface.commit()`
immediately** (`EV/lib.rs:800-806`, dispatched at `IE/multi_window.rs:875-878`). Margins only matter on anchored
edges. With `Anchor::Bottom | Anchor::Left`, `bottom` and `left` position the surface and `top` and `right` are
ignored (layer-shell protocol).

**`LayoutChange { anchor, size }`** calls `set_anchor(size.resolve_anchor(anchor))` and `set_size(..)`, then commits
(`EV/lib.rs:782-793`, `EV/lib.rs:816-820`). A size change produces a configure, which triggers a relayout and a
`Resized` event (`EV/lib.rs:1989-2002`, `IE/multi_window.rs:460-473`).

**Pointer coordinates are surface-local and nothing else.**
- `wl_pointer.motion` `surface_x/surface_y` are forwarded untouched (`EV/seat.rs:715-728`), converted to
  `CursorMoved` (`IE/event.rs:131-135`), and divided by the app scale factor (`IE/conversion.rs:35-43`).
- Events are routed to the surface the pointer *entered* (`EV/seat.rs:523-527`, `EV/seat.rs:694-714`).
- There is **no global pointer position** anywhere in the API, and Wayland does not expose one.
- `window::drag` (xdg `move`) is not implemented: every `window::Action` other than Close, GetOldest, GetLatest,
  GetSize, Screenshot and GetScaleFactor is dropped (`IE/multi_window.rs:1379-1420`, `_ => {}` at 1419). It would
  not apply to a layer surface anyway.
- **None of these exist (grep):** zwp_relative_pointer, pointer constraints, `set_buffer_scale`,
  `xdg_toplevel.set_app_id`, min/max size.

**The drag problem.** After you send new margins, motion events keep arriving relative to *some* origin: the old one
until the compositor applies the commit, the new one afterwards. The client cannot tell which, because there is no
acknowledgement for a margin-only change. The two naive approaches both misbehave:
- Applying `p - grab` incrementally double-counts deltas.
- Applying it absolutely against the last *sent* origin overshoots by whatever is still in flight, and can leave the
  pet offset from the cursor when the mouse stops.

**Recommended algorithm ("confirmed origin")**, compile-checked:
1. On left press inside the sprite, record `grab = cursor` (surface-local) and start the drag.
2. On `CursorMoved(p)` with no move pending: set `target = confirmed_margins + (p - grab)`, clamp it, send one
   `MarginChange`, and mark it pending with a counter of 2.
3. **While a move is pending, ignore motion events.**
4. On each `RedrawRequested` of the pet, decrement the counter. When it reaches 0, `confirmed_margins = target`.

Why two pet frames are enough:
- A pet frame is only drawn after the frame callback of the previous present (`EV/lib.rs:943-953`, `EV/lib.rs:965-979`,
  `EV/lib.rs:2347-2363`). The callback is requested in `on_pre_present` (`IE/multi_window.rs:657-665`).
- So the second `RedrawRequested` after the margin commit proves the compositor has processed a later commit, and
  with it (commits are applied in order) the margin commit.
- Within one dispatch batch, motion messages are broadcast before the refresh loop's `RedrawRequested`
  (`EV/lib.rs:2830-3031` then `3558-3623`). Every motion event seen after that `RedrawRequested` is therefore
  relative to the new origin.
- This requires `listen_raw`, because `listen_with` drops `RedrawRequested` (`iced_futures-0.14.0/src/event.rs:26-47`
  vs `54-71`), and one subscription so that both event kinds stay ordered.

The cost is that the pet moves at most every other frame, i.e. 30 Hz on a 60 Hz output and 72 Hz on 144 Hz. In
exchange it has no drift and no overshoot.

*Assumption to verify on Hyprland:* the compositor applies layer margins when it processes the commit, which is
standard double-buffered layer-shell state. The algorithm also holds if it defers the change to its next repaint.

```rust
// CHECK/src/main.rs (pet_event)
Event::Mouse(mouse::Event::CursorMoved { position }) => {
    self.cursor = Some(position);
    let Some(drag) = self.drag.as_mut() else { return Task::none() };
    if drag.pending.is_some() { return Task::none(); }          // stale origin: ignore
    let dx = (position.x - drag.grab.x).round() as i32;
    let dy = (position.y - drag.grab.y).round() as i32;
    if dx == 0 && dy == 0 { return Task::none(); }
    drag.moved = true;
    let (left, bottom) = self.clamp(self.left + dx, self.bottom - dy); // anchor Bottom|Left: y grows down
    if let Some(drag) = self.drag.as_mut() { drag.pending = Some((left, bottom, 2)); }
    Task::done(Message::MarginChange { id: self.pet_id, margin: (0, 0, bottom, left) })
}
Event::Window(window::Event::RedrawRequested(_)) => {
    if let Some(drag) = self.drag.as_mut()
        && let Some((left, bottom, frames)) = drag.pending
    {
        if frames <= 1 { self.left = left; self.bottom = bottom; drag.pending = None; }
        else { drag.pending = Some((left, bottom, frames - 1)); }
    }
    Task::none()
}
```
```rust
// CHECK/src/main.rs (subscription): raw events only while dragging, since they form a redraw loop
if self.drag.is_some() {
    subscriptions.push(iced::event::listen_raw(|event, _status, id| match event {
        Event::Mouse(_)
        | Event::Window(window::Event::RedrawRequested(_) | window::Event::Resized(_)) => Some(Message::Input(id, event)),
        _ => None,
    }));
} else {
    subscriptions.push(iced::event::listen_with(|event, _status, id| match event {
        Event::Mouse(_) | Event::Window(window::Event::Resized(_)) => Some(Message::Input(id, event)),
        _ => None,
    }));
}
```
Further notes:
- **Implicit grab.** Fast drags that leave the surface keep delivering motion to it while the button is held. That
  is Wayland's implicit pointer grab, a compositor behaviour, and the input region does not matter during it.
  exwlshellev keeps routing to the entered surface until `leave` (`EV/seat.rs:681-714`). *Verify on Hyprland.*
- **Side buttons count as left.** Every button code except 273 (right) and 274 (middle) becomes `Left`
  (`IE/event.rs:12-18`), so side buttons can start a drag. Filter by position or ignore it.
- **Multi-monitor.** A layer surface is bound to one output. Moving the pet to another monitor means `RemoveWindow`
  plus `NewLayerShell { output_option: OutputOption::OutputName(..) | GlobalName(..) }`, with margins derived from
  `OutputInfo::logical_position` (section 7).
- **Fallback if margins misbehave on Hyprland: a full-output overlay.** Use `Anchor::all()` + `LayerSize::FILL`,
  draw the pet at an offset inside, and move it by changing that offset. Surface-local then equals output-local,
  drag math is exact and immediate, and the menu can be drawn inside. The cost: every animated frame re-renders
  and presents a full-output buffer, because the wgpu path has no partial damage (section 8), so the compositor
  repaints the whole output per frame.

---

## 5. Popups and menus

**`NewPopUp { settings: IcedNewPopupSettings, id }`** (`IE/actions.rs:33-117`, handled at `IE/multi_window.rs:959-994`
and `EV/lib.rs:3238-3334`):
```rust
pub struct IcedNewPopupSettings { pub size: PixelSize, pub parent: Option<window::Id>, pub placement: PopupPlacement,
    pub anchor: PopupAnchor, pub gravity: PopupGravity, pub constraint_adjustment: PopupConstraintAdjustment }
IcedNewPopupSettings::new(parent, size, anchor_position: (i32, i32), anchor_size: PixelSize)   // anchor rect
IcedNewPopupSettings::at_position(parent, size, position)                                     // 1x1 anchor rect
IcedNewPopupSettings::on_current_surface(size, anchor_position, anchor_size)                  // parent = None
IcedNewPopupSettings::at_position_on_current_surface(size, position)
    .anchor(PopupAnchor) .gravity(PopupGravity) .constraint_adjustment(PopupConstraintAdjustment)
// defaults: anchor BottomLeft, gravity BottomRight, FlipX|FlipY|SlideX|SlideY (actions.rs:83-95)
```
- **Positioning** uses a real `xdg_positioner` relative to the **parent's surface-local (logical) coordinates**
  (`EV/events.rs:116-128`, `EV/lib.rs:3793-3822`). `size` is logical and fixed up front. `set_reactive` is sent when
  the positioner is v3+ (`EV/lib.rs:3818-3820`), so the compositor re-places the popup if the parent moves.
- **Parent.** It must be a layer surface or another popup. An xdg-toplevel parent is skipped with a warning
  (`EV/lib.rs:3268-3291`). `parent: None` falls back to keyboard focus, then pointer surface, then last surface
  (`EV/lib.rs:1783-1794`). **An unknown parent id makes the whole request a silent no-op** (`IE/multi_window.rs:971-977`).
- **Grab and dismissal.**
  - The runtime takes `take_popup_grab_serial()`: the last button-press serial (consumed), otherwise the pointer-enter
    serial (`EV/lib.rs:1130-1133`, `IE/multi_window.rs:978`).
  - It then calls `xdg_popup.grab(seat, serial)` (`EV/lib.rs:3295-3302`). With a grab, the compositor dismisses the
    popup on a click outside the client's surfaces by sending `popup_done`, which leads to `request_close` and the
    Closed event (`EV/lib.rs:2090-2095`).
  - Opening the popup in response to the right-button *press* gives it a valid press serial. The enter-serial
    fallback may be rejected by some compositors, which dismiss the popup immediately. *Verify on Hyprland.*
- **Its own view.** A popup is a separate `wl_surface` with its own renderer and GPU surface
  (`IE/multi_window/window_manager.rs:81-115`). `daemon`'s `view(state, id)` is called with the popup's id
  (`IE/user_interface.rs:138-149`), so dispatch on `id` (`EX/counter_multi/src/main.rs:282-329`). The `application`
  builder would show the same view in every surface.
- **Closing.** `iced::window::close(popup_id)` becomes `RemoveWindow` and then `request_close`
  (`IE/multi_window.rs:1380-1382`, `954-958`). Closing a parent closes its popups first (`EV/lib.rs:1174-1188`,
  `3528-3539`). Every close reaches the app as `window::close_events()` (`IE/multi_window.rs:724-729`,
  `iced_runtime-0.14.0/src/window.rs:238-246`).

**`PopUpReposition { settings, id }`** calls `xdg_popup.reposition` with a new positioner (`EV/lib.rs:3335-3378`). It
needs xdg_popup v3 and otherwise logs and does nothing (`EV/lib.rs:3357-3364`). `settings.parent` is ignored
(`IE/actions.rs:172-175`). Upstream example: `EX/counter_multi/src/main.rs:309-315`.

**`NewMenu { settings: IcedNewMenuSettings { size, gravity }, id }`** (`IE/multi_window.rs:1018-1053`):
- It opens a popup at the current pointer position on the surface under the pointer
  (`PopupPlacement::Position`, anchor `TopLeft`).
- It is silently skipped when the pointer is on no surface (`IE/multi_window.rs:1022-1032`).
- `grab_serial: None` (`IE/multi_window.rs:1045`) means **no grab**, so nothing dismisses it for you.
- CHANGELOG 0.19.0 lists `NewMenu`, `IcedNewMenuSettings` and `menu_open` as *removed* (`REPO/CHANGELOG.md`, 0.19.0
  "Removed (breaking)"), yet 0.20.1 still ships them (`IE/actions.rs:119-123`, `MAC:64`). **Treat `NewMenu` as
  unstable and use `NewPopUp`.**

The upstream README also calls popup support "only a toy" (`REPO/README.md:13`). It works, but plan for rough edges.

```rust
// CHECK/src/main.rs (right-press on the sprite opens the context menu)
let id = window::Id::unique();
self.menu_id = Some(id);
Task::done(Message::NewPopUp {
    settings: IcedNewPopupSettings::new(
        self.pet_id,
        PixelSize::px(MENU.0, MENU.1),
        (position.x as i32, position.y as i32),   // click point, pet-surface-local
        PixelSize::px(1, 1),
    )
    .gravity(PopupGravity::TopRight),              // grow up-right; flips/slides near edges
    id,
})
```
**Alternative: draw the menu inside the pet surface.**
- It needs room, which means growing the surface with `LayoutChange` and adding the menu to the input region.
- Outside clicks go to other clients, so the only way to learn about them is keyboard focus loss. That requires
  `KeyboardInteractivity::OnDemand`, which would let the pet steal focus on click.
- The popup is simpler and gets dismissal from the grab.

---

## 6. Normal xdg windows next to the layer surface (Settings)

**`NewBaseWindow { settings: IcedXdgWindowSettings, id }`**, or `Message::base_window_open(settings)`
(`IE/actions.rs:15-31`, handled at `IE/multi_window.rs:942-953` and `EV/lib.rs:3379-3438`):
```rust
pub struct IcedXdgWindowSettings { pub size: Option<PixelSize>, pub client_side_decorations: bool }  // Default: None, false
```
- **Size.** It is the initial size, defaulting to 300×300 (`EV/lib.rs:3432`). After that the compositor's configure
  wins (`EV/lib.rs:2024-2033`). There is no min/max size API.
- **Decorations.** If the compositor offers `zxdg_decoration_manager_v1`, the runtime requests ServerSide, or
  ClientSide when `client_side_decorations` is true (`EV/lib.rs:3394-3408`). iced_exwlshell draws no client-side
  decorations, so CSD means "undecorated". On Hyprland server-side means Hyprland's borders and no title bar.
- **Title.** The xdg title starts as `""` because `IcedXdgWindowSettings` always sends `title: None`
  (`IE/actions.rs:23-31`, `EV/lib.rs:3392`). `Daemon::title(..)` is applied only in `State::synchronize`
  (`IE/multi_window/state.rs:193-197`, `state.rs:38-43`). That runs after every `update` batch
  (`IE/multi_window.rs:1154-1156`), so the title appears after the next message; with a ticking pet that is
  immediate.
- **No `app_id`.** Nothing calls `set_app_id` (grep), so Hyprland window rules must match on **title**, not
  class/app_id.
- **Closing.**
  - Close it yourself with `window::close(id)`.
  - When the user closes it (compositor `xdg_toplevel.close`), the window is **destroyed immediately**
    (`EV/lib.rs:2034-2039`, then `EV/lib.rs:3514-3539`).
  - There is **no `CloseRequested`** (grep: the runtime emits only `Opened`, `Resized`, `Closed`, `RedrawRequested`,
    focus and input events). The only signal is `window::Event::Closed` / `window::close_events()`
    (`IE/multi_window.rs:704-735`).
  - Unsaved settings cannot veto the close; save on change.
- **View dispatch.** The same `view(state, id)`, so `if Some(id) == self.settings_id { .. }`
  (`EX/counter_multi/src/main.rs:286-301`).
- **Overlays.** iced overlays such as `pick_list` dropdowns and tooltips render inside the Settings surface. They are
  not xdg popups, so they work but are clipped to the window.
- **Transparency.** `style()` has no window id (`IE/build_pattern/daemon.rs:780-791`), so every surface shares one
  clear colour (`IE/multi_window.rs:657-661`, `IE/multi_window/state.rs:135-137`). With a transparent clear colour
  for the pet, **the Settings view must paint its own opaque background.** Alternatively, return a distinct `Theme`
  per window from `theme(state, id)` and branch on the theme in `style`: `synchronize` calls `style(self.theme())`
  per window (`state.rs:203-204`).

```rust
// CHECK/src/main.rs (menu "Settings")
let (id, open) = Message::base_window_open(IcedXdgWindowSettings {
    size: Some(PixelSize::px(520, 420)),
    client_side_decorations: false,
});
self.settings_id = Some(id);
Task::batch([close_menu, open])
// ...
fn title(&self, id: window::Id) -> Option<String> {
    (Some(id) == self.settings_id).then(|| "AiPet Settings".to_owned())
}
// settings_view(): container(..).width(Fill).height(Fill)
//     .style(|t: &Theme| container::Style { background: Some(t.palette().background.into()), ..Default::default() })
```

---

## 7. Output (monitor) info and scaling

**`Settings::shell_broadcast`** (`IE/settings.rs:61-64`) combined with `iced_wayland_subscriber::shell::channel()`
(`SUB/shell.rs:89-93`). Subscribe with `ShellReceiver::listen()`. It replays the current monitors, shells and
window→output mappings to every new subscriber (`SUB/shell.rs:149-177`). Events (`SUB/shell.rs:51-75`):
- `NewShell(ShellInfo { window, shell })`, sent when a surface is materialized (`IE/multi_window.rs:488-492`)
- `WindowOutputChanged { window, output: Option<OutputInfo> }`, on `wl_surface.enter/leave` and when that output's
  info changes (`IE/multi_window.rs:493-503`, `780-786`; `EV/lib.rs:1884-1906`, `EV/lib.rs:2137-2184`)
- `OutputAdded` / `OutputUpdated` / `OutputRemoved(OutputInfo)` (`IE/multi_window.rs:742-757`)
- `Closed(id)` (`SUB/shell.rs:125-131`)

`OutputInfo` is sctk 0.21.1's (`SUB/info.rs:5`). Its fields (`smithay-client-toolkit-0.21.1/src/output.rs:317-377`):
`id` (the registry global name, usable with `OutputOption::GlobalName`), `name`, `location`, `modes`
(`SUB/info.rs:17-23` `pixel_size()` returns the current mode in physical px), integer `scale_factor`,
**`logical_position: Option<(i32,i32)>`** and **`logical_size: Option<(i32,i32)>`**. `logical_size` is the right
bound for clamping margins, since margins are logical too.

Alternatively, `iced_wayland_subscriber::output::listen(connection)` (`SUB/output.rs:110`) runs its own queue
and needs the shared connection passed through `with_connection`.

```rust
// CHECK/src/main.rs
Message::Shell(ShellEvent::WindowOutputChanged { window, output }) => {
    if window == self.pet_id {
        self.output = output.as_ref().map(|info| info.id);
        self.output_size = output.and_then(|info| info.logical_size);
    }
    Task::none()
}
Message::Shell(ShellEvent::OutputUpdated(info)) => {
    if Some(info.id) == self.output { self.output_size = info.logical_size; }
    Task::none()
}
```
With `OutputOption::Active` the output is unknown until the first `wl_surface.enter`, which only happens after the
surface maps. Clamping needs a fallback until then; the prototype clamps only at 0.

**Fractional scaling is supported, and viewporter is mandatory.**
- `wp_fractional_scale_manager_v1` is bound if present (`EV/lib.rs:2483-2485`), and every surface type gets a
  `wp_fractional_scale_v1` (e.g. `EV/lib.rs:2555-2559`, `3206-3214`, `3304-3312`, `3409-3417`).
- `preferred_scale` updates the surface scale (`scale/120`) and requests a redraw (`EV/lib.rs:2108-2136`). The default
  is 120, i.e. 1.0, until the first event arrives (`EV/lib.rs:453-454`).
- `wp_viewporter` is bound optionally (`EV/lib.rs:2473`), **but `State::new` panics without it**:
  `.expect("iced_layershell need viewport support ...")` (`IE/multi_window/state.rs:65-68`).
- There is no `set_buffer_scale` path (grep). Without fractional-scale support the surface renders at 1× and gets
  upscaled.

To read the fractional scale in the app, use `iced::window::scale_factor(id)`, which returns the Wayland scale of
that surface (`IE/multi_window.rs:1414-1417`, `iced_runtime-0.14.0/src/window.rs:376-380`). Scale changes are
**not** reported as `window::Event::Rescaled`, because `ScaleFactorChanged` maps to nothing (`IE/conversion.rs:158`).
They do trigger `Resized` (`IE/multi_window.rs:460-473`), so re-query the scale on `Resized`.
```rust
// CHECK/src/bin/snippets.rs
Msg::Appeared(id, ShellType::LayerShell) => window::scale_factor(id).map(move |scale| Msg::Scale(id, scale)),
```
Pixel art: 5 logical px per sprite pixel at a scale of 1.25 is 6.25 physical px, so Nearest filtering gives uneven
columns. Pick `zoom_logical = round(5 × s) / s` so each sprite pixel covers an integer number of physical pixels,
and compute the input-region rectangles from the same numbers.

---

## 8. Animation, redraw model, idle CPU, transparency

**Redraws are request-driven and paced by frame callbacks, not continuous:**
- Each unit has a `RefreshRequest` of `NextFrame`, `At(Instant)` or `Wait` (`EV/lib.rs:515-526`, merging at
  `EV/lib.rs:889-904`).
- A frame is drawn only when the surface is configured, a refresh is due, **and** the frame callback of the
  previous present has fired (`EV/lib.rs:905-953`). The callback is requested just before present
  (`IE/multi_window.rs:657-665`, `EV/lib.rs:965-979`) and marks the slot available on `done`
  (`EV/lib.rs:2347-2363`). So at most one frame per compositor frame (vsync).
- **What triggers a refresh:**
  - any app message: `request_refresh_all(NextFrame)` under the default policy (`IE/multi_window.rs:1125-1143`,
    `IE/redraw.rs:46-49`)
  - a widget calling `shell.request_redraw()` or `request_redraw_at(..)` (`IE/multi_window.rs:1203-1214`)
  - configure events (`EV/lib.rs:1972`, `EV/lib.rs:2001`) and scale changes (`EV/lib.rs:2126`)
- **Idle.** When no unit wants a refresh, the calloop dispatch timeout is `None` and the loop blocks indefinitely
  (`EV/lib.rs:1135-1149`, `EV/lib.rs:3635-3643`). Idle CPU is then:
  - your own timers
  - the smithay-clipboard worker thread, about 0.4% CPU according to `IE/multi_window.rs:372-374`; turn it off with
    `iced_exwlshell::disable_clipboard()` (`IE/lib.rs:51-56`) at the cost of copy/paste in Settings
  - the `mundy` colour-scheme stream (`IE/multi_window.rs:141-168`), which also **blocks startup for up to 200 ms**
    (`IE/multi_window.rs:159-167`). It sits behind the default feature `linux-theme-detection`
    (`Cargo.toml.orig:14,20`); drop it with `default-features = false, features = ["macros"]`.
- **Every message rebuilds every window's view** (`IE/multi_window.rs:1144-1173`, `IE/user_interface.rs:138-149`).
  Keep `view` cheap, and use `redraw_scope` so ticks redraw only the pet (`IE/build_pattern/daemon.rs:872-878`,
  `IE/redraw.rs:46-83`).
- **No partial damage on the wgpu path.** Each refresh re-renders and presents the full surface
  (`IE/multi_window.rs:639-665`). tiny-skia does diff damage (`iced_tiny_skia-0.14.1/src/window/compositor.rs:176-214`),
  but see section 9.

**Ways to drive 60 fps:**
1. **`iced::time::every(Duration::from_millis(16))`.** It needs the iced `tokio` or `smol` feature
   (`iced-0.14.0/src/time.rs:4-13`, `iced_futures-0.14.0/src/backend/native/tokio.rs:38-57`). Each tick is a message,
   so it redraws according to `redraw_scope`. The sprite probably animates at 8-12 fps, so tick at that rate and let
   the drag drive frames while dragging.
2. **`iced::window::frames()`.** `listen_raw` on `RedrawRequested` from **every** window
   (`iced_runtime-0.14.0/src/window.rs:208-213`). It is self-sustaining at the display refresh rate. Scope it to the
   pet with `redraw_scope`, or it redraws Settings too.
3. **A custom widget that calls `shell.request_redraw()`** on `RedrawRequested` (`EX/redraw/src/main.rs:148-181`).
   There is no message, no `update` and no view rebuild; only that surface redraws. It is the cheapest option for
   continuous animation derived from `Instant`.

```rust
// CHECK/src/main.rs (main): a pet id minted before the builder, so the 'static redraw_scope closure can use it
let pet_id = window::Id::unique();
daemon(move || Pet::boot(pet_id, shell_rx.clone()), "aipet", Pet::update, Pet::view)
    .title(Pet::title)
    .style(|_state: &Pet, theme: &Theme| iced::theme::Style {
        background_color: Color::TRANSPARENT,          // transparent clear colour (shared by all surfaces)
        text_color: theme.palette().text,
    })
    .subscription(Pet::subscription)                   // includes iced::time::every(100 ms)
    .redraw_scope(move |message: &Message| match message {
        Message::Tick(_) => Scope::Window(pet_id),
        Message::Input(id, Event::Window(window::Event::RedrawRequested(_))) => {
            if *id == pet_id { Scope::Window(pet_id) } else { Scope::None }
        }
        Message::Input(id, _) => Scope::Window(*id),
        _ => Scope::All,
    })
    .settings(Settings {
        layer_settings: LayerShellSettings { start_mode: StartMode::Background, ..Default::default() },
        shell_broadcast: shell_tx,
        ..Default::default()
    })
    .run()
```
**Transparency.** `style()`'s `background_color` is the clear colour passed to `present` (`IE/multi_window.rs:657-661`).
The default style is the theme's opaque base (`iced_program-0.14.0/src/lib.rs:108-110`), so you must set a style
with `Color::TRANSPARENT` or an alpha colour (`EX/counter_timer/src/main.rs:141-147`). Real per-pixel transparency
also needs an alpha-capable swapchain; see the next section.

---

## 9. Renderer: wgpu, tiny-skia, image and canvas

- The compositor type is `<P::Renderer as compositor::Default>::Compositor` (`IE/multi_window.rs:173-177`). It is
  created synchronously (`futures::executor::block_on`) on the first `Refresh`, with the first surface as the
  compatible window (`IE/multi_window.rs:363-380`, `399-411`).
- With iced's default features (`wgpu`, `tiny-skia`, ...: `iced-0.14.0/Cargo.toml:64-73`), the renderer is
  `fallback::Renderer<iced_wgpu, iced_tiny_skia>` (`iced_renderer-0.14.0/src/lib.rs:25-36`).
  - It tries wgpu first, then tiny-skia, and `ICED_BACKEND` can override the order (`iced_renderer-0.14.0/src/fallback.rs:267-330`).
  - `Compositor::new` equals `with_backend(.., None)` (`iced_graphics-0.14.0/src/compositor.rs:21-28`).
- **wgpu transparency works:** it prefers `PostMultiplied`, then `PreMultiplied`, then `Auto`
  (`iced_wgpu-0.14.0/src/window/compositor.rs:123-136`). The present mode defaults to `AutoVsync`; override it with
  `ICED_PRESENT_MODE` (`iced_wgpu-0.14.0/src/settings.rs:32-35`, `60-81`).
- **The tiny-skia fallback is opaque on Wayland.** softbuffer 0.4.8 creates `wl_shm` buffers as `Xrgb8888`
  (`softbuffer-0.4.8/src/backends/wayland/buffer.rs:97-104`, `136-144`). If wgpu cannot initialise (no Vulkan/GL),
  the pet is drawn as an opaque rectangle. The input region still keeps clicks exact.
- **Images and canvas work.** They are ordinary iced features, and the runtime passes the app's renderer through.
  - Each surface has its own renderer (`IE/multi_window/window_manager.rs:91-95`).
  - Nearest-neighbour scaling is available: `image(handle).filter_method(image::FilterMethod::Nearest)`
    (`iced_widget-0.14.2/src/image.rs:123-126`, `iced_core-0.14.0/src/image.rs:232-238`).
  - Canvas requires the `canvas` feature, and `canvas::Program::draw` has the usual 0.14 signature
    (`iced_widget-0.14.2/src/canvas/program.rs:13-57`).
  - One gap: the `image::allocate` action only uses the first window's renderer
    (`// TODO: Shared image cache` at `IE/multi_window.rs:1338-1348`). The image widget itself is unaffected.

```rust
// CHECK/src/main.rs: nearest-neighbour sprite and canvas shadow
pin(image(self.sprite.clone())                         // image::Handle::from_rgba(26, 24, rgba)
        .filter_method(image::FilterMethod::Nearest)
        .width(PET_W as f32).height(PET_H as f32))
    .position(self.sprite_origin())

impl<Message> canvas::Program<Message> for ShadowOval {
    type State = ();
    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let center = frame.center();
        let oval = canvas::Path::new(|b| b.ellipse(canvas::path::arc::Elliptical {
            center, radii: Vector::new(bounds.width / 2.0, bounds.height / 2.0),
            rotation: Radians(0.0), start_angle: Radians(0.0), end_angle: Radians(std::f32::consts::TAU),
        }));
        frame.fill(&oval, Color::from_rgba(0.0, 0.0, 0.0, 0.22));
        vec![frame.into_geometry()]
    }
}
```

---

## 10. Known bugs, TODOs and panics that could affect us

From grepping `todo!|unimplemented!|panic!|unreachable!|expect(|unwrap()|TODO|FIXME` over both crates (no `todo!`,
`unimplemented!` or `FIXME` found), plus reading.

**Panics and crashes:**
1. **`run()` panics instead of returning `Err`** if the Wayland connection or `xdg_wm_base` is missing:
   `WindowState::build().expect("Cannot create layershell")` (`IE/multi_window.rs:139`; binds at `EV/lib.rs:2442-2467`).
2. **A missing layer-shell panics lazily.** The global is optional (`EV/lib.rs:2493`), but creating a layer surface
   does `.expect("We need layershell here")` (`EV/lib.rs:2523`, `2591`, `2887`, `3170`). On GNOME/Mutter that means a
   crash on `NewLayerShell`. **Probe `zwlr_layer_shell_v1` (v3+), `xdg_wm_base` and `wp_viewporter` before choosing
   this backend**, and hand the probed connection to `Settings::with_connection`. See `CHECK/src/bin/snippets.rs`
   (`mod probe`).
3. **A missing `wp_viewporter` panics** at the first surface (`IE/multi_window/state.rs:65-68`).
4. **A GPU `OutOfMemory` on present panics** (`IE/multi_window.rs:671-674`). `Outdated` and `Lost` are recovered
   (`IE/multi_window.rs:675-695`).
5. **Minor `unreachable!`s** on unknown protocol enum values: `IE/event.rs:37` (`KeyState`), `IE/event.rs:146`
   (`ButtonState`), `EV/seat.rs:238` (keymap format), `EV/lib.rs:2384` (session lock only). Unlikely in practice.
6. **Other panics we can avoid:** `VirtualKeyboardPressed` without a virtual keyboard (`IE/multi_window.rs:912`),
   `NewInputPanel` without the protocol (`EV/lib.rs:3460`), and `LayerSize::px(0, _)` / `PixelSize::px(0, _)`
   (`EV/size.rs:27`, `EV/size.rs:136`). Use `try_px` for runtime values.

**Lifecycle and behaviour gotchas:**
7. **The event loop exits when the last surface closes**, unless the start mode is `AllScreens` or `Background`
   (`EV/lib.rs:3550-3556`). With `Active`, a pet surface closed by the compositor (output unplugged) ends the app.
   Use `Background` and recreate the pet on `Closed`.
8. **Runtime-created layer surfaces record the *global* anchor and size** instead of their own:
   `.layout(window_state.anchor, window_state.size)` (`EV/lib.rs:3228`). This only skews `get_anchor`,
   `get_layer_size` and the exclusive-zone warning; `LayoutChange` always sends both values.
9. **Margin tuple doc bug** in the macro (section 2).
10. **Silent failures:**
    - `NewPopUp` with an unknown parent (`IE/multi_window.rs:971-977`).
    - `NewMenu` with no pointer surface (`IE/multi_window.rs:1022-1032`).
    - `PopUpReposition` on xdg_popup < v3 (`EV/lib.rs:3357-3364`).
    - `SetInputRegion` before bind (`IE/multi_window.rs:890-896`).
    - Actions for an id that never materializes are **re-queued forever** (`IE/multi_window.rs:853-861`), e.g.
      `window::close(menu_id)` after a failed `NewPopUp`. Clear ids on `ShellEvent`/close events and do not close
      unknown ids.
11. **No `CloseRequested`, no `app_id`, and an empty initial title** for xdg windows (section 6).
12. **`window::Action`s besides Close and the getters are ignored**, including Drag, Move, Resize and SetMode
    (`IE/multi_window.rs:1419`).
13. **Side mouse buttons are reported as Left** (`IE/event.rs:12-18`).
14. **`SetInputRegion` commits the surface itself** (`IE/multi_window.rs:908`) and uses the shared region
    (section 3).
15. **The tiny-skia fallback is opaque on Wayland** (section 9).
16. **One clear colour for all windows** (`style` has no id; section 6).
17. **The blocking startup:**
    - the `mundy` theme probe, up to 200 ms (`IE/multi_window.rs:159-167`)
    - the synchronous compositor creation (`IE/multi_window.rs:365-367`)
18. **TODOs:**
    - `// TODO: Shared image cache in compositor` (`IE/multi_window.rs:1342`)
    - `// TODO: Error handling (?)` for `LoadFont`, and the font channel is dropped when no compositor exists yet
      (`IE/multi_window.rs:1447-1454`)
    - `// TODO: handle callback` for blur (`EV/blur.rs:7`)
    - `// TODO: not now` for other pointer events such as gestures and frames (`EV/seat.rs:729-731`)
19. **Upstream calls popups "only a toy"** (`REPO/README.md:13`), and the CHANGELOG contradicts the source on `NewMenu`.

---

## Recommended design for the pet's Wayland shell

Full compile-checked version: `CHECK/src/main.rs`. Pieces and their verified snippets are in the sections above.

**0. Backend choice (startup).**
- If `wayland_client::Connection::connect_to_env()` succeeds and the registry offers `zwlr_layer_shell_v1` ≥ 3,
  `xdg_wm_base` and `wp_viewporter`, run the exwlshell daemon with `with_connection: Some(conn.into())`.
- Otherwise use plain iced/winit (XWayland, X11, Windows, macOS).
- This avoids the panics in section 10 items 1-3. Code: `CHECK/src/bin/snippets.rs` `mod probe`.

**1. One daemon, known ids.**
- `daemon(boot, "aipet", update, view)` with `#[to_layer_message(multi)]` and `StartMode::Background`, so there is
  no anonymous initial surface and no exit when surfaces close.
- Mint `pet_id` before building, so the `'static` `redraw_scope` closure can target it.
- A transparent `style`, a `title` for Settings, `shell_broadcast` for output info, and optionally
  `disable_clipboard()` and `default-features = false, features = ["macros"]`.

**2. The pet layer surface.**
- `boot` returns `Task::done(NewLayerShell { .. })` with these settings:
  - `anchor: Bottom | Left`, `size: LayerSize::px(SURFACE_W, bubbles + sprite + shadow)`
  - `margin: Some((0, 0, bottom, left))`, `layer: Layer::Top`, `exclusive_zone: None`
  - `keyboard_interactivity: None`, so clicking the pet never steals focus
  - `events_transparent: true`, so it is click-through from the first frame
- Chain the first `SetInputRegion` onto it; it is queued until the surface exists.
- The surface grows upwards when bubbles appear: `LayoutChange` with a new height plus `MarginChange` plus
  `SetInputRegion`, then re-send the region on `Resized`.
- On `WindowClosed(pet_id)`, recreate the surface.

**3. The input region "per frame".**
- Recompute the rectangles every frame if layout can change: alpha-mask row runs of the sprite at the current zoom,
  plus each bubble card, clamped to the surface.
- **Send `SetInputRegion` only when the set changes** (and after `Resized`). Sending it every frame works but adds
  a commit and a runtime hop per frame.
- Units are surface-local logical px, which equal iced layout px at app scale factor 1.0.

**4. Drag by margins.**
- Use the confirmed-origin algorithm from section 4: one `MarginChange` in flight, and motion ignored until the
  pet's second `RedrawRequested` after it.
- Switch the subscription to `listen_raw` only while dragging, clamp to `OutputInfo::logical_size`, and persist
  `(left, bottom)` on release.
- A press-release without movement is a click.
- If Hyprland shows jitter or offsets, fall back to the full-output overlay (section 4) and accept the full-surface
  redraw cost while animating.

**5. The context menu as an xdg popup.**
- On right-press inside the sprite, send
  `NewPopUp { IcedNewPopupSettings::new(pet_id, PixelSize::px(w, h), click, PixelSize::px(1, 1)).gravity(TopRight), id }`.
- The compositor positions it relative to the pet, flips or slides it at screen edges, and dismisses it on outside
  click (press-serial grab).
- `view(id)` renders the menu for `menu_id`; clear `menu_id` on `close_events()`, and close it with
  `window::close(id)` after an item is chosen.
- Fallback: draw it inside the pet surface, which needs a larger surface, region updates and OnDemand keyboard.
  Only use this if popup grabs misbehave on Hyprland.

**6. Settings as a base xdg window.**
- `Message::base_window_open(IcedXdgWindowSettings { size: Some(PixelSize::px(520, 420)), client_side_decorations: false })`,
  with an opaque background in its view.
- The title comes from `Daemon::title`. It has no app_id, so Hyprland rules should match the title "AiPet Settings".
- Track `settings_id` and clear it on `close_events()`. There is no close veto, so save settings on change.

**7. Animation.**
- `iced::time::every(..)` (iced feature `tokio`) at the sprite's frame rate, scoped with
  `redraw_scope(Tick => Scope::Window(pet_id))`. The loop sleeps between ticks.
- For smooth effects, a small custom widget that calls `shell.request_redraw()` avoids per-frame messages and view
  rebuilds.

**8. Monitoring and scale.**
- `ShellEvent::WindowOutputChanged` / `OutputUpdated` provide `logical_size` for clamping.
- On `Resized`, re-query `window::scale_factor(pet_id)` and adjust the sprite zoom so pixels land on whole physical
  pixels.

**Open items to verify on the running Hyprland 0.56.2 (not verifiable from source):**
- margins applied at commit
- the implicit pointer grab during a drag on a layer surface
- `xdg_popup.grab` accepted with the right-press serial
- wgpu picking an alpha-capable mode on the user's GPU
- exclusive-zone interaction: with `exclusive_zone: None` (protocol default 0), margins are measured from the area
  left by panels, so the `logical_size` clamp can be off by the panel height
