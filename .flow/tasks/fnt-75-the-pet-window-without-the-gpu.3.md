---
satisfies: [R2, R3, R10]
---
# fnt-75-the-pet-window-without-the-gpu.3 Bubbles, header, music player, tooltip and inline menu on the CPU, with cosmic-text

## Description
The rest of the window: cards (shadows, glow for attention, Failed's red), the reviews header, the music player, tooltips, the inline menu and icons, drawn into the scene's pixmap, and text measured and drawn with cosmic-text instead of iced's paragraph. Bubble sizes and line breaks must match today's.

**Size:** M
**Files:** `rust/crates/aipet-ui/src/{view,style,menu,cards,lib}.rs`, a scene text module
**Touches:** [rust/crates/aipet-ui/**]

### Approach
- One `FontSystem` and `SwashCache` in the scene, glyph masks blitted (pattern: iced_tiny_skia's text drawing); fonts as `style.rs:16-29`.
- Move `style::fit`/`text_width`, `fit_card`, `tip_size` (`lib.rs:1412,1465`) onto it; pin with a test listing today's widths and wraps for a set of strings at scales 1 and 2, recorded from the iced path before switching.
- Icons: reuse the CPU pictures in `style.rs:86-269`.
- Hit and drawn rects keep coming from the layout, as today.
- Decide one cosmic-text version for the workspace (the spike used 0.19, iced pins 0.15): the pet uses its own; Settings keeps iced's.

### Investigation targets
**Required:**
- `rust/crates/aipet-ui/src/view.rs:33-265` - layer order and card drawing
- `rust/crates/aipet-ui/src/style.rs:6-84` - measuring and fitting
- `rust/crates/aipet-ui/src/menu.rs:17` - the inline menu

### Key context
- `SwashCache`'s image cache is unbounded: keep one per scene, a few sizes.
- The bubble stack's fold/spread dropped frames on iced (fn-1.28 open item): check it does not here.

## Acceptance
- [ ] Cards, header, music player, tooltip and inline menu draw into the pixmap with their actions and hit rects
- [ ] The width and wrap test matches the recorded iced figures at scale 1 and 2
- [ ] aipet-ui tests (layout, hit rects, actions) pass on the new drawing

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
