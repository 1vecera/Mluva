# Mluva launch film — motion style guide

Derived from the waa catch-up loop (v6–v16): every rule below is implemented in `src/`, not aspirational.

## Easing vocabulary

- `EASE_OUT` (default in `tween`): entrances, reveals, settles. Nothing arrives linearly.
- `EASE_IN_OUT`: camera moves over a whole scene (S01, S02, S04, S05, S08, S11). Linear zooms are banned — they read as slideshows.
- Overshoot pops (S01 recorder, `over` term): scale peaks ~1.05 then settles. Only one pop per scene.
- Punch-ins: `punch(t, at, amp)` — up in 0.12 s, decay over 0.35 s. Land exactly on action beats: F9 presses, paste, tile/full snaps, polish apply, theme switches, first live block, question retire. Amplitudes 0.02–0.045, never stacked more than three per shot.

## Transition vocabulary

- Scene edges: `SceneFade` = opacity + 0.985→1→1.015 scale + alternating ±28 px horizontal slide (parity by scene index). Never a flat crossfade.
- Theme/mode switches inside a scene: 0.6 s crossfade plus a punch and (S04) a decaying accent glow on the newly selected pill.
- State changes inside widgets: slide + fade over 0.2–0.3 s (S03 step labels, chips stagger 0.08 s apart, S11 meta rows stagger 0.2 s).

## Camera

- Every scene has exactly one continuous move (slow push or drift + punch accents). No static shots except deliberate holds.
- Origins sit on the subject (F9 key, diff region, search box), never dead center by accident.
- Tightest framing is S06/S07 (~1.2×) and always eases back out so window chrome returns.

## Typography in motion

- Captions: one sentence per page (split at commas past 52 chars), active word in frost, whole page rises 10 px on entry.
- Kinetic descriptor (S02 lockup tagline): word-by-word rise, 0.06 s stagger.
- Closing line (S11): letter-spacing settles 2→−1 as it fades in.

## Colour rules

- Identity only through `src/brand.ts`. Scene palettes come from `theme.ts` (Nord default; Tokyo Night / Rosé Pine in S08).
- Backgrounds: deep radial gradient + water texture at 0.30 opacity + grain at 0.05 + top light + vignette. Texture must never read as smoke: low displacement, cool highlights, UI always dominant.
- Glow accents use the palette's own accent/frost/green/yellow at 0.2–0.55 alpha, never white.

## Sound rules

- Voice: one Sarah take, 30 ms edge fades per segment, unity gain; loudness finished in mastering (`script/master.sh`: −16 LUFS, TP −1.5).
- Music: downloaded bombinsound bed, ducks to 0.17 under narration, swells to 0.42–0.56 at edges, 0.3 s crossfaded loop seam.
- SFX ladder: clicks 0.25, switches 0.3, whooshes 0.3, ding 0.28. Every SFX lands on a visible action.

## Verification (the loop)

1. `npx tsc --noEmit` must pass before any render.
2. Fast loop: `remotion still` on the touched scene's frames; look with fresh eyes for the single biggest flaw.
3. Milestone loop: `script/review.sh vN` (preview + 22 timestamped frames + sheets + volumedetect + ebur128 summary).
4. Audio loop: `script/master.sh` on release candidates; integrated −16 ± 1 LUFS, true peak ≤ −1 dBTP.
5. Content loop: Scribe transcript of the mix must return all eleven lines in order (Omarchy heard as Omachi is a known Scribe bias, not a narration fault).
