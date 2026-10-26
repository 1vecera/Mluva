# Product

<!-- impeccable:product-schema 1 -->

Scope: the native macOS app in `macos/`. The repository's [product contract](../docs/product-contract.md) remains the authority for behavior and privacy. Facts marked *(inferred)* come from Daniel's brief of 26 October 2026, not from an interview.

## Platform

adaptive — one product whose design language follows each OS: GTK/Omarchy on Linux, Liquid Glass on macOS, Windows later.

## Users

People who write by speaking: developers dictating into editors, terminals and running Claude Code or Codex sessions, and anyone dictating into ordinary Mac apps. They are mid-task in another app; Mluva must appear, listen, deliver and get out of the way.

## Product Purpose

Turn speech into editable text and deliver it to the exact place the person started from, keeping the original recognition. On macOS the goal is to reach more users with a build that feels like it was made for the Mac *(inferred)*.

## Positioning

Dictation that targets agents as well as text fields: the F12 session wheel sends a finished dictation to a chosen running agent session, and screen annotation lets the person mark what they are describing.

## Operating Context

Global function-key shortcuts (F9 dictate, F8/F12 agent wheel, F7 keep draft), a menu-bar presence, a floating recording display, History for later editing, ElevenLabs Scribe for recognition, Herdr and Claude Code's own session registry for agents.

## Capabilities and Constraints

- Raw recognition stays immutable and separate from working edits and delivered text; volatile recognition is never delivered.
- Clipboard first; paste only into the exact captured, nonsecure target; never retry an uncertain paste.
- Incognito writes no History or retained audio.
- Credentials live in the Keychain and never appear in settings, History or logs.
- Agent sending requires Herdr's guarded prompt; sessions outside Herdr are draft-only.

## Brand Commitments

The glossy red jelly mark and soft-M wordmark in [docs/brand](../docs/brand/README.md); red `#E91B27` is the flat mark colour.

## Evidence on Hand

Synthetic content only in screenshots and tests. No user transcripts, recordings or private window names.

## Product Principles

- Native first: system components, SF Symbols, semantic colours and Liquid Glass where macOS puts it.
- The recording display is the product's face; everything else stays quiet.
- Never surprise the destination: what is sent, pasted or kept is always visible.
