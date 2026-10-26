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

The desktop shows through. Between it and the dial lies one flat translucent wash: the theme's canvas pulled 60% toward black (toward white on a light theme), like a terminal's background, at 72%. It has no gradient, so nothing can band, and there is no disc, panel or vignette. About 28% of the desktop's contrast survives it, against 12% for a canvas-grey wash at 88%, the strength needed for the same legibility without the darker colour ([sweep](wash-sweep.jpg)). Under every line of text the overlay clears the cells, as a terminal overwrites the cells it prints in: the wash's colour at 0.9, about a cell beyond the text sideways and 4 px above and below, feathered over 8 px, with no radius or stroke. A key and the first line of its name are cleared as one span. Over the empty wash the cleared cells cannot be seen, so they are no panel; over the desktop they keep terminal text or a bright page from reading as part of a name, while the desktop between the names stays visible. Mluva's managed Hyprland Lua gives the `mluva-session-wheel` namespace a `no_anim` layer rule, as Omarchy does for its own menus, so the wheel appears with no compositor fade of its own; the dial then fades in over at most 160 ms. The rule is kept as one handle across reloads: a live rule is reused, a switched-off one is switched on and a rule the compositor dropped is made again, so reloads never stack duplicates. It runs under `pcall` after the bindings, so a Hyprland without layer rules, or any error in the rule, never stops the bindings from applying. Clicking anywhere outside a control dismisses the overlay, like Escape.

On X11, or on compositors without layer-shell, the same wheel opens as an ordinary frameless dialog whose ground is the same wash colour, opaque, since a window there cannot be relied on to show the desktop through. The offscreen tests use that path.

The overlay links `gtk4-layer-shell`, which is part of Omarchy's base install. It is linked rather than loaded later because it must load before `libwayland-client`.

## The dial

The dial is drawn live in a few thin lines with GtkSnapshot; nothing is baked, so there is no art in the binary. Colour comes from the active theme's canvas, ink, action, warning and success tokens.

- **Ring.** One 1 px hairline circle in the ink at 24%.
- **Ticks.** One 12 px tick per session inside the ring, in the ink at 72%: amber when blocked or waiting, with a small green dot when working, hollow when draft only, and a faint point when the session is gone. The tick of the hovered or focused name lifts: longer, wider and brighter.
- **Chosen tick.** Grows from its tick: longer (20 px), wider and in the light's colour, the accent mixed toward the ink, or amber when the choice is blocked or missing. The tick that stops being chosen shrinks back the same way, at the same time.
- **Hand.** A thin line from just outside the hub text to 14 px short of the chosen tick, so it never runs through the words and never merges with the tick: in the light's colour on a dark wash, neutral ink on a light one, where a coloured hand would read as a second light. It turns on a critically damped spring (see Motion) and takes the short way round.
- **Light.** One trace along the ring at the hand's bearing, drawn at the hand itself, so it rides the hand through a swing and settles with it. On a dark wash a near-white 2 px core lies over a 10 px halo in the light's colour, so it reads as light over a bright page too, about 5:1 against the washed docs page at its brightest; on a light wash the core is the saturated accent over a faint bloom of it, coloured light on paper. The bright core reaches 0.55 of the tick spacing, at most 16°, so at twenty sessions it lights only the chosen tick ([twice actual size](after-trace-twenty.png)); a low fade of at most 0.3 reaches at least 12° and at most 34° either side, so it reads as light at any count, and never more than 0.3 on a neighbour. While the hand turns, a tail trails behind it like a comet's. It turns amber for a blocked or missing choice. It is the only ornament.
- **Hub.** At the centre, the agent key in the light's colour and what then happens to the words: `F12 → send`, `F12 → queue`, `F12 → prefill` or `F12 → draft only`; or, without the key, why Record is withdrawn: `blocked` or `gone` in amber, `choose a session`, `no sessions`. The centre never repeats the chosen name.
- **Status line.** One quiet line just under the names explains a level, a draft, a page or a withdrawn choice: "Arrives as a message from another Claude session", "This session has no inbox", "Chosen on page 2", "Answer its prompt where it runs, or choose another session", "Mluva can't find it now; choose another session". It always holds one line, so a new choice moves nothing.
- **Legend.** `F12 record   F9 transcript only   Esc cancel`, under the status line, with the pager beyond 26 sessions. Record dims while it is withdrawn. The hub and the legend both show F12: the hub says what pressing it will do; the legend's Record is the real button, the one the pointer and a screen reader act through.

Per frame the stage draws the wash, the cleared cells (baked once into a texture and reused until a name moves or focus changes), the ring, the trace, the ticks and the hand: a few strokes and one small blur clipped to the lit arc.

## Motion

Every movement is paint and transform only: no name's allocation moves and nothing reflows. Nothing waits on it: keys, clicks, Record, paging and Escape act on the first frame, and a new choice mid-swing turns from wherever the hand is, at its present speed. Nothing snaps and nothing bounces: every value that changes with the choice, hover, focus, page or state eases, and none passes its target. Arrivals use the strong ease-out `cubic-bezier(0.23, 1, 0.32, 1)`, solved exactly. With GTK animations disabled everything is placed at rest.

- **Opening, 0.30 s.** The wash, the hub and the legend fade in over 120 ms. The ring draws itself clockwise from one o'clock over 200 ms. Each tick fades in over 80 ms as the draw reaches it, and each name settles in from 8 px further out over 100 ms on the same sweep, so the dial arrives in reading order. The hand springs from one o'clock to the remembered session meanwhile, the trace riding it, and the chosen tick grows as it arrives.
- **Hand spring.** Critically damped, stepped with its exact solution, so the hand arrives at its tick without passing it at any length of hop. Its natural frequency falls from 36 rad/s for a neighbour to 26 rad/s across the dial: an 18° hop is still by about 0.18 s and a 170° hop by about 0.33 s, 99% of the way by 0.25 s ([hand bearing against time](hand-angle.png)).
- **Retarget.** A new choice keeps the hand's place and speed and retunes the spring for what is left, so a quick second letter bends the swing instead of restarting it. A hand already rushing toward the new tick is held firmer, just enough that it still arrives without passing it; a hand that has passed the new tick carries on a few degrees with its own speed, turns and lands on it from the side it approaches.
- **Chosen tick.** The new choice's tick grows and the old one's shrinks back together, each easing over about 0.2 s (time constant 60 ms).
- **Light.** The trace's tail reaches as far behind the hand as it turns in 45 ms, at most 40°. While the hand turns faster than 60° a second the light dims to 75% (time constant 40 ms) and it brightens back as the hand settles (time constant 80 ms): the soft landing.
- **Amber.** A choice that is or becomes blocked or missing eases the light, the hand and the chosen tick to amber (time constant 80 ms), not a flash.
- **Hover and focus.** The tick of the name under the pointer or keyboard focus lifts, 4 px longer, wider and brighter (time constant 50 ms). Focus's reverse-video key cell eases in and out over 180 ms, and its underline fades in with the lift; it leaves at once when focus moves on.
- **Paging.** The new page's ticks and names turn 8° into place, clockwise for the next page and back for the previous, and fade in over 180 ms.
- **Names and text.** A name brightens or dims through a 180 ms colour transition on the same ease-out; the chosen name keeps its weight, since a change of weight cannot ease, and is carried by brightness and its key's colour. The hub and the status line cross-fade over 160 ms at a fixed size, so new text never moves them. Record eases its dimming the same way.

The motion was checked against Emil Kowalski's animation standards:

| Standard | Before | Now |
| --- | --- | --- |
| No bounce in UI motion | The hand overshot its tick by 1–2.5° and settled back | Critically damped: it never passes its tick, on any hop or retarget |
| Retarget from the current state | Kept, but could still overshoot after a retarget | Keeps place and speed, and holds a rushing hand firm enough not to pass the new tick |
| Nothing jarring: no instant swaps | The chosen tick, the bold weight, the dimmed name and the hub text snapped | The tick grows while the old one shrinks; the name eases by colour with no weight change; the hub and status line cross-fade; Record eases its dimming |
| Exact strong curves, no approximations | An approximated quartic ease-out | `cubic-bezier(0.23, 1, 0.32, 1)` solved exactly for arrivals |
| Under 300 ms | The opening 0.30 s, a neighbour 0.18 s | Unchanged; a hop across the dial is 99% there at 0.25 s and still at about 0.33 s, as distance scales duration |
| No heavy work when motion starts | Every choice measured every name before the swing's first frame | The arrangement is reused: under 1 ms |
| Stay inside the frame budget | The cleared cells were redrawn every frame | Baked once and reused while the hand swings |
| Stagger, capped, never blocking | Names arrive along the ring's draw | Unchanged: within 0.30 s, keys work from the first frame |
| Reduced motion | GTK's animations setting off places everything at rest | Unchanged: GTK has no gentler mode to offer |
| No motion on keyboard-triggered surfaces | Motion on a key opened many times a day | Kept as Daniel asked: short, quiet and never delaying a key |

The opening and a swing to the opposite side at fixed timestamps, over the busy desktop:

![The opening with twenty sessions at 16, 50, 83, 116, 166, 216, 300 and 450 ms](motion-open.jpg)

![A hop across the dial with the trace riding the hand, at the same timestamps](motion-swing.jpg)

The whole reel as it plays, from the production renderer on a stepped 60 fps clock ([GIF](motion.gif)): opening with 6 and 20 sessions, a short hop, a hop across the dial, a new choice mid-swing, a blocked choice and a page turn at 30 sessions.

Frame cost on the cairo renderer the private desktop uses, from a release build of the gallery: each frame snapshots the whole 1920×1200 window and renders it to a texture. That is a worst case, since on screen GTK repaints only what changed; the budget at 60 fps is 16.7 ms.

| Scene | Frames | Median ms | 95th percentile ms | Slowest ms |
| --- | --- | --- | --- | --- |
| Opening, 6 sessions | 45 | 8.7 | 9.5 | 10.2 |
| Opening, 20 sessions | 45 | 10.1 | 12.1 | 14.7 |
| Short hop | 30 | 9.3 | 10.5 | 10.7 |
| Hop across the dial | 40 | 11.1 | 15.0 | 16.1 |
| New choice mid-swing | 40 | 11.4 | 15.9 | 16.8 |
| Blocked choice | 45 | 10.7 | 14.6 | 15.9 |
| Opening, 30 sessions | 36 | 11.3 | 12.7 | 13.3 |
| Page turn | 30 | 4.3 | 5.3 | 5.5 |

A new choice takes 0.4–0.8 ms before the swing's first frame can be drawn, 2.2 ms when the page changes: geometry depends only on the names and the room, so a choice reuses the last arrangement instead of measuring every name again. The trace's blur is clipped to the lit arc, the opening ring is drawn as arcs rather than a masked circle, and the cleared cells are baked once and reused while the hand swings.

## Layout

Sessions follow Herdr's order: workspace, tab, then pane layout, with left or top first. The first session, the leftmost pane, sits at one o'clock; the rest follow clockwise at even angles.

Every full name stands just outside the ring on its own tick's bearing:

- A name hangs off a point 24 px beyond the ring along its bearing: wholly to the right on the right half and wholly to the left on the left half, even beside the poles, so every key sits at its own tick; and above or below near the top and bottom. It never touches the ring.
- A name that wraps grows toward the equator, where the ticks are further apart, instead of pushing its neighbours toward the pole: above the equator its further lines hang below its first; below it, the name rises so that its last line ends where a one-line name's would.
- Names that would still touch are pushed apart toward their pole, the one nearest three or nine o'clock keeping its place; a name pushed away from the ring then slides back toward its anchor as far as the ring and its neighbours allow. Every key sits within 72 px of its own tick; with the gallery's names the farthest is 51 px at twenty sessions and 54 px at twenty-six, and a one-line name's key sits about 39 px out. There are no leader lines, boxes or key caps.
- Everything is JetBrains Mono. The uppercase key, in full ink, faces the dial, then two cells, then the name. A tag (pane, or product where different agents share a name) and a state caption (`working`, `blocked`, `waiting`, `queue`, `prefill`, `draft only`, `gone`) flow after the name's last word, smaller and quieter, the caption amber when the session needs the person. Names wrap only at spaces, or inside a single long token, at 58 characters first; they are never ellipsized.
- Names rest at 85% ink, and a session Record cannot reach rests dimmer. Hover brightens the name, lights its key and lifts its tick. Keyboard focus shows as a terminal cursor: the key in reverse video, the wash's colour on the light's, and the name underlined 2 px, 3 px below its baseline, with its tick lifted. The chosen name is in full ink with its key in the light's colour.

The ring grows with the count, from 150 px for one session to 290 px for twenty, and shrinks, never the names, when the screen is smaller. Geometry depends only on the count and on the names: every name is measured with the longest caption after it, so a refresh that changes a status, or a new choice, moves nothing; and since lines break greedily, a caption never moves a word of the name. A refresh in which no Herdr server answers is a failed read, not an empty list: while the wheel is open it keeps the sessions it shows and its page, and sending stays guarded.

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
| `F12 SEND` on the dial against `F12 Record` in the legend | `F12 → send` on the hub, the key in the light's colour; `F12 record` in the legend |
| Faint 12 px letter-spaced dial printing | 16 px mono in full ink |
| Delivery hints wrapped to three lines inside the dial | One fixed status line above the legend |
| The chosen name grew, a size jump | It brightens to full ink and its key takes the light's colour; nothing changes size or weight |
| Captions on a reserved second line doubled every name's height, so names near the poles stacked far from their ticks | The caption flows after the name's last word; boxes are one line |
| Names beside the poles were centred over their anchor, so two keys could sit between two ticks | Names hang wholly to their side; each key sits at its tick |
| A light on a spring of its own lagged the hand mid-swing | The trace is drawn at the hand and settles with it |
| Desktop text in the same mono face read as part of a name, through a glyph halo ([before and after](review-fix1-legibility.png)) | The cells under every line are cleared, as a terminal does |
| At twenty sessions the trace was a smudge over the washed page, about 1.25:1 ([before and after](review-fix2-trace.png)) | A near-white core over a 10 px halo, about 5:1, with a low fade of at most 0.3 around it |
| A wrapped name at twenty pushed its neighbours up to 93 px from their ticks ([before and after](review-fix4-tick-to-name.png)) | Wrapped names grow toward the equator and pushed names slide back; every key within 72 px, 51 px at worst with the gallery's names |
| Keys read as part of their names | Keys in full ink, two cells from the name, names at 85% |
| Keyboard focus was invisible at 100% ([before and after](review-fix6-focus.png)) | A reverse-video key cell and a 2 px underline, and the tick lifts |
| On a light theme the trace read as a second hand ([before and after](review-fix7-light.png)) | A saturated accent core with a faint bloom, and a neutral ink hand |
| The hand overshot its tick and settled back, which read as a bounce | A critically damped spring that never passes its tick, on any hop or retarget ([plot](hand-angle.png)) |
| The chosen tick, the chosen name's weight and the hub text snapped | The tick grows while the old one shrinks; the name eases by brightness; the hub and status line cross-fade |

The eleven artifacts Daniel confirmed for the wheel before the watch stay fixed: no desktop text competes with a name (the wash and the cleared cells), no banding (one flat wash), no accent arc, no box on the chosen name, on hover or around keys, no leader lines, no focus rings, no long name repeated in the centre, the `no_anim` rule, and geometry that never moves with status or choice.

## Evidence

| Boundary | Evidence |
| --- | --- |
| Native gate | `make linux-test` with `TMPDIR` on disk passed: 311 passed, zero failed, 72 gated or ignored. Both strict Clippy configurations, formatting and generated-feature consistency passed. Unit tests cover, besides the earlier geometry tests: every key within 72 px of its tick, wrapped names included, for 6, 20 and 26 sessions; the hand starting clear of the hub text at every bearing; the trace's bright core on the chosen tick, at most 0.3 at a neighbour and a low fade that reads at any count, and its tail only while the hand turns; the hand never passing its tick on a short hop or across the dial; a new choice mid-swing, toward a tick ahead, behind or further on, bending the swing without a jump and without passing; the chosen tick growing while the previous one shrinks; the opening drawing clockwise from one o'clock and bringing every tick and name in within 0.3 s on the strong ease-out; the light dimming in travel and brightening on arrival; amber easing in; a page turn and a hover lift finishing; everything placed at rest with animations off; and the wash deepening the canvas away from the ink. |
| Real application | `make linux-agent-voice-test` passed: 20 captures, 18 recognitions, five guarded submissions, two queued through Claude Code's inbox, a Codex send-if-idle and a busy Codex thread keeping its draft. Private X11 keys exercise selection, Escape, second F12, F9, paging at 30 sessions, server reorder and target disappearance, and the wheel fits the private screen from zero to 20 sessions. The four known narrow-editor allocation warnings remain and none comes from the wheel. |
| Shortcut portal | Not rerun: this change touches neither the shortcuts nor the Hyprland Lua. |
| Visual states | The `session_wheel_gallery` example renders the production wheel in 30 synthetic captures per palette (one, three, six, 20 and 30 sessions; long, duplicate, Czech, CJK and emoji names; blocked, missing, draft-only and untitled sessions; every delivery level; no selection; empty; hover; keyboard focus on a choice, on Record and on Cancel; the hand in flight and at rest), as an overlay that keeps its alpha or as the window fallback. It audits itself and fails if a name leaves the screen, two names overlap, or three status changes plus a new choice on twenty sessions move any name or any key; a build with the name not taking the caption's room fails that audit, as it should. Nord, default dark and default light overlays and the Nord and light window fallback passed: layout independent of status and choice. With `motion` as its last argument it renders the motion reel frame by frame on a stepped 60 fps clock and times each frame. |
| Visual review | Daniel rejected the watch as far too complex and asked for a minimal, terminal-like, see-through dial that keeps a lighting quality. Three light treatments (a glow on the hand and chosen tick, a trace along the ring, a phosphor wedge) were compared at 1, 6 and 20 sessions over the busy desktop; Daniel picked the trace. An impeccable critique of every state and palette followed: an assessment of design specificity, Nielsen's heuristics, cognitive load and two personas (26/40, judged authored for this product rather than interchangeable) and the bundled detector (no findings in the stylesheet). Applied: keys pinned at their ticks whatever caption follows a name (a real regression, now audited by the gallery), a reason and a next step for blocked and gone, higher text contrast and halos, the legend under the names, a focus underline and tick lift, a crisp trace on light themes, a 16 px hub, amber for `waiting` and a wider hollow draft tick. Kept as they are, with the brief: positional letters, a ring rather than a list above about twelve sessions, and announcements for screen readers. An independent finish review of the captures then asked for cleared cells under text, a brighter trace at twenty sessions, keys nearer their ticks, brighter keys, a visible focus and a light-theme trace that reads as light; all are applied, with before and after crops linked in the table above. Its reading of the pager as colliding with `Esc cancel` was desktop text showing through between them: the allocations never overlapped, and the gallery now audits the legend and pager for overlap and squeezing. The motion was then checked against Emil Kowalski's animation standards. |
| Live overlay | The previous build of this dial (`98b5cce`) was shown to Daniel as a real overlay on his desktop with synthetic sessions. He kept the styling and asked for smoother, bounce-free motion that never snaps and for F12 back in the hub; this version answers that. This version has not yet been shown live: no live F12 path, and no check of the cleared cells over his real desktop or of frame times on the GPU renderer. The Lenovo's 3840×2400 screen at scale 2 is not measured. |

Render the gallery inside the private desktop at the screen being judged:

```sh
cargo build --locked -p mluva-gtk --example session_wheel_gallery
OFFSCREEN_SCREEN_SPEC=1920x1200x24 bwrap --die-with-parent --bind / / --dev /dev --tmpfs /run/dbus -- \
  bash dev/run-isolated-browser.sh "$PWD/tmp/wheel-gallery" -- \
  "$CARGO_TARGET_DIR/debug/examples/session_wheel_gallery" nord "$PWD/tmp/wheel-gallery/png" overlay
```

Drop `overlay` for the window fallback. Overlay captures keep their alpha; composite them over a desktop to judge them. The gallery prints whether the layout was independent of status and choice and exits with a failure on any audit problem.

The [machine-readable result](results.json) contains only synthetic acceptance data; session names, panes and sockets are fixtures. The checks ran in a disposable X11 desktop with private HOME/XDG, D-Bus, accessibility, clipboard, audio fixtures and loopback providers. They do not establish physical F12/F9 input, a real microphone or provider, or the full live Wayland journey.
