# Session wheel acceptance

The configured agent shortcut opens a session wheel before capture. With F12 configured, the first press only displays the wheel and selects. The second press starts capture for the remembered guarded recipient, and a later press stops it through the existing cancellation and delivery flow. The product default remains F8.

## Names

Each session is named by its agent's own title, shown whole:

- **Claude Code:** the custom or generated session title Claude Code puts in the terminal title, with its activity glyph stripped by Herdr. A brand-new session titled only "Claude Code" has no name yet.
- **Codex:** the thread name in Codex's own `session_index.jsonl`, matched to the session ID Herdr reports for that pane. The latest entry wins, so renames show at once.

Codex's terminal title is not parsed: its layout is configurable, and before a thread is named it shows only the project (`project` in the real-agent run below).

A session without a title shows its Herdr agent name, then its pane ID. This is a genuine fallback, not a parse failure. Mluva never renames a session and never shortens a title to fit; only metadata beyond 240 characters is bounded. Herdr's agent name stays the routing handle for `mluva herdr select`, and `mluva herdr list` reports `title` beside it. The previous wheel showed Herdr's agent name, which for dispatched agents is a handle such as `cos` or `linear-…`, and cut names to two lines with an ellipsis.

## Overlay

On Omarchy the wheel floats over the desktop like Omarchy's own launcher, with no window, frame or card. It is a wlr-layer-shell overlay that covers the focused monitor, panels included, and takes the keyboard while it is shown.

The desktop shows through. Between it and the dial lies one flat translucent wash: the theme's canvas pulled 60% toward black (toward white on a light theme), like a terminal's background, at 72%. It has no gradient, so nothing can band, and there is no disc, panel or vignette. About 28% of the desktop's contrast survives it, against 12% for a canvas-grey wash at 88%, the strength needed for the same legibility without the darker colour ([sweep](wash-sweep.jpg)). Each glyph of the wheel carries a soft halo in the wash's colour, so a name stays readable over terminal text or a bright page behind it while the desktop around it stays visible. Mluva's managed Hyprland Lua gives the `mluva-session-wheel` namespace a `no_anim` layer rule, as Omarchy does for its own menus, so the wheel appears with no compositor fade of its own; the dial then fades in over at most 160 ms. The rule is kept as one handle across reloads: a live rule is reused, a switched-off one is switched on and a rule the compositor dropped is made again, so reloads never stack duplicates. It runs under `pcall` after the bindings, so a Hyprland without layer rules, or any error in the rule, never stops the bindings from applying. Clicking anywhere outside a control dismisses the overlay, like Escape.

On X11, or on compositors without layer-shell, the same wheel opens as an ordinary frameless dialog whose ground is the same wash colour, opaque, since a window there cannot be relied on to show the desktop through. The offscreen tests use that path.

The overlay links `gtk4-layer-shell`, which is part of Omarchy's base install. It is linked rather than loaded later because it must load before `libwayland-client`.

## The dial

The dial is drawn live in a few thin lines with GtkSnapshot; nothing is baked, so there is no art in the binary. Colour comes from the active theme's canvas, ink, action, warning and success tokens.

- **Ring.** One 1 px hairline circle in the ink at 24%.
- **Ticks.** One 12 px tick per session inside the ring, in the ink at 72%: amber when blocked or waiting, with a small green dot when working, hollow when draft only, and a faint point when the session is gone. The hovered or focused session's tick brightens and lengthens.
- **Chosen tick.** Longer (20 px), wider and in the light's colour: the accent mixed toward the ink, or the warning colour when the choice is blocked or missing.
- **Hand.** A thin line in the light's colour from just outside the hub text to 14 px short of the chosen tick, so it never runs through the words and never merges with the tick. It turns on a damped spring (see Motion) and takes the short way round.
- **Light.** One trace along the ring at the hand's bearing: a fine bright arc over a soft halo on a dark wash, a crisp arc on a light one. It is drawn at the hand itself, so it rides the hand through a swing and settles with it; while the hand turns, a tail trails behind it like a comet's and the light dims a little, then brightens softly as the hand arrives. Its reach ahead narrows with the count, to 0.55 of the tick spacing and at most 34°, so at twenty sessions it lights only the chosen tick and is almost dark halfway to a neighbour ([twice actual size](after-trace-twenty.png)). It turns amber for a blocked or missing choice. It is the only ornament.
- **Hub.** At the centre, one short line saying what happens to the words once the agent key has recorded them: `→ send`, `→ queue`, `→ prefill` or `→ draft only`; or why Record is withdrawn: `blocked` or `gone` in amber, `choose a session`, `no sessions`. The centre never repeats the chosen name.
- **Status line.** One quiet line above the legend explains a level, a draft or a page: "Arrives as a message from another Claude session", "This session has no inbox", "Chosen on page 2". It always holds one line, so a new choice moves nothing.
- **Legend.** `F12 record   F9 transcript only   Esc cancel`, with the pager beyond 26 sessions. Record dims while it is withdrawn.

F12 always records, so the key and "record" appear once, in the legend; the hub says only what then happens to the words. The previous wheel printed `F12 SEND` on the dial while the legend said `F12 Record`.

Per frame the stage draws the wash, the ring, the trace, the ticks, the chosen tick and the hand: a few strokes and one small blur clipped to the lit arc. Each label's halo is two text shadows.

## Motion

Every movement is paint and transform only: no name's allocation moves and nothing reflows. Nothing waits on it: keys, clicks, Record, paging and Escape act on the first frame, and a new choice mid-swing turns from wherever the hand is, at its present speed. With GTK animations disabled everything is placed at rest.

- **Opening, 0.30 s.** The wash, the hub and the legend fade in over 120 ms. The ring draws itself clockwise from one o'clock over 220 ms on a cubic deceleration, so the draw itself can be seen. Each tick fades in over 50 ms as the draw passes it, and each name settles in from 10 px further out over 80 ms on the same sweep. The hand springs from one o'clock to the remembered session meanwhile, the trace riding it.
- **Hand spring, tuned to the hop.** The overshoot is 7% of the hop, between 0.5° and 3°, so it is felt on a short hop and never showy across the dial; the damping ratio follows from it (0.65 for a neighbour, 0.79 across), so there is one overshoot and no second swing. The hand peaks 0.11 s plus 0.8 ms per degree after the choice: an 18° hop peaks at 124 ms, overshoots 1.1° and is still by 183 ms; a 170° hop peaks at 246 ms, overshoots 2.5° and is still by 383 ms.
- **Retarget.** A new choice keeps the hand's place and speed and retunes the spring for what is left, so a quick second letter bends the swing instead of restarting it.
- **Light.** The trace's tail reaches as far behind the hand as it turns in 45 ms, at most 40°. While the hand turns faster than 60° a second the light dims to 70% (time constant 40 ms) and it brightens back as the hand settles (time constant 100 ms).
- **Amber.** A choice that is or becomes blocked or missing eases the light, the hand and the chosen tick to amber with a 90 ms time constant, not a flash.
- **Hover and focus.** The tick of the name under the pointer or keyboard focus lifts, 4 px longer, wider and brighter, with a 50 ms time constant.
- **Paging.** The new page's ticks and names turn 8° into place, clockwise for the next page and back for the previous, and fade in over 180 ms.
- **Names.** The old choice's name eases down and the new one's up through a 160 ms colour transition; the bold weight changes at once, without changing any width.

The opening and a swing to the opposite side at fixed timestamps, over the busy desktop:

![The opening with twenty sessions at 16, 50, 83, 116, 166, 216, 300 and 450 ms](motion-open.jpg)

![A hop across the dial with the trace riding the hand, at the same timestamps](motion-swing.jpg)

The whole reel as it plays, from the production renderer on a stepped 60 fps clock ([GIF](motion.gif)): opening with 6 and 20 sessions, a short hop, a hop across the dial, a new choice mid-swing, a blocked choice and a page turn at 30 sessions.

Frame cost on the cairo renderer the private desktop uses, from a release build of the gallery: each frame snapshots the whole 1920×1200 window and renders it to a texture. That is a worst case, since on screen GTK repaints only what changed; the budget at 60 fps is 16.7 ms.

| Scene | Frames | Median ms | 95th percentile ms | Slowest ms |
| --- | --- | --- | --- | --- |
| Opening, 6 sessions | 45 | 7.9 | 8.6 | 9.3 |
| Opening, 20 sessions | 45 | 11.1 | 13.5 | 19.3 |
| Short hop | 30 | 11.2 | 12.6 | 14.1 |
| Hop across the dial | 40 | 11.6 | 14.4 | 15.1 |
| New choice mid-swing | 40 | 12.0 | 14.3 | 14.4 |
| Blocked choice | 45 | 12.3 | 15.9 | 16.9 |
| Opening, 30 sessions | 36 | 12.9 | 13.9 | 14.7 |
| Page turn | 30 | 4.4 | 5.0 | 5.0 |

The first measurement took 25–60 ms a frame. Three changes brought it under budget: the trace's blur is clipped to the lit arc instead of the ring's whole square (about 28 ms at twenty sessions), each label's halo is two shadows instead of five (about 20 ms) and the opening ring is drawn as arcs rather than a masked circle.

## Layout

Sessions follow Herdr's order: workspace, tab, then pane layout, with left or top first. The first session, the leftmost pane, sits at one o'clock; the rest follow clockwise at even angles.

Every full name stands just outside the ring on its own tick's bearing:

- A name hangs off a point 24 px beyond the ring along its bearing: wholly to the right on the right half and wholly to the left on the left half, even beside the poles, so every key sits at its own tick; and above or below near the top and bottom. It never touches the ring.
- Names that would touch are pushed apart toward their pole, the one nearest three or nine o'clock keeping its place. There are no leader lines, boxes or key caps.
- Everything is JetBrains Mono. The uppercase letter key faces the dial, then the name. A tag (pane, or product where different agents share a name) and a state caption (`working`, `blocked`, `waiting`, `queue`, `prefill`, `draft only`, `gone`) flow after the name's last word, smaller and quieter, the caption amber when Record cannot reach the session. Names wrap only at spaces, or inside a single long token; they are never ellipsized.
- Names rest slightly dimmed, and a session Record cannot reach rests dimmer. Hover and keyboard focus brighten the name, light its key and brighten its tick; nothing is boxed or ringed. The chosen name is bold with its key in the light's colour.

The ring grows with the count, from 150 px for one session to 290 px for twenty, and shrinks, never the names, when the screen is smaller. Geometry depends only on the count and on the names: every name is measured in its bold type with the longest caption after it, so a refresh that changes a status, or a new choice, moves nothing; and since lines break greedily, a caption never moves a word of the name. A refresh in which no Herdr server answers is a failed read, not an empty list: while the wheel is open it keeps the sessions it shows and its page, and sending stays guarded.

Letters choose in clockwise order, home row first, and display uppercase: A S D F G H J K L, then Q W E R T Y U I O P, then Z X C V B N M. Typing either case chooses. Up to 26 sessions show together; Left/Right page beyond that, and Escape cancels. A click on a tick, or within 12 px of it, chooses it and a click elsewhere inside the ring records.

## Before and after

From the production renderer offscreen at 1920×1200 on the Nord reference theme. The before captures are the sundial watch at `f7dd77b`, whose room was opaque. The after captures keep the overlay's alpha and are composited over a synthetic busy desktop: a code terminal, a test-output terminal and a bright documentation page, all invented.

| State | Before | After |
| --- | --- | --- |
| One | ![Before: one session](before-one.jpg) | ![After: one session](after-one.jpg) |
| Six, with duplicates and a working session | ![Before: six sessions](before-six.jpg) | ![After: six sessions](after-six.jpg) |
| Twenty sessions, one long legacy name chosen | ![Before: twenty sessions](before-twenty.jpg) | ![After: twenty sessions](after-twenty.jpg) |
| Blocked choice | ![Before: blocked choice](before-blocked.jpg) | ![After: blocked choice](after-blocked.jpg) |
| Remembered session missing | ![Before: missing session](before-missing.jpg) | ![After: missing session](after-missing.jpg) |
| Every delivery level, draft only chosen | ![Before: draft only](before-draft.jpg) | ![After: draft only](after-draft.jpg) |
| 30 sessions, the remembered one on page 2 | ![Before: paged](before-paged.jpg) | ![After: paged](after-paged.jpg) |
| Keyboard focus on a choice | ![Before: keyboard focus](before-focus.jpg) | ![After: keyboard focus](after-focus.jpg) |
| Pointer hover on a choice | ![Before: hover](before-hover.jpg) | ![After: hover](after-hover.jpg) |
| Default light palette, twenty | ![Before: light palette](before-light.jpg) | ![After: light palette](after-light.jpg) |
| The hand swinging to a new choice, about 60 ms in | ![Before: mid-swing](before-swing.jpg) | ![After: mid-swing, the trace riding the hand](after-swing.jpg) |

More captures of the after state: [default dark palette, blocked choice](after-dark-blocked.jpg), [untitled sessions falling back to Herdr name or pane](after-untitled.jpg) and [no selection](after-none.jpg).

The window fallback on X11, at its natural size:

| Six sessions | Twenty sessions |
| --- | --- |
| ![Six sessions in the window fallback](window-nord-six.jpg) | ![Twenty sessions in the window fallback](window-nord-twenty.jpg) |

## Visual issues fixed

| Issue | Now |
| --- | --- |
| An opaque room hid the desktop | One flat wash lets about 28% of it through |
| A knurled steel case, bezel, sunray dial and crystal, 2.5 MB of baked Blender layers and a 544 KB `.blend` | A ring, ticks, a hand and one light, drawn live; `resources/wheel/` is gone |
| Names floated far from their indices (about 250 px for a left-hand name at six sessions) | Names hang 24 px beyond the ring on their own bearing, keys at their ticks |
| Sans names with mono keys | JetBrains Mono throughout |
| The hand ran through `F12 RECORD` whenever the choice was in the lower half | The hub sits at the centre and the hand starts on an ellipse around it |
| `F12 SEND` on the dial against `F12 Record` in the legend | `→ send` on the hub, `F12 record` in the legend |
| Faint 12 px letter-spaced dial printing | 16 px mono in full ink |
| Delivery hints wrapped to three lines inside the dial | One fixed status line above the legend |
| The chosen name grew, a size jump | It turns bold; mono bold keeps the same width |
| Captions on a reserved second line doubled every name's height, so names near the poles stacked far from their ticks | The caption flows after the name's last word; boxes are one line |
| Names beside the poles were centred over their anchor, so two keys could sit between two ticks | Names hang wholly to their side; each key sits at its tick |
| A light on a spring of its own lagged the hand mid-swing | The trace is drawn at the hand and settles with it |

The eleven artifacts Daniel confirmed for the wheel before the watch stay fixed: no desktop text competes with a name (the wash and halos), no banding (one flat wash), no accent arc, no box on the chosen name, on hover or around keys, no leader lines, no focus rings, no long name repeated in the centre, the `no_anim` rule, and geometry that never moves with status or choice.

## Evidence

| Boundary | Evidence |
| --- | --- |
| Native gate | `make linux-test` with `TMPDIR` on disk passed: 311 passed, zero failed, 72 gated or ignored. Both strict Clippy configurations, formatting and generated-feature consistency passed. Unit tests cover, besides the earlier geometry tests: the hand starting clear of the hub text at every bearing; the trace reaching at most 0.55 of the tick spacing and 34°, almost dark halfway to a neighbour, for 1–26 sessions, and its tail only while the hand turns; one small overshoot and no second swing on a short hop and across the dial; a new choice mid-swing turning on from where the hand is, at its speed, without a jump; the opening drawing clockwise from one o'clock and bringing every tick and name in within 0.3 s; the light dimming in travel and brightening on arrival; amber easing in; a page turn and a hover lift finishing; everything placed at rest with animations off; and the wash deepening the canvas away from the ink. |
| Real application | `make linux-agent-voice-test` passed: 20 captures, 18 recognitions, five guarded submissions, two queued through Claude Code's inbox, a Codex send-if-idle and a busy Codex thread keeping its draft. Private X11 keys exercise selection, Escape, second F12, F9, paging at 30 sessions, server reorder and target disappearance, and the wheel fits the private screen from zero to 20 sessions. The four known narrow-editor allocation warnings remain and none comes from the wheel. |
| Shortcut portal | Not rerun: this change touches neither the shortcuts nor the Hyprland Lua. |
| Visual states | The `session_wheel_gallery` example renders the production wheel in 30 synthetic captures per palette (one, three, six, 20 and 30 sessions; long, duplicate, Czech, CJK and emoji names; blocked, missing, draft-only and untitled sessions; every delivery level; no selection; empty; hover; keyboard focus on a choice, on Record and on Cancel; the hand in flight and at rest), as an overlay that keeps its alpha or as the window fallback. It audits itself and fails if a name leaves the screen, two names overlap, or three status changes plus a new choice on twenty sessions move any name or any key; a build with the name not taking the caption's room fails that audit, as it should. Nord, default dark and default light overlays and the Nord and light window fallback passed: layout independent of status and choice. With `motion` as its last argument it renders the motion reel frame by frame on a stepped 60 fps clock and times each frame. |
| Visual review | Daniel rejected the watch as far too complex and asked for a minimal, terminal-like, see-through dial that keeps a lighting quality. Three light treatments (a glow on the hand and chosen tick, a trace along the ring, a phosphor wedge) were compared at 1, 6 and 20 sessions over the busy desktop; Daniel picked the trace. An impeccable critique of every state and palette followed: an assessment of design specificity, Nielsen's heuristics, cognitive load and two personas (26/40, judged authored for this product rather than interchangeable) and the bundled detector (no findings in the stylesheet). Applied: keys pinned at their ticks whatever caption follows a name (a real regression, now audited by the gallery), a reason and a next step for blocked and gone, higher text contrast and halos, the legend under the names, a focus underline and tick lift, a crisp trace on light themes, a 16 px hub, amber for `waiting` and a wider hollow draft tick. Kept as they are, with the brief: positional letters, a ring rather than a list above about twelve sessions, and announcements for screen readers. |
| Not verified live | The dial has not been shown on the live compositor: no live layer-shell run, no live F12 path and no check of the wash or halos over Daniel's real desktop. The Lenovo's 3840×2400 screen at scale 2 is not measured. |

Render the gallery inside the private desktop at the screen being judged:

```sh
cargo build --locked -p mluva-gtk --example session_wheel_gallery
OFFSCREEN_SCREEN_SPEC=1920x1200x24 bwrap --die-with-parent --bind / / --dev /dev --tmpfs /run/dbus -- \
  bash dev/run-isolated-browser.sh "$PWD/tmp/wheel-gallery" -- \
  "$CARGO_TARGET_DIR/debug/examples/session_wheel_gallery" nord "$PWD/tmp/wheel-gallery/png" overlay
```

Drop `overlay` for the window fallback. Overlay captures keep their alpha; composite them over a desktop to judge them. The gallery prints whether the layout was independent of status and choice and exits with a failure on any audit problem.

The [machine-readable result](results.json) contains only synthetic acceptance data; session names, panes and sockets are fixtures. The checks ran in a disposable X11 desktop with private HOME/XDG, D-Bus, accessibility, clipboard, audio fixtures and loopback providers. They do not establish physical F12/F9 input, a real microphone or provider, or the full live Wayland journey.
