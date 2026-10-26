import { AbsoluteFill } from "remotion";
import { C, LABEL, mono, ramp } from "../../showreel/theme";

// The delivery shots: real Mluva and a disposable agent session, captured offscreen.
// Until the capture is staged this draws an empty frame with its slot named, for timing reviews.
export const Delivery: React.FC<{ which: "first" | "second"; frame: number; length: number }> = ({ which, frame, length }) => {
  const enter = ramp(frame, 0, 8);
  return (
    <AbsoluteFill style={{ opacity: enter * ramp(frame, length - 4, length, (t) => t, 1, 0) }}>
      {[0, 1].map((side) => (
        <div
          key={side}
          style={{
            position: "absolute",
            left: side ? 990 : 128,
            top: 210,
            width: 802,
            height: 620,
            border: "1px dashed rgba(245,245,245,0.2)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <div style={{ ...mono(15, LABEL) }}>{side ? "AGENT SESSION" : "MLUVA"} · CAPTURE PENDING</div>
        </div>
      ))}
      <div style={{ ...mono(15, C.ink3), position: "absolute", left: 128, top: 860 }}>{which.toUpperCase()} DELIVERY</div>
    </AbsoluteFill>
  );
};
