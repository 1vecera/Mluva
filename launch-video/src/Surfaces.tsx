import { Img, staticFile } from "remotion";
import data from "./generated/intro.json";

export const APP_WIDTH = data.width;
export const APP_HEIGHT = data.height;
export type Surface =
  | "empty"
  | "original"
  | "original-editable"
  | "polished"
  | "structured"
  | "history"
  | "history-search"
  | "commands"
  | "providers"
  | "providers-detail"
  | "provider-settings"
  | "settings"
  | "live-start"
  | "live-empty"
  | `live-progress-${1 | 2 | 3}`
  | "dictation-empty"
  | `dictation-${1 | 2 | 3}`
  | "dictation-full"
  | "grilling"
  | "grilling-edited"
  | `theme-${"nord" | "tokyo-night" | "rose-pine"}`;

/** Keep the native raster intact; a registered, opaque native clock patch advances time.
 * Whole-image compositing avoids fractional-scale seams and duplicate alpha.
 */
export const NativeApp: React.FC<{ surface: Surface; clock?: number }> = ({
  surface,
  clock,
}) => {
  const [x, y, width, height] = data.clock;
  return (
    <div
      style={{
        position: "relative",
        width: APP_WIDTH,
        height: APP_HEIGHT,
        background: "#000",
      }}
    >
      <Img
        src={staticFile(`ui/${surface}.png`)}
        style={{
          position: "absolute",
          inset: 0,
          width: APP_WIDTH,
          height: APP_HEIGHT,
        }}
      />
      {clock === undefined ? null : (
        <Img
          src={staticFile(
            `ui/clocks/clock-${Math.min(12, Math.max(0, clock)).toString().padStart(2, "0")}.png`,
          )}
          style={{ position: "absolute", left: x, top: y, width, height }}
        />
      )}
    </div>
  );
};

export const NativeReconstruction: React.FC<{
  surface: Surface;
  clock?: number;
}> = (props) => (
  <div
    style={{
      transform: "scale(2)",
      transformOrigin: "top left",
      width: APP_WIDTH,
      height: APP_HEIGHT,
    }}
  >
    <NativeApp {...props} />
  </div>
);
