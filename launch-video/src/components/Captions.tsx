import { useMemo } from "react";
import { AbsoluteFill, Sequence, useCurrentFrame, useVideoConfig } from "remotion";
import { FONT, NORD, hexToRgba } from "../theme";
import type { Timing, Word } from "../timing";

type Page = { words: Word[]; start: number; end: number };

const MAX_CHARS = 44;

/** Group the narration words into short caption pages, breaking on sentence ends and width. */
const buildPages = (timing: Timing): Page[] => {
  const pages: Page[] = [];
  for (const scene of timing.scenes) {
    let current: Word[] = [];
    const flush = () => {
      if (current.length) pages.push({ words: current, start: 0, end: 0 });
      current = [];
    };
    for (const w of scene.words) {
      const length = current.reduce((n, x) => n + x.text.length + 1, 0) + w.text.length;
      if (current.length && length > MAX_CHARS) flush();
      current.push(w);
      if (/[.?!]$/.test(w.text) && current.length >= 3) flush();
    }
    flush();
  }
  // Two passes: every start must exist before any end can look at the next page.
  pages.forEach((p) => {
    p.start = p.words[0].start - 0.12;
  });
  pages.forEach((p, i) => {
    const next = pages[i + 1];
    const naturalEnd = p.words[p.words.length - 1].end + 0.6;
    p.end = next ? Math.min(naturalEnd, next.start - 0.04) : naturalEnd;
  });
  return pages;
};

const CaptionPage: React.FC<{ page: Page; sceneOffset: number }> = ({ page, sceneOffset }) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const now = sceneOffset + frame / fps;
  return (
    <AbsoluteFill style={{ justifyContent: "flex-end", alignItems: "center", paddingBottom: 46 }}>
      <div
        style={{
          fontFamily: FONT,
          fontWeight: 500,
          fontSize: 34,
          lineHeight: 1.25,
          letterSpacing: -0.4,
          color: NORD.fgStrong,
          background: hexToRgba(NORD.deep, 0.66),
          border: `1px solid ${hexToRgba(NORD.fgStrong, 0.08)}`,
          padding: "12px 28px",
          borderRadius: 14,
          whiteSpace: "pre",
        }}
      >
        {page.words.map((w, i) => {
          const active = now >= w.start - 0.03 && now < (page.words[i + 1]?.start ?? w.end + 0.4);
          return (
            <span key={`${w.start}-${i}`} style={{ color: active ? NORD.frost : NORD.fgStrong }}>
              {(i ? " " : "") + w.text}
            </span>
          );
        })}
      </div>
    </AbsoluteFill>
  );
};

export const Captions: React.FC<{ timing: Timing }> = ({ timing }) => {
  const { fps } = useVideoConfig();
  const pages = useMemo(() => buildPages(timing), [timing]);
  return (
    <AbsoluteFill>
      {pages.map((page, i) => {
        const from = Math.round(page.start * fps);
        const duration = Math.max(1, Math.round((page.end - page.start) * fps));
        return (
          <Sequence key={i} from={from} durationInFrames={duration} layout="none">
            <CaptionPage page={page} sceneOffset={from / fps} />
          </Sequence>
        );
      })}
    </AbsoluteFill>
  );
};
