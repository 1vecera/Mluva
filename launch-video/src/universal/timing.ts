// Sixteen bars of the 128 BPM score, 30 seconds. The score's grid sits 18 ms in (cues.json).
export const FPS = 60;
export const WIDTH = 1920;
export const HEIGHT = 1080;
export const DURATION = 1800;
export const BPM = 128;
export const BEAT = 60 / BPM;
export const GRID_OFFSET = 0.018;

export const beatTime = (beat: number) => GRID_OFFSET + beat * BEAT;
export const beatFrame = (beat: number) => Math.round(beatTime(beat) * FPS);
export const beatAt = (frame: number) => (frame / FPS - GRID_OFFSET) / BEAT;

export type Chapter = { index: number; name: string; from: number; to: number; beats: [number, number] };

// Chapter boundaries in beats. The drop lands on beat 32, the re-drop on 44, the final impact on 56.
const PLAN: [string, number, number][] = [
  ["KINETIC TYPE", 0, 8],
  ["SPLIT SCREEN", 8, 16],
  ["MOTION SYSTEMS", 16, 24],
  ["UI ANIMATION", 24, 32],
  ["SCREEN CAPTURE", 32, 44],
  ["TYPE MONTAGE", 44, 56],
  ["LOGO · END CARD", 56, 64],
];

export const CHAPTERS: Chapter[] = PLAN.map(([name, a, b], i) => ({
  index: i + 1,
  name,
  beats: [a, b],
  from: a === 0 ? 0 : beatFrame(a),
  to: b === 64 ? DURATION : beatFrame(b),
}));

export const chapterAt = (frame: number) =>
  CHAPTERS.find((chapter) => frame >= chapter.from && frame < chapter.to) ?? CHAPTERS[CHAPTERS.length - 1];
