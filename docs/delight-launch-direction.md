# Mluva launch direction

The most delightful dictation app for Omarchy. A small native tool for speaking, keeping, editing and polishing useful text. Every visible control should earn its space; the app follows the desktop's theme and lets the writing stay primary.

This direction reconciles Daniel's two 2026-09-09 notes. The newer note controls feature scope; the older note supplies the preference for the original animated opening and recognizably Omarchy desktop. The product name remains Mluva and the new logo from PR #17 is retained.

## This implementation

- Stabilize Live dictation and draft widths, preserve both panes through final reconciliation, and reserve independent scrollbar gutters.
- Follow incoming text smoothly with configurable room below the newest line; preserve deliberate manual scrolling and edits.
- Replace Recording/REC wording with a calm centered red pulse around its existing size, keeping the timer, Stop and accessible state.
- Render basic Markdown with small headings and restrained emphasis while retaining exact editable/copyable/savable source.
- Add Ctrl+P for searchable existing app actions, including recording, Live, polishing, rewriting, copying, saving, History and Settings.
- Merge PRs #17–19, validate the combined runtime, install it on Daniel's Lenovo, then capture new footage from that installed build.

## Film

Target roughly 55 seconds. Open with the new large Mluva identity and positioning over the existing clean H3 animated background. Introduce Omarchy through the desktop itself, then show the floating recorder, recording in the app, Live rewrite, Copy, Polish, editing and Save, Ctrl+P, and a theme change. Return to the identity at the close.

Use fast connected cuts, purposeful camera movement and crisp software-rendered text. Give the app most of the frame; avoid a persistent side headline or a menu-by-menu presentation. Real GTK and Quickshell controls provide the feature footage. Synthetic content and provider fixtures are documented as such; editing speed is not a provider-latency claim. Raw captures, the composition recipe and QA remain inspectable.

## Resolved working choices

- Install target: this Lenovo's existing per-user Mluva installation.
- Recording pulse: inward/outward size change around a fixed center, with no lateral movement.
- Live separation: equal-width panes and a small gap, stacking at the existing narrow breakpoint.
- Early scroll: retain the existing configurable line lookahead, with proportional anticipation as the last line fills.
- Ctrl+P: command search over existing actions, rather than a new settings or automation system.
- Video tooling: the existing offline capture/FFmpeg pipeline, using real text and native app pixels over background motion.

Bundling with Omarchy and a dedicated default shortcut are future aspirations. This pass does not establish readiness for every live compositor, audio device, target application or macOS installation; the existing capability register retains those acceptance boundaries.
