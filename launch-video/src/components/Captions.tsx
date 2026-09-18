import { useMemo } from "react";
import { AbsoluteFill, Sequence, useCurrentFrame, useVideoConfig } from "remotion";
import { FONT, NORD, hexToRgba } from "../theme";
import type { Timing, Word } from "../timing";

type Page = { words: Word[]; start: number; end: number };

const MAX_CHARS = 52;

const chars = (words: Word[]) => words.reduce((n, w) => n + w.text.length, 0) + Math.max(0, words.length - 1);

/** Split a run of words into pages: whole if it fits, else at the comma nearest the middle, else at the word nearest the middle. */
const splitRun = (words: Word[]): Word[][] => {
  if (chars(words) <= MAX_CHARS || words.length < 2) return [words];
  const mid = words.length / 2;
  let cut = -1;
  let best = Infinity;
  words.forEach((w, i) => {
    if (i < words.length - 1 && /[,;:]$/.test(w.text) && Math.abs(i + 1 - mid) < best) {
      best = Math.abs(i + 1 - mid);
      cut = i + 1;
    }
  });
  if (cut < 0) cut = Math.round(mid);
  return [...splitRun(words.slice(0, cut)), ...splitRun(words.slice(cut))];
};

/** Group the narration words into caption pages: one sentence per page, long sentences split at commas. */
const buildPages = (timing: Timing): Page[] => {
  const pages: Page[] = [];
  for (const scene of timing.scenes) {
    let sentence: Word[] = [];
    const flush = () => {
      if (sentence.length) for (const run of splitRun(sentence)) pages.push({ words: run, start: 0, end: 0 });
      sentence = [];
    };
    for (const w of scene.words) {
      sentence.push(w);
      if (/[.?!]$/.test(w.text)) flush();
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
          fontSize: 36,
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
