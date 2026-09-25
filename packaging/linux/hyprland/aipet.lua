-- AiPet's window rules for Hyprland 0.55 and later (the Lua config). The installers put this file in
-- ~/.local/share/AiPet/hyprland/; include it from ~/.config/hypr/hyprland.lua with:
--
--   pcall(dofile, os.getenv("HOME") .. "/.local/share/AiPet/hyprland/aipet.lua")
--
-- (pcall: once AiPet is uninstalled, the missing file is skipped instead of breaking the config.)
-- For hyprland.conf (Hyprland 0.54 and earlier) use aipet.conf next to this file.
--
-- The pet and its Settings draw their own frames and shadows on transparent windows, so Hyprland's border, rounding,
-- blur and shadow would frame the whole transparent rectangle. The pet moves itself while it's dragged, and an
-- animated move makes the drag overshoot. Pinned, it stays on its monitor whichever workspace is shown.
-- Don't add no_focus: the pet then gets no mouse input at all.

hl.window_rule({
    name = "aipet-pet",
    match = { class = "^AiPet$", title = "^AiPet$" },
    float = true, pin = true,
    border_size = 0, rounding = 0,
    no_blur = true, no_shadow = true, no_anim = true,
})

hl.window_rule({
    name = "aipet-settings",
    match = { class = "^AiPet$", title = "^AiPet · Settings$" },
    border_size = 0, rounding = 0,
    no_blur = true, no_shadow = true,
})
