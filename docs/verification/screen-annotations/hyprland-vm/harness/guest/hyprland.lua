-- Minimal synthetic test session: Hyprland defaults without animations or wallpaper.
hl.monitor({ output = "", mode = "preferred", position = "auto", scale = 1 })
hl.env("XCURSOR_SIZE", "24")
hl.config({
  general = { gaps_in = 4, gaps_out = 8, border_size = 2, layout = "dwindle" },
  decoration = { rounding = 0, shadow = { enabled = false }, blur = { enabled = false } },
  animations = { enabled = false },
  misc = { force_default_wallpaper = 0, disable_hyprland_logo = true },
  ecosystem = { no_donation_nag = true, no_update_news = true },
  input = { kb_layout = "us,cz", follow_mouse = 1 },
})
-- The same window drag as a typical desktop: SUPER + left button.
hl.bind("SUPER + mouse:272", hl.dsp.window.drag(), { mouse = true })
hl.bind("SUPER + mouse:273", hl.dsp.window.resize(), { mouse = true })
hl.on("hyprland.start", function()
  hl.exec_cmd("dbus-update-activation-environment --systemd WAYLAND_DISPLAY XDG_CURRENT_DESKTOP HYPRLAND_INSTANCE_SIGNATURE XDG_SESSION_TYPE")
end)
-- Mluva's opt-in managed portal bindings, as documented in docs/omarchy-integration.md.
local mluva_keys = (os.getenv("XDG_CONFIG_HOME") or (os.getenv("HOME") .. "/.config")) .. "/mluva/hyprland-shortcuts.lua"
local file = io.open(mluva_keys, "r")
if file then file:close(); dofile(mluva_keys) end
-- Test layout: Mluva's window elsewhere; the synthetic settings page floats over the tiled windows.
hl.window_rule({ name = "mluva-away", match = { class = "^com\\.mluva\\.Linux$" }, workspace = "2 silent" })
hl.window_rule({ name = "settings-float", match = { class = "^synthetic\\.settings$" },
  float = true, size = "700 760", move = "700 150" })
-- Optional output layout for multi-monitor runs.
local monitors = os.getenv("HOME") .. "/.config/hypr/monitors.lua"
local layout = io.open(monitors, "r")
if layout then layout:close(); dofile(monitors) end
