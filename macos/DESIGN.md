---
name: Mluva for macOS
description: A drafting instrument made of glass for speaking into apps and agent sessions.
colors:
  redline: "#E91B27"
  redline-on-graphite: "#FF4048"
  graphite: "#121316"
  chalk: "#F0F0F0"
  chalk-dim: "#F0F0F09E"
  chalk-faint: "#F0F0F061"
  rule-on-graphite: "#FFFFFF29"
  amber: "#FFB847"
typography:
  display:
    fontFamily: "JetBrains Mono, SF Mono, monospace"
    fontSize: "30pt"
    fontWeight: 700
    lineHeight: 1.1
  headline:
    fontFamily: "JetBrains Mono, SF Mono, monospace"
    fontSize: "22pt"
    fontWeight: 700
  title:
    fontFamily: "JetBrains Mono, SF Mono, monospace"
    fontSize: "15pt"
    fontWeight: 700
  body:
    fontFamily: "JetBrains Mono, SF Mono, monospace"
    fontSize: "13.5pt"
    fontWeight: 400
    lineHeight: "22pt"
  label:
    fontFamily: "JetBrains Mono, SF Mono, monospace"
    fontSize: "11pt"
    fontWeight: 500
  state:
    fontFamily: "JetBrains Mono, SF Mono, monospace"
    fontSize: "10.5pt"
    fontWeight: 700
    letterSpacing: "1.2pt"
rounded:
  keycap: "3pt"
  plate: "6pt"
  sheet: "10pt"
spacing:
  hairline: "0.75pt"
  cell: "12pt"
  sheet: "24pt"
components:
  keycap:
    textColor: "{colors.chalk}"
    typography: "{typography.label}"
    rounded: "{rounded.keycap}"
    padding: "0 5pt"
  keycap-lit:
    backgroundColor: "{colors.redline}"
    textColor: "#FFFFFF"
    rounded: "{rounded.keycap}"
  pill-strip:
    backgroundColor: "{colors.graphite}"
    textColor: "{colors.chalk}"
    rounded: "{rounded.sheet}"
    width: "540pt"
  wheel-plate:
    backgroundColor: "{colors.graphite}"
    textColor: "{colors.chalk}"
    rounded: "{rounded.plate}"
    padding: "7pt 12pt 7pt 7pt"
---

# Design System: Mluva for macOS

## Overview

**Creative North Star: "The glass drafting instrument"**

Mluva on the Mac shares the spirit of the Omarchy build, not its styling: keyboard first, text first, one typeface, honest labels and nothing decorative. Words are set like measurements in JetBrains Mono, the same bundled face as on Linux. Every key is drawn as an outlined legend. Structure comes from hairline rules, ticks and leader lines rather than boxes and cards. Everything that floats over other apps is graphite Liquid Glass, so the pill and wheel read the same over any wallpaper or window.

Red is the redline: the live recording signal, the chosen session and the marks you draw on the screen. Nothing else is red. The glossy jelly mark from `docs/brand` stays the one organic shape, as the signal lamp.

The first build was rejected as generic: SF Pro, rounded glass capsules and tinted chips. Those are the anti-reference.

**Key Characteristics:**
- One typeface everywhere the app writes its own words; native controls keep their native chrome.
- Keycaps, rules, ticks and leader lines are the whole ornament vocabulary.
- Graphite glass for floating layers; windows follow the system appearance.
- Red only for live, chosen or marked; amber only for needs-attention.

## Colors

Monochrome graphite and chalk, cut by one redline.

### Primary
- **Redline** (#E91B27; #FF4048 on graphite): recording state word and timer, the lit keycap of the action that stops recording or records for a session, the wheel's selection arc, node and plate underline, the cancel-window line, screen ink and keyframe badges.

### Neutral
- **Graphite** (#121316 at 55% under Liquid Glass): the ground of the pill, transcript sheet, wheel disc, wheel plates and pager.
- **Chalk** (#F0F0F0): text on graphite. Dim (62%) for secondary lines, faint (38%) for volatile words and ring ticks.
- **Rule on graphite** (white at 16%): hairlines and glass edges on floating layers. In windows the rule is `primary` at 16%.
- **Amber** (#FFB847): a blocked agent session and delivery that needs checking. Never decorative.

### Named Rules
**The Redline Rule.** Red marks only what is live, chosen or drawn by the person. If it is neither, it is chalk or `primary`.

## Typography

**All authored text:** JetBrains Mono Regular, Medium and Bold, bundled in `Contents/Resources/Fonts` with its OFL licence and registered through `ATSApplicationFontsPath`.
**Native chrome:** window titles, toolbars, search fields, form controls and buttons keep the system face.

### Hierarchy
- **Display** (Bold, 30 pt): the Welcome statement only.
- **Headline** (Bold, 22 pt): each Welcome step's lowercase title.
- **Title** (Bold, 15 pt; 18 pt in History): the wheel's chosen session and the History title.
- **Body** (Regular, 13.5–14 pt on 22 pt leading): transcripts and the History editor, which keeps a 640 pt measure (about 72 characters).
- **Label** (Medium, 11–12.5 pt): row titles, plate names, actions, keycap legends.
- **State** (Bold, 10.5 pt, uppercase, 1.2 pt tracking): REC, SEND, PASTED, READY and other one-word states.

### Named Rules
**The Lowercase Voice Rule.** Mluva's own labels and actions are lowercase (dictate, agent wheel, keep draft, history). Sentences keep sentence case.

## Layout

Floating layers sit bottom centre (pill, 540 pt wide) or fill the screen (wheel). The pill is a title block: cells divided by vertical hairlines for state, destination and actions, with a 14 pt footer band for the live trace or the countdown. The wheel centres a 236 pt disc inside a 146 pt-radius protractor ring with 72 ticks (long every 30°); plates sit on the tightest ring that clears the disc and each other, joined to their ring node by a radial-then-straight leader. Windows use 16–24 pt margins and separate groups with full-width rules.

## Elevation & Depth

Depth comes from the glass itself and the 42% black dim behind the wheel; nothing adds its own shadow except the jelly lamp's contact shadow. Floating layers are one level: glass is never stacked on glass.

## Shapes

Small, squared corners: 3 pt keycaps, 6 pt plates, 10 pt sheets, circles only for the wheel disc, ring and nodes. Lines are 0.75 pt hairlines; the selection arc is 2 pt with round caps; leaders are 0.75 pt, 1.25 pt when chosen.

## Components

### Keycap
An outlined 1 pt box (`primary` at 45%) with a Medium mono legend; lit, it fills red with a white legend. Used for every shortcut, and as a button when the action has a key.

### Recording strip and sheet
The strip shows the jelly lamp, a state word and a 00:00 timer; then `→ destination` with a dim detail line; then keycap actions and plain lowercase actions (cancel, dismiss) that underline on hover. The footer is a scrolling hairline level trace while recording, or a red dimension line that shrinks over the two-second send window with the time left beside it. The sheet above holds up to five lines of live words; volatile words are faint.

### Session dial
Protractor ring, nodes, leaders and the red arc from twelve o'clock to the chosen session. Plates hold a keycap letter, the session name and a status mark with words. Status marks are drawn on a fixed ramp: ready is a solid dot, working a half-filled ring, blocked an amber diamond, unknown an empty ring, draft-only an empty square.

### Menu bar panel
A key legend: the header with the lamp, `mluva` and a state word; rows of keycap, action and current value; the last dictation in mono with copy and open; lowercase links for history, settings and quit.

### History
A mono sidebar whose rows put the outcome left and the time right against the margin; a mono title, a ruled meta line and the editor on a fixed measure; keyframes as hairline-framed thumbnails.

## Do's and Don'ts

### Do:
- **Do** draw a shortcut as a `Keycap` wherever it is mentioned.
- **Do** separate with 0.75 pt rules and leader lines before reaching for a container.
- **Do** keep native controls (toggles, pickers, search, toolbars) native and set only their words in mono.

### Don't:
- **Don't** use SF Pro or rounded system type for Mluva's own words.
- **Don't** use capsule chips, tinted glass or cards to group content.
- **Don't** use red for anything that is not live, chosen or a mark.
