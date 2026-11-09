# UI framework spike: gpui or something simpler

## Goal & Context

The pet's UI is small: one transparent, click-through, always-on-top window that draws a sprite, a few chat bubbles and a board, plus a Settings window of forms. Today it runs on iced 0.14 with wgpu, and on Wayland compositors with layer-shell, iced_exwlshell. iced's software renderer can't draw a transparent window, so the pet needs a GPU context. The fn-1.29 memory audit found GPU drivers and GL libraries to be most of the pet's memory (222 MB of a 300 MB RSS before the cuts). The owner asked (2026-11-09) for a spike on gpui, or a simpler framework, since the pet's UI is simple. This spec only answers the question. A migration, if the spike recommends one, gets its own spec.

## Acceptance Criteria

- **R1:** A proof file compares at least these candidates against today's iced stack, on the same criteria:
  - gpui;
  - a minimal stack with no UI framework for the pet window (a window, a CPU-drawn buffer, the existing tiny-skia drawing, and native code for layer-shell and click-through);
  - one mainstream retained toolkit (Slint or egui).

  The criteria:
  - a transparent window;
  - click-through on the art only (per-pixel hit mask);
  - always-on-top without window rules, on Windows, X11 and Wayland layer-shell (Hyprland);
  - crisp pixel scaling;
  - text for bubbles;
  - a second window for Settings;
  - calm CPU and RSS, measured with fn-1's method on Linux;
  - binary size;
  - build needs on Windows (Git Bash, MSVC);
  - licence and maintenance.

  Errors: a candidate that can't be built or run on a platform is recorded with the reason, never dropped silently.
- **R2:** Each candidate gets a throwaway prototype that shows the pet window: the sprite of one built-in pack, one bubble, click-through on the art, and dragging. The prototypes run on Linux (Hyprland); Windows is tried with the owner present. Errors: no error surface beyond R1's recorded failures.
- **R3:** The proof ends with a recommendation. The options are keep iced, move the pet window only (Settings stays where it is), or move both. Each comes with its cost in tasks and what it would change in fn-1's budgets (R18) and in the platform code. Errors: no error surface (findings only).

## Early proof point

Task fnt-73-ui-framework-spike-gpui-or-something.1 is the whole spec. If no candidate can do a transparent click-through window on Hyprland's layer-shell, the recommendation is to keep iced, and the spike says why.

## Boundaries

- No migration: prototypes live on a scratch branch and are never merged.
- No changes to the pet's crates.
- Only the candidates in R1, plus any the spike finds clearly better, each in one line of its own.

## Decision Context

The owner asked for gpui by name and for "a simpler framework since the UI is relatively simple". The minimal stack is in the list because the pet already draws its sprites with tiny-skia, so a framework would mostly provide windows and text. Measuring with fn-1's method keeps the numbers comparable with fn-1.28 and fn-1.29.

## Quick commands

```bash
scripts/perf/pet-calm.sh <label> Sprout -- <prototype binary>
```

## Requirement coverage

| Req | Description | Task(s) | Gap justification |
| --- | --- | --- | --- |
| R1 | A proof file compares at least these candidates against today's iced stack, on the same criteria: | fnt-73-ui-framework-spike-gpui-or-something.1 | — |
| R2 | Each candidate gets a throwaway prototype that shows the pet window: the sprite of one built-in pack, one bubble, click-through on the art, and dragging. The prototypes run on Linux (Hyprland); Windows is tried with the owner present. Errors: no error surface beyond R1's recorded failures. | fnt-73-ui-framework-spike-gpui-or-something.1 | — |
| R3 | The proof ends with a recommendation. The options are keep iced, move the pet window only (Settings stays where it is), or move both. Each comes with its cost in tasks and what it would change in fn-1's budgets (R18) and in the platform code. Errors: no error surface (findings only). | fnt-73-ui-framework-spike-gpui-or-something.1 | — |
