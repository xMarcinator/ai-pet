-- AiPet's rules for Hyprland 0.55 and later (the Lua config). The installers put this file in
-- ~/.local/share/AiPet/hyprland/; include it from ~/.config/hypr/hyprland.lua with:
--
--   pcall(dofile, os.getenv("HOME") .. "/.local/share/AiPet/hyprland/aipet.lua")
--
-- (pcall: once AiPet is uninstalled, the missing file is skipped instead of breaking the config.)
-- For hyprland.conf (Hyprland 0.54 and earlier) use aipet.conf next to this file.
--
-- The pet is a layer surface in the "aipet" namespace. It moves itself while it's dragged, and Hyprland's layers
-- animation would make each move glide behind the pointer, so the rule turns that off. Layers aren't blurred unless a
-- rule asks for it, so the pet needs nothing else.
--
-- Settings draws its own frame and shadow on a transparent window, so Hyprland's border, rounding, blur and shadow
-- would frame the whole transparent rectangle. It's matched by its title, which the pet sets on every backend.

hl.layer_rule({
    name = "aipet-pet",
    match = { namespace = "^aipet$" },
    no_anim = true,
})

hl.window_rule({
    name = "aipet-settings",
    match = { title = "^AiPet · Settings$" },
    border_size = 0, rounding = 0,
    no_blur = true, no_shadow = true,
})
