import { Camera, useT } from "../components/Camera";
import { Caret, Workspace } from "../components/Workspace";
import { FONT, NORD, hexToRgba, revealed, tween } from "../theme";
import type { SceneTiming } from "../timing";
import { HERO, HERO_FONT } from "./S06Polish";
import { DRAFT, MarkdownBlocks, SOURCE, type Block } from "./S09Live";

const QUESTION_1 = "Who will use the export?";
const ANSWER = "It's for me, as a file I can keep.";
const QUESTION_2 = "How should files be named?";

const ANSWER_FROM = 0.9;
const RETIRE = 2.95;
const APPEND = 3.25;
const NEXT = 3.75;

const QuestionCard: React.FC<{ question: string; answer: string; typing: boolean; opacity: number; lift: number; t: number }> = ({ question, answer, typing, opacity, lift, t }) => (
  <div
    style={{
      opacity,
      transform: `translateY(${lift}px)`,
      marginTop: 22,
      padding: "16px 18px",
      borderRadius: 12,
      border: `1px solid ${hexToRgba(NORD.accent, 0.6)}`,
      background: hexToRgba(NORD.accent, 0.1),
      fontFamily: FONT,
      display: "flex",
      flexDirection: "column",
      gap: 10,
    }}
  >
    <div style={{ fontSize: 13, letterSpacing: 1, color: NORD.accent }}>QUESTION</div>
    <div style={{ fontSize: 22, color: NORD.fgStrong }}>{question}</div>
    <div style={{ fontSize: 20, color: answer ? NORD.fgStrong : NORD.muted, padding: "8px 12px", borderRadius: 8, background: NORD.deep, minHeight: 40, display: "flex", alignItems: "center" }}>
      {answer || "Answer by voice or type…"}
      {typing ? <Caret t={t} color={NORD.frost} height={22} /> : null}
    </div>
  </div>
);

/** S10 · Build a task spec: the question retires, the answer stays, the notes grow. */
export const S10TaskSpec: React.FC<{ scene: SceneTiming }> = () => {
  const t = useT();
  const answer = ANSWER.slice(0, revealed(t, ANSWER_FROM, 0.05, ANSWER.length));
  const retire = tween(t, [RETIRE, RETIRE + 0.3], [0, 1]);
  const punch10 = Math.min(tween(t, [RETIRE, RETIRE + 0.12], [0, 0.02]), tween(t, [RETIRE + 0.12, RETIRE + 0.47], [0.02, 0]));
  const next = tween(t, [NEXT, NEXT + 0.3], [0, 1]);
  const blocks: Block[] = [
    ...DRAFT.filter((b) => b.text !== "Personal use.").map((b) => ({ ...b, at: -1 })),
    t < APPEND ? { text: "Personal use.", kind: "p", at: -1 } : { text: "Audience: personal use; keep a file.", kind: "p", at: APPEND },
  ];
  const confirmed = retire > 0 ? [{ text: ANSWER, kind: "ok" as const, at: RETIRE }] : [];
  return (
    <Camera zoom={tween(t, [0, 1.2], [1.08, 1.18]) + punch10} originX={72} originY={52}>
      <div style={{ position: "absolute", left: HERO.x, top: HERO.y, width: HERO.w, height: HERO.h, border: `2px solid ${NORD.border}`, borderRadius: 16, overflow: "hidden", boxSizing: "border-box", boxShadow: `0 40px 100px ${hexToRgba("#000000", 0.5)}` }}>
        <Workspace
          width={HERO.w - 4}
          height={HERO.h - 4}
          palette={NORD}
          t={t}
          fontSize={HERO_FONT}
          selected={1}
          noteTitle="Notes export requirements"
          noteMeta="Task spec · Codex"
          badge="Task spec"
          statusLeft="1 question answered · 1 open"
          statusRight="Ctrl+1 Original · Ctrl+2 Draft"
          composer
          original={<span style={{ opacity: 0.85 }}>{SOURCE}</span>}
          draft={
            <>
              <MarkdownBlocks blocks={[...blocks, ...confirmed]} t={t} palette={NORD} />
              {retire < 1 ? <QuestionCard question={QUESTION_1} answer={answer} typing={t >= ANSWER_FROM && t < RETIRE} opacity={1 - retire} lift={-16 * retire} t={t} /> : null}
              {next > 0 ? <QuestionCard question={QUESTION_2} answer="" typing={false} opacity={next} lift={(1 - next) * 14} t={t} /> : null}
            </>
          }
        />
      </div>
    </Camera>
  );
};
