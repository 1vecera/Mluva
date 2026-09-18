import timing from "./generated/timing.json";

export type Word = { text: string; start: number; end: number };

export type SceneTiming = {
  id: string;
  text: string;
  /** Absolute seconds on the film timeline. */
  sceneStart: number;
  lineStart: number;
  lineEnd: number;
  sceneEnd: number;
  /** Cut points inside the narration WAV, seconds. */
  srcStart: number;
  srcEnd: number;
  words: Word[];
};

export type Timing = {
  narrationFile: string;
  totalSeconds: number;
  scenes: SceneTiming[];
};

export const TIMING: Timing = timing as Timing;

export const sceneById = (id: string): SceneTiming => {
  const s = TIMING.scenes.find((x) => x.id === id);
  if (!s) throw new Error(`Unknown scene ${id}`);
  return s;
};
