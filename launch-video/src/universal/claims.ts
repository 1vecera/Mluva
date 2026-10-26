// Every claim the reel makes about routing, in one place. It must match what the target model
// ships (rust/mluva-core/src/agent_target.rs): a level is named only where a route delivers it.

export type Level = "SEND" | "QUEUE" | "PREFILL" | "DRAFT ONLY";

// The four delivery levels, strongest first, as the target model defines them.
export const LEVELS: { verb: Level; line: string }[] = [
  { verb: "SEND", line: "SUBMITTED TO THAT SESSION · CONFIRMED" },
  { verb: "QUEUE", line: "IN THE AGENT'S OWN QUEUE FOR THAT SESSION" },
  { verb: "PREFILL", line: "IN A NEW CHAT · YOU PRESS SEND" },
  { verb: "DRAFT ONLY", line: "KEPT AS AN EDITABLE DRAFT IN MLUVA" },
];

// Before the target model: Herdr's guarded prompt was the only way Mluva sent.
export const BEFORE: { kind: string; where: string; outcome: "SEND" | "DRAFT ONLY" | "NOT LISTED" }[] = [
  { kind: "CLAUDE CODE", where: "HERDR", outcome: "SEND" },
  { kind: "CODEX", where: "HERDR", outcome: "SEND" },
  { kind: "CLAUDE CODE", where: "TERMINAL · TMUX", outcome: "DRAFT ONLY" },
  { kind: "CODEX", where: "TERMINAL · TMUX", outcome: "NOT LISTED" },
  { kind: "CLAUDE DESKTOP", where: "CHAT", outcome: "NOT LISTED" },
  { kind: "CODEX DESKTOP", where: "APP", outcome: "NOT LISTED" },
];

// The transports behind the one model.
export const ROUTES = ["HERDR GUARDED PROMPT", "CLAUDE CODE INBOX", "CODEX QUEUE", "CLAUDE DESKTOP LINK"];

// The model's fields, filled one per beat.
export const FIELDS: { key: string; values: string[] }[] = [
  { key: "KIND", values: ["CLAUDE CODE", "CLAUDE DESKTOP", "CODEX CLI", "CODEX DESKTOP"] },
  { key: "NAME", values: ["THE AGENT'S OWN SESSION TITLE"] },
  { key: "STATE", values: ["IDLE", "WORKING", "BLOCKED", "GONE"] },
  { key: "DELIVERY", values: ["SEND", "QUEUE", "PREFILL", "DRAFT ONLY"] },
];

// Each transport and the level it delivers, as the routing code defines it (Route::delivery).
// Codex sends only to an idle live thread; a busy one keeps the draft, so it reads "SEND · IF IDLE".
export const PAIRS: { route: string; where: string; kind: string; state: string; level: string; line: string }[] = [
  { route: "HERDR GUARDED PROMPT", where: "CLAUDE CODE · CODEX IN HERDR", kind: "CLAUDE CODE", state: "IDLE", level: "SEND", line: "SUBMITTED TO THAT SESSION · CONFIRMED" },
  { route: "CLAUDE CODE INBOX", where: "TERMINAL · TMUX · DESKTOP CODE", kind: "CLAUDE CODE", state: "WORKING", level: "QUEUE", line: "TAKEN AT ITS NEXT TURN" },
  { route: "CODEX QUEUE", where: "TERMINAL · TMUX", kind: "CODEX CLI", state: "IDLE", level: "SEND · IF IDLE", line: "STARTED BY THAT THREAD · ELSE KEPT" },
  { route: "CLAUDE DESKTOP LINK", where: "A NEW CHAT", kind: "CLAUDE DESKTOP", state: "—", level: "PREFILL", line: "IN A NEW CHAT · YOU PRESS SEND" },
];
