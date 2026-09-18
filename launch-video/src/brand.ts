import { staticFile } from "remotion";

/**
 * Single swap point for the identity.
 *
 * The files in public/brand are copies of docs/brand/svg (the final logo set, PR #38).
 * Every scene draws the identity through <Lockup/>, <Mark/> and <Wordmark/> in
 * components/Logo.tsx, which only animate the containers, so a future identity change
 * means copying new SVGs into public/brand and updating these paths. The SVGs carry their
 * own width/height, so no aspect ratios are hard-coded here.
 */
export const BRAND = {
  /** Hero lockup for splash surfaces: mark at 2.25× the M height, #F5F5F5 ink for dark backgrounds. */
  lockupSrc: staticFile("brand/mluva-logo-large-mark-on-dark.svg"),
  /** Compact lockup (1.05 ratio) for title bars and small surfaces. */
  lockupCompactSrc: staticFile("brand/mluva-logo-on-dark.svg"),
  /** Glossy red mark, square framing. */
  markSrc: staticFile("brand/mluva-mark.svg"),
  /** Wordmark alone, on-dark ink. */
  wordmarkSrc: staticFile("brand/mluva-wordmark-on-dark.svg"),
  /** Flat brand red, for accents that must match the mark. */
  red: "#E91B27",
  name: "Mluva",
  tagline: "Most delightful dictation for Omarchy",
  closing: "A little more delight every day.",
  license: "Open source · Apache-2.0",
  repo: "github.com/1vecera/Mluva",
} as const;
