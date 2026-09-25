// One bar of the 128 BPM score per chapter. The score's first kick lands 14 ms in.
export const FPS = 60;
export const WIDTH = 1920;
export const HEIGHT = 1080;
export const DURATION = 900;
export const BPM = 128;
export const BEAT = 60 / BPM;
export const GRID_OFFSET = 0.014;

export const beatTime = (beat: number) => GRID_OFFSET + beat * BEAT;
export const beatFrame = (beat: number) => Math.round(beatTime(beat) * FPS);
export const beatAt = (frame: number) => (frame / FPS - GRID_OFFSET) / BEAT;

export type Chapter = { index: number; name: string; from: number; to: number };

const names = [
  "KINETIC TYPE",
  "UI ANIMATION",
  "MOTION SYSTEMS",
  "3D / PARTICLES",
  "LOGO ANIMATION",
  "DATA VISUALISATION",
  "TYPE MONTAGE",
  "END CARD",
];

export const CHAPTERS: Chapter[] = names.map((name, i) => ({
  index: i + 1,
  name,
  from: i === 0 ? 0 : beatFrame(i * 4),
  to: i === names.length - 1 ? DURATION : beatFrame((i + 1) * 4),
}));

export const chapterAt = (frame: number) =>
  CHAPTERS.find((chapter) => frame >= chapter.from && frame < chapter.to) ?? CHAPTERS[CHAPTERS.length - 1];
