import { loadFont } from "@remotion/fonts";
import { interpolate, staticFile } from "remotion";

export type CaptionWord = {
  text: string;
  start: number;
  end: number;
  emphasis: boolean;
};
export type CaptionPhrase = {
  start: number;
  end: number;
  words: CaptionWord[];
};

loadFont({
  family: "Adwaita Sans",
  url: staticFile("fonts/AdwaitaSans-Regular.ttf"),
  weight: "100 900",
});

const captionColor = {
  text: "oklch(0.98 0.005 260)",
  ink: "oklch(0.18 0.035 270)",
  surface: "oklch(0.18 0.035 270 / 0.94)",
  accent: "oklch(0.74 0.19 28)",
};
const clamp = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const;

export const PoppyCaptions: React.FC<{
  phrases: CaptionPhrase[];
  time: number;
}> = ({ phrases, time }) => {
  const phrase = phrases.find((p) => time >= p.start && time < p.end);
  if (!phrase) return null;
  const enter = interpolate(time - phrase.start, [0, 0.12], [0.98, 1], clamp);
  return (
    <div
      style={{
        position: "absolute",
        top: 42,
        left: 120,
        right: 120,
        display: "flex",
        justifyContent: "center",
        pointerEvents: "none",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          gap: 15,
          padding: "12px 25px",
          borderRadius: 21,
          background: captionColor.surface,
          boxShadow: "0 8px 28px #0005",
          transform: `scale(${enter})`,
          fontFamily: '"Adwaita Sans", sans-serif',
          fontSize: 58,
          fontWeight: 800,
          lineHeight: 1.2,
          letterSpacing: -1.4,
          whiteSpace: "nowrap",
        }}
      >
        {phrase.words.map((word, index) => {
          const highlighted = word.emphasis && time >= word.start;
          const pop = highlighted
            ? interpolate(time - word.start, [0, 0.07, 0.18], [1, 1.055, 1], clamp)
            : 1;
          return (
            <span
              key={index}
              style={{
                display: "inline-block",
                padding: word.emphasis ? "3px 10px 5px" : "3px 0 5px",
                borderRadius: 12,
                color: highlighted ? captionColor.ink : captionColor.text,
                background: highlighted ? captionColor.accent : "transparent",
                transform: `scale(${pop})`,
              }}
            >
              {word.text}
            </span>
          );
        })}
      </div>
    </div>
  );
};
