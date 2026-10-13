import { account, accessToken, signIn, signOut } from "./auth.mjs";
import { draftStorage } from "./storage.mjs";
import { DeviceWire } from "./wire.mjs";
import { CloudSpeech } from "./speech.mjs";

const $ = (id) => document.getElementById(id);
const config = await (
  await fetch("/config.json", { cache: "no-store" })
).json();
if (config.origin && config.origin !== location.origin && !config.dev) {
  const link = document.createElement("a");
  link.href = config.origin;
  link.className = "quiet";
  link.textContent = "Open the new Mluva address";
  document.querySelector(".header-actions").prepend(link);
}
let auth = account(),
  deviceId,
  devices = [],
  wire,
  speech,
  started,
  ticker,
  wakeLock,
  saving = false,
  stopping = false,
  signingOut = false,
  recording = false;
let recordingId = crypto.randomUUID(),
  rawText = "",
  audio,
  audioUrl,
  historyEntries = [],
  cursor,
  storageTimer,
  storageQueue = Promise.resolve(),
  saved = false;
let receiversReached,
  sendingText = false;
let selected = new Set(),
  selectionInitialized = false,
  streamId,
  sequence = 0,
  sending = Promise.resolve(),
  liveFailure = false;
let noteState = null,
  pendingSave,
  conflictNote,
  deleted = false,
  editGeneration = 0,
  rewriteGeneration = 0,
  rewriteResult,
  continuation,
  commandSource,
  continueRequested = false;
const templates = {
  polish:
    "Improve clarity and grammar. Preserve facts, meaning and language. Return the revised text only.",
  notes:
    "Organize these words into concise Markdown notes with headings and bullet points. Preserve uncertainty and do not invent facts.",
  questions:
    "Identify unanswered questions in these notes, then organize the supplied facts. Clearly distinguish questions from established facts.",
  custom: "",
};
const incoming = new Map();
const samples = [];
function notice(message) {
  $("notice").textContent = message;
  $("notice").hidden = !message;
}
function state(online, label) {
  $("connection").dataset.state = online ? "online" : "offline";
  $("connection-label").textContent = label;
}
function buttonState() {
  $("record").disabled =
    saving ||
    stopping ||
    sendingText ||
    !wire ||
    (wire.stopped && !$("incognito").checked) ||
    !devices.some((d) => d.id === deviceId) ||
    !config.speechEnabled;
  $("send").disabled =
    $("incognito").checked ||
    recording ||
    saving ||
    sendingText ||
    !wire ||
    wire.socket?.readyState !== WebSocket.OPEN;
  $("save").disabled =
    recording || saving || stopping || deleted || $("incognito").checked;
  $("cancel-recording").hidden = !recording;
  $("rewrite").disabled =
    recording || $("incognito").checked || !config.rewriteEnabled;
  $("save-template").disabled = $("incognito").checked;
  $("new-note").disabled = recording || saving;
  $("clear").disabled = recording || saving || stopping;
  $("draft").readOnly = recording || saving || stopping;
  $("note-title").disabled = recording || saving || stopping;
  $("task-mode").disabled = recording || saving || stopping;
  $("continue-recording").disabled =
    $("record").disabled ||
    recording ||
    !noteState ||
    !saved ||
    noteState.mode !== "dictation" ||
    $("incognito").checked;
  $("continue-recording").hidden = recording;
  $("merge-note").disabled =
    recording ||
    saving ||
    !noteState ||
    !saved ||
    noteState.mode !== "dictation" ||
    $("incognito").checked;
  $("retain").disabled = recording || saving || stopping;
}
async function api(path, method = "GET", body) {
  const assertSession = () => {
    if (signingOut && !(method === "DELETE" && path === `/devices/${deviceId}`))
      throw new Error("Signed out here. Sign in to resume account sync.");
  };
  assertSession();
  const token = await accessToken(config);
  assertSession();
  const reply = await fetch(
    `${config.api}${path}${path.includes("?") ? "&" : "?"}deviceId=${deviceId ?? ""}`,
    {
      method,
      headers: {
        authorization: `Bearer ${token}`,
        ...(body ? { "content-type": "application/json" } : {}),
      },
      ...(body ? { body: JSON.stringify({ ...body, deviceId }) } : {}),
      cache: "no-store",
      signal: AbortSignal.timeout(15000),
    },
  );
  let result;
  try {
    result = await reply.json();
  } catch {
    throw new Error(
      "The host did not respond. Your words stay here; reconnect to retry.",
    );
  }
  if (!reply.ok)
    throw Object.assign(
      new Error(
        result.message ?? "Connection failed. Keep your draft and retry.",
      ),
      { status: reply.status },
    );
  assertSession();
  return result;
}
function persist() {
  if (!auth) return Promise.resolve();
  const value =
    !$("incognito").checked &&
    $("retain").checked &&
    ($("draft").value || audio)
      ? {
          id: recordingId,
          rawText,
          text: $("draft").value,
          audio,
          noteState,
          pendingSave,
          title: $("note-title").value,
          mode: $("task-mode").value,
        }
      : null;
  // Serialize writes and capture the snapshot now, so a delayed old write cannot resurrect a cleared draft.
  storageQueue = storageQueue
    .catch(() => {})
    .then(() => draftStorage(auth.owner, value));
  storageQueue.catch((error) => notice(error.message));
  return storageQueue;
}
function recovery(
  message = "Retry saving when you reconnect, or download a copy.",
) {
  $("recovery").hidden = false;
  $("recovery-message").textContent = message;
  if (audioUrl) URL.revokeObjectURL(audioUrl);
  $("download-audio").hidden = !audio?.size;
  if (audio?.size) {
    audioUrl = URL.createObjectURL(audio);
    $("download-audio").href = audioUrl;
    $("download-audio").download =
      `mluva-${recordingId}.${audio.type.includes("mp4") ? "m4a" : "webm"}`;
  }
  $("recovery").querySelector("strong").textContent = $("retain").checked
    ? "Your draft is kept on this device."
    : "Keep this page open to retain your draft.";
}
function download(name, content, type = "text/markdown") {
  const url = URL.createObjectURL(new Blob([content], { type })),
    link = document.createElement("a");
  link.href = url;
  link.download = name;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
async function copy(content, message = "Copied on this device.") {
  try {
    await navigator.clipboard.writeText(content);
    notice(message);
  } catch {
    notice(
      "Clipboard permission was refused. Select the text and copy it manually.",
    );
  }
}
function confirm(title, description) {
  $("confirm-title").textContent = title;
  $("confirm-description").textContent = description;
  $("confirm-dialog").showModal();
  return new Promise((resolve) => {
    const finish = (answer) => {
      $("confirm-dialog").close();
      $("confirm-yes").onclick = null;
      $("confirm-no").onclick = null;
      $("confirm-dialog").oncancel = null;
      resolve(answer);
    };
    $("confirm-yes").onclick = () => finish(true);
    $("confirm-no").onclick = () => finish(false);
    $("confirm-dialog").oncancel = () => finish(false);
  });
}
async function refreshDevices() {
  const known = new Set(devices.map((device) => device.id));
  const result = await api("/devices");
  devices = result.devices;
  if (selectionInitialized)
    devices
      .filter((device) => device.id !== deviceId && !known.has(device.id))
      .forEach((device) => selected.add(device.id));
  config.speechEnabled = result.speechEnabled;
  config.rewriteEnabled = result.rewriteEnabled;
  if (!selectionInitialized) {
    selected = new Set(
      devices.filter((d) => d.id !== deviceId).map((d) => d.id),
    );
    selectionInitialized = true;
  }
  selected = new Set(
    [...selected].filter((id) =>
      devices.some((d) => d.id === id && id !== deviceId),
    ),
  );
  $("devices").replaceChildren();
  for (const device of devices) {
    const row = document.createElement("div");
    row.className = "device";
    const label = document.createElement("label"),
      checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.checked = selected.has(device.id);
    checkbox.disabled = device.id === deviceId || recording;
    checkbox.setAttribute("aria-label", `Send to ${device.name}`);
    checkbox.onchange = () => {
      if (checkbox.checked) selected.add(device.id);
      else selected.delete(device.id);
    };
    const icon = document.createElement("span");
    icon.className = "device-icon";
    icon.textContent =
      device.kind === "phone" ? "▯" : device.kind === "tablet" ? "▭" : "⌘";
    icon.setAttribute("aria-hidden", "true");
    const words = document.createElement("div"),
      name = document.createElement("strong"),
      caption = document.createElement("span");
    name.textContent = device.name;
    caption.className = "small";
    caption.textContent =
      device.id === deviceId
        ? "This device"
        : device.online
          ? "Connected"
          : "Offline · history will be available";
    words.append(name, caption);
    label.append(checkbox, icon, words);
    row.append(label);
    const remove = document.createElement("button");
    remove.className = "quiet";
    remove.textContent = "Remove";
    remove.disabled = recording;
    remove.setAttribute("aria-label", `Remove ${device.name}`);
    remove.onclick = async () => {
      if (
        !(await confirm(
          `Remove ${device.name}?`,
          "This device will lose live access. Saved history remains in your account.",
        ))
      )
        return;
      try {
        await api(`/devices/${device.id}`, "DELETE");
        if (device.id === deviceId) {
          wire?.close();
          localStorage.removeItem(`mluva-device:${auth.owner}`);
          await signOut(config);
        } else await refreshDevices();
      } catch (error) {
        notice(error.message);
      }
    };
    row.append(remove);
    $("devices").append(row);
  }
  const current = devices.find(
    (device) => device.id === deviceId && device.currentSession,
  );
  $("device-name").textContent = current?.name ?? "this device";
  $("record-hint").textContent = config.speechEnabled
    ? "Keep this screen open while recording."
    : "Cloud speech is awaiting host activation. Send text below, or use native Mluva.";
  buttonState();
  return Boolean(current);
}
function renderHistory() {
  const choice = $("merge-with").value;
  $("merge-with").replaceChildren(new Option("Choose a conversation…", ""));
  for (const entry of historyEntries.filter(
    (e) => e.id !== recordingId && (e.mode ?? "dictation") === "dictation",
  ))
    $("merge-with").add(new Option(entry.title, entry.id));
  $("merge-with").value = choice;
  $("history").replaceChildren();
  const needle = $("search").value.toLocaleLowerCase();
  for (const entry of historyEntries.filter((item) =>
    `${item.title} ${item.text} ${item.rawText}`
      .toLocaleLowerCase()
      .includes(needle),
  )) {
    const card = document.createElement("article");
    card.className = "history-card";
    const meta = document.createElement("div");
    meta.className = "meta";
    meta.textContent = `${new Date(entry.createdAt).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })} · ${devices.find((d) => d.id === entry.deviceId)?.name ?? "Saved device"}`;
    const title = document.createElement("h3");
    title.textContent = entry.title;
    const words = document.createElement("p");
    words.textContent = entry.text;
    const actions = document.createElement("div");
    actions.className = "row";
    const copyButton = document.createElement("button");
    copyButton.className = "secondary";
    copyButton.textContent = "Copy";
    copyButton.onclick = () => copy(entry.text);
    const exportButton = document.createElement("button");
    exportButton.className = "quiet";
    exportButton.textContent = "Export";
    exportButton.onclick = () =>
      download(`mluva-${entry.id}.md`, markdown(entry));
    const remove = document.createElement("button");
    remove.className = "quiet";
    remove.textContent = "Delete";
    remove.onclick = async () => {
      if (
        !(await confirm(
          "Delete these words?",
          "This recording will be removed from cloud history on all your devices.",
        ))
      )
        return;
      try {
        await api(`/history/${entry.id}`, "DELETE");
        if (recordingId === entry.id) {
          deleted = true;
          saved = true;
          pendingSave = undefined;
          buttonState();
          notice(
            "This note was deleted. Export your open text if you need a copy.",
          );
        }
        historyEntries = historyEntries.filter((item) => item.id !== entry.id);
        renderHistory();
      } catch (error) {
        notice(error.message);
      }
    };
    const original = document.createElement("details"),
      summary = document.createElement("summary"),
      originalWords = document.createElement("p");
    summary.textContent = "Original words";
    originalWords.textContent = entry.rawText;
    original.append(summary, originalWords);
    const open = document.createElement("button");
    open.className = "secondary";
    open.textContent = "Open";
    open.onclick = () => openNote(entry);
    actions.append(open, copyButton, exportButton, remove);
    card.append(meta, title, words, original, actions);
    $("history").append(card);
  }
  if (!$("history").children.length) {
    const empty = document.createElement("p");
    empty.className = "loading";
    empty.textContent = needle
      ? "No match in loaded history. Load earlier recordings to search further."
      : "Your first thought belongs here. Record or type something, then save it.";
    $("history").append(empty);
  }
  $("more").hidden = !cursor;
}
let historyLoading = false,
  historyPending = false;
async function loadHistory(earlier = false) {
  if ($("incognito").checked) return;
  if (historyLoading) {
    if (!earlier) historyPending = true;
    return;
  }
  historyLoading = true;
  try {
    const result = await api(
      `/history${earlier && cursor ? `?cursor=${encodeURIComponent(cursor)}` : ""}`,
    );
    historyEntries = earlier
      ? [
          ...historyEntries,
          ...result.entries.filter(
            (item) => !historyEntries.some((old) => old.id === item.id),
          ),
        ]
      : result.entries;
    cursor = result.cursor;
    const current = historyEntries.find((e) => e.id === recordingId);
    if (
      current &&
      noteState &&
      current.revision > noteState.revision &&
      saved &&
      !recording &&
      !saving
    )
      await openNote(current);
    renderHistory();
  } finally {
    historyLoading = false;
    if (historyPending) {
      historyPending = false;
      loadHistory().catch((error) => notice(error.message));
    }
  }
}
function renderIncoming() {
  $("incoming").replaceChildren();
  for (const session of [...incoming.values()].reverse()) {
    const card = document.createElement("article");
    card.className = "incoming-card";
    const title = document.createElement("h3");
    title.textContent = `${session.deviceName} · ${session.phase === "stopped" ? "Finished" : session.interrupted ? "Connection interrupted" : "Speaking"}`;
    const settled = document.createElement("p");
    settled.textContent = session.text;
    const partial = document.createElement("p");
    partial.className = "partial";
    partial.textContent = session.partial;
    const row = document.createElement("div");
    row.className = "row";
    const button = document.createElement("button");
    button.className = "secondary";
    button.textContent = "Copy settled words";
    button.disabled = !session.text;
    button.onclick = () => copy(session.text);
    const exported = document.createElement("button");
    exported.className = "quiet";
    exported.textContent = "Download";
    exported.disabled = !session.text;
    exported.onclick = () =>
      download(`mluva-live-${session.id}.md`, session.text);
    row.append(button, exported);
    card.append(title, settled, partial, row);
    $("incoming").append(card);
  }
  if (!incoming.size) {
    const placeholder = document.createElement("p");
    placeholder.className = "empty-state";
    placeholder.textContent = "Words from your other devices will appear here.";
    $("incoming").append(placeholder);
  }
}
function event(message) {
  if (signingOut || $("incognito").checked) return;
  if (message.type === "stream" && message.deviceId !== deviceId) {
    let session = incoming.get(message.sessionId);
    if (!session) {
      session = {
        id: message.sessionId,
        deviceName: message.deviceName,
        text: "",
        partial: "",
        sequence: -1,
        missing: message.phase !== "started",
      };
      incoming.set(message.sessionId, session);
    }
    if (message.sequence <= session.sequence) return;
    if (message.sequence !== session.sequence + 1) session.missing = true;
    session.sequence = message.sequence;
    session.phase = message.phase;
    if (message.phase === "partial") session.partial = message.text;
    if (message.phase === "segment") {
      session.text += `${session.text ? "\n" : ""}${message.text}`;
      session.partial = "";
    }
    if (message.phase === "stopped") session.partial = "";
    if (session.missing)
      notice(
        "Some live updates were missed. The sender can save the complete words to history.",
      );
    while (incoming.size > 6) incoming.delete(incoming.keys().next().value);
    renderIncoming();
  } else if (message.type === "history.changed") {
    if (message.deleted && message.id === recordingId) {
      deleted = true;
      saved = true;
      pendingSave = undefined;
      editGeneration++;
      rewriteGeneration++;
      persist();
      notice(
        "This note was deleted on another device. It will not be saved again.",
      );
    }
    loadHistory().catch((error) => notice(error.message));
  } else if (
    message.type === "devices.changed" ||
    message.type === "reconnected"
  ) {
    refreshDevices().catch((error) => notice(error.message));
    if (message.type === "reconnected")
      loadHistory().catch((error) => notice(error.message));
  } else if (message.type === "revoked") {
    wire.close();
    localStorage.removeItem(`mluva-device:${auth.owner}`);
    state(false, "Removed");
    notice(
      "This device was removed. Your unsaved draft remains here; sign out to connect it again.",
    );
    if (recording) stopRecording(false);
  } else if (message.type === "disconnected") {
    incoming.forEach((session) => {
      session.interrupted = true;
      session.partial = "";
    });
    renderIncoming();
    if (recording) {
      notice(
        "Live connection interrupted. Ordered updates will retry on reconnect; save the complete words to history.",
      );
    }
  } else if (message.type === "error") notice(message.message);
  buttonState();
}
function beginLive() {
  streamId = crypto.randomUUID();
  sequence = 0;
  receiversReached = undefined;
  liveFailure = false;
  sending = Promise.resolve();
  return publish("started");
}
function publish(phase, content = "") {
  if (liveFailure || $("incognito").checked) return Promise.resolve();
  if (!selected.size && phase === "started") {
    liveFailure = true;
    notice(
      "No receiving device selected. Your words will stay here and can be saved to history.",
    );
    return Promise.resolve();
  }
  const message = {
    sessionId: streamId,
    sequence: sequence++,
    phase,
    text: content,
    targets: [...selected],
  };
  sending = sending
    .then(() => wire.publish(message))
    .then((receipt) => {
      if (phase === "started") receiversReached = receipt?.receivers;
      if (phase === "started" && receiversReached === 0)
        notice(
          "Your selected devices are offline. Save the words to history so they can pick them up later.",
        );
    })
    .catch((error) => {
      liveFailure = true;
      notice(error.message);
    });
  return sending;
}
function segments(content) {
  // Keep every WebSocket frame under the API Gateway limit without cutting Unicode code points.
  const parts = [];
  let part = "",
    bytes = 0;
  for (const char of content) {
    const size = new TextEncoder().encode(char).length;
    if (bytes + size > 15000) {
      parts.push(part);
      part = "";
      bytes = 0;
    }
    part += char;
    bytes += size;
  }
  if (part) parts.push(part);
  return parts;
}
async function saveDraft(automatic = false) {
  if (saving || recording || deleted || $("incognito").checked) return;
  const content = $("draft").value;
  if (!content.trim() && !rawText.trim()) {
    if (!automatic) notice("Speak or type a thought first.");
    return;
  }
  if (saved) {
    if (!automatic) notice("Already saved to history.");
    return;
  }
  if (automatic && !$("retain").checked) {
    recovery(
      "History is off. Copy or download these words before closing this page.",
    );
    return;
  }
  saving = true;
  $("draft-state").textContent = "Saving…";
  buttonState();
  try {
    await persist().catch(() => {});
    pendingSave ??= {
      id: recordingId,
      rawText: rawText || content,
      text: content,
      title:
        $("note-title").value || content.trim().split("\n")[0].slice(0, 80),
      mode: $("task-mode").value,
      deliveredText: noteState?.deliveredText ?? content,
      rewrites: noteState?.rewrites ?? [],
      segments: noteState?.segments ?? [],
      baseRevision: noteState?.revision ?? 0,
      operationId: crypto.randomUUID(),
    };
    await persist().catch(() => {});
    const result = await api(
      noteState ? `/history/${recordingId}` : "/history",
      noteState ? "PUT" : "POST",
      pendingSave,
    );
    if (deleted) return;
    noteState = result;
    rawText = result.rawText;
    pendingSave = undefined;
    $("original-text").textContent = rawText;
    $("note-title").value = result.title;
    $("workspace-title").textContent = "Workspace";
    saved = true;
    audio = undefined;
    await draftStorage(auth.owner, null);
    $("recovery").hidden = true;
    $("draft-state").textContent = "Saved to history";
    notice("Saved. Your words are available on all your devices.");
    await loadHistory();
  } catch (error) {
    if (error.status === 409) {
      conflictNote = await api(`/history/${recordingId}`).catch(() => null);
      if (conflictNote) {
        $("conflict-text").value = conflictNote.text;
        $("conflict-dialog").showModal();
      }
    }
    if (error.status === 410) {
      deleted = true;
      pendingSave = undefined;
    }
    recovery(error.message);
    notice(error.message);
  } finally {
    saving = false;
    buttonState();
  }
}
async function startRecording(continuing = false) {
  if (recording || stopping || saving) return;
  if (
    !window.AudioContext ||
    !window.AudioWorkletNode ||
    !window.MediaRecorder ||
    !navigator.mediaDevices?.getUserMedia
  ) {
    notice(
      "This browser cannot record live audio. Send text here, or open Mluva in current Safari or Chrome.",
    );
    return;
  }
  const command = $("task-mode").value === "command";
  if (command && !$("draft").value.trim()) {
    notice(
      "For Command, paste the text to change in the editor, then record your instruction.",
    );
    return;
  }
  if (
    !command &&
    !saved &&
    $("draft").value &&
    !(await confirm(
      "Start a fresh recording?",
      "Download or save the current draft first if you want to keep it. Starting will clear it.",
    ))
  )
    return;
  commandSource = command ? $("draft").value : null;
  continuation = continuing
    ? {
        note: structuredClone(noteState),
        text: $("draft").value,
        segmentId: crypto.randomUUID(),
        raw: "",
      }
    : null;
  if (!continuing) {
    recordingId = crypto.randomUUID();
    noteState = null;
  }
  pendingSave = undefined;
  deleted = false;
  editGeneration++;
  rewriteGeneration++;
  if (!continuing) {
    $("note-title").value = "";
    $("original-text").textContent = "";
    rawText = "";
  }
  audio = undefined;
  saved = false;
  if (!continuing && !command) $("draft").value = "";
  $("partial").hidden = true;
  $("recovery").hidden = true;
  recording = true;
  notice(command ? "Speak an instruction for the text in the editor." : "");
  started = Date.now();
  $("record").dataset.recording = "true";
  $("record-label").textContent = "Stop";
  $("record").setAttribute("aria-label", "Stop recording");
  $("draft-state").textContent = "Listening";
  buttonState();
  // API work and audio permission begin together; only the temporary token reaches this browser.
  const token = api("/speech-token", "POST", { deviceId, consent: true }).then(
    (result) => result.token,
  );
  token.catch(() => {});
  let partialTimer, latestPartial;
  speech = new CloudSpeech({
    token,
    endpoint: config.dev ? config.speechSocket : undefined,
    onPartial: (value) => {
      latestPartial = value;
      $("partial").textContent = value;
      $("partial").hidden = !value;
      // Coalesce speculative updates; committed segments are never coalesced away.
      if (!partialTimer)
        partialTimer = setTimeout(() => {
          partialTimer = undefined;
          if (recording) publish("partial", latestPartial.slice(-4000));
        }, 120);
    },
    onSegment: (value) => {
      if (!value) return;
      const observed = performance.now();
      if (continuation) {
        continuation.raw += `${continuation.raw ? "\n" : ""}${value}`;
        $("draft").value = `${continuation.text}\n\n${continuation.raw}`;
        noteState.segments = [
          ...(noteState.segments ?? []).filter(
            (s) => s.id !== continuation.segmentId,
          ),
          { id: continuation.segmentId, rawText: continuation.raw },
        ];
      } else {
        rawText += `${rawText ? "\n" : ""}${value}`;
        $("draft").value = commandSource ?? rawText;
        if (commandSource !== null) $("instruction").value = rawText;
      }
      renderOriginal();
      $("partial").hidden = true;
      requestAnimationFrame(() => {
        samples.push(performance.now() - observed);
        if (samples.length > 200) samples.shift();
      });
      for (const segment of segments(value)) publish("segment", segment);
      persist().catch(() => {});
      if (
        new TextEncoder().encode($("draft").value).length > 85000 &&
        recording &&
        !stopping
      ) {
        notice(
          "This thought reached the text limit. Saving it before you continue.",
        );
        stopRecording();
      }
    },
    onFailure: (error) => {
      notice(error.message);
      if (recording && !stopping) stopRecording(false);
    },
  });
  try {
    beginLive().catch(() => {});
    await speech.start();
    if ("wakeLock" in navigator)
      wakeLock = await navigator.wakeLock.request("screen").catch(() => null);
    ticker = setInterval(() => {
      const elapsed = Math.floor((Date.now() - started) / 1000);
      $("timer").textContent =
        `${Math.floor(elapsed / 60)}:${String(elapsed % 60).padStart(2, "0")}`;
      if (elapsed >= 7200) stopRecording();
    }, 500);
    await refreshDevices();
  } catch (error) {
    notice(error.message);
    await stopRecording(false);
  }
}
async function stopRecording(commit = true) {
  if (!recording || stopping) return;
  stopping = true;
  buttonState();
  clearInterval(ticker);
  $("draft-state").textContent = "Finishing words";
  let failed = false;
  try {
    await speech.finish(commit);
  } catch (error) {
    failed = true;
    notice(error.message);
  }
  audio = speech.audio;
  await wakeLock?.release().catch(() => {});
  wakeLock = undefined;
  recording = false;
  stopping = false;
  $("record").dataset.recording = "false";
  $("record-label").textContent = "Record";
  $("record").setAttribute("aria-label", "Start recording");
  $("partial").hidden = true;
  $("draft-state").textContent =
    failed || !commit ? "Settled words · review before saving" : "Ready";
  publish("stopped");
  await persist().catch(() => {});
  recovery(
    failed || !commit
      ? "Speech was interrupted. Download the audio before leaving; the draft contains settled words only."
      : "Your recording is ready to save or download.",
  );
  buttonState();
  if (commit && !failed && $("task-mode").value === "dictation")
    await saveDraft(true);
  else if (commit && !failed) {
    $("rewrite-panel").open = true;
    if ($("task-mode").value === "scratchpad") {
      $("template").value = "notes";
      chooseTemplate();
    }
    notice(
      $("task-mode").value === "command"
        ? "Instruction captured. Rewrite the editor text, review the preview, then keep it explicitly."
        : "Review your notes. Save them or create a structured rewrite before keeping it.",
    );
  }
  await refreshDevices().catch(() => {});
}

async function beginSignIn(createAccount = false) {
  if ($("signin").disabled) return;
  $("signin").disabled = true;
  $("create-account").disabled = true;
  $("signin-error").hidden = true;
  $("welcome").setAttribute("aria-busy", "true");
  try {
    if (!config.dev && !navigator.onLine)
      throw new Error("You’re offline. Reconnect, then try signing in again.");
    if (config.dev) {
      const payload = await (
        await fetch("/dev/session", { method: "POST" })
      ).json();
      sessionStorage.setItem("mluva-auth", JSON.stringify(payload));
      location.reload();
    } else await signIn(config, $("remember-login").checked, createAccount);
  } catch (error) {
    $("signin-error").textContent = error.message;
    $("signin-error").hidden = false;
    resetSignIn();
  }
}
function resetSignIn() {
  $("signin").disabled = false;
  $("create-account").disabled = false;
  $("welcome").removeAttribute("aria-busy");
}
$("signin").onclick = () => beginSignIn();
$("create-account").onclick = () => beginSignIn(true);
window.addEventListener("pageshow", resetSignIn);
$("signout").onclick = async () => {
  if (signingOut) return;
  signingOut = true;
  editGeneration++;
  rewriteGeneration++;
  clearTimeout(storageTimer);
  wire?.close();
  if (recording) await stopRecording(false);
  await persist().catch(() => {});
  $("workspace").hidden = true;
  $("signout").disabled = true;
  if (deviceId)
    await api(`/devices/${deviceId}`, "DELETE").catch(() =>
      notice(
        "Signed out here; remove this offline device from another device to revoke it.",
      ),
    );
  localStorage.removeItem(`mluva-device:${auth.owner}`);
  await signOut(config);
};
$("record").onclick = () => {
  if (recording) stopRecording();
  else {
    continueRequested = false;
    $("speech-dialog").showModal();
  }
};
$("speech-cancel").onclick = () => $("speech-dialog").close();
$("speech-start").onclick = () => {
  $("speech-dialog").close();
  startRecording(continueRequested);
};
$("save").onclick = () => saveDraft();
$("retry").onclick = () => saveDraft();
$("send").onclick = async () => {
  if (!$("draft").value.trim()) {
    notice("Type a thought first.");
    return;
  }
  sendingText = true;
  buttonState();
  await beginLive();
  for (const segment of segments($("draft").value))
    await publish("segment", segment);
  await publish("stopped");
  sendingText = false;
  buttonState();
  if (!liveFailure && receiversReached > 0)
    notice("Sent to your connected devices. Save it to keep it in history.");
};
$("draft").addEventListener("input", () => {
  if (saved) {
    saved = false;
  }
  editGeneration++;
  $("draft-state").textContent = "Unsaved draft";
  clearTimeout(storageTimer);
  storageTimer = setTimeout(() => persist(), 250);
});
$("retain").onchange = async () => {
  if (
    !$("retain").checked &&
    !(await confirm(
      "Keep new recovery in memory?",
      "Unsaved text and audio will be lost when this page closes. Saved cloud history remains.",
    ))
  ) {
    $("retain").checked = true;
    return;
  }
  await persist().catch(() => {});
  if (!$("recovery").hidden) recovery();
};
$("clear").onclick = async () => {
  if (
    ($("draft").value || audio) &&
    !(await confirm(
      "Clear this draft?",
      "Unsent words and local audio will be removed from this device. Saved cloud history remains.",
    ))
  )
    return;
  clearTimeout(storageTimer);
  $("draft").value = "";
  rawText = "";
  noteState = null;
  pendingSave = undefined;
  deleted = false;
  editGeneration++;
  rewriteGeneration++;
  $("note-title").value = "";
  $("original-text").textContent = "";
  audio = undefined;
  saved = false;
  recordingId = crypto.randomUUID();
  $("recovery").hidden = true;
  $("draft-state").textContent = "Speak or type";
  await persist().catch(() => {});
};
$("export-draft").onclick = () =>
  download(
    `mluva-${recordingId}.md`,
    `${$("draft").value}\n\n## Original words\n\n${rawText || $("draft").value}\n`,
  );
$("diagnostics").onclick = () =>
  download(
    "mluva-connection-diagnostics.json",
    JSON.stringify(
      {
        measuredAt: new Date().toISOString(),
        environment: config.dev ? "local-synthetic" : "hosted",
        audioFrameMs: 50,
        audioRelay: false,
        committedEventToAnimationFrameMs: samples,
        liveUpdateAcknowledgementRttMs: wire?.rtts ?? [],
        oneWayNetworkLatency: "not measured",
        sub100MsExtraSpeechDelay: "not certified",
      },
      null,
      2,
    ),
    "application/json",
  );
$("copy-link").onclick = () =>
  copy(
    location.origin,
    "App link copied. Open it on another device and use the same account.",
  );
$("refresh-devices").onclick = () =>
  refreshDevices().catch((error) => notice(error.message));
$("connection").onclick = () => wire?.connect();
$("search").oninput = renderHistory;
$("more").onclick = () =>
  loadHistory(true).catch((error) => notice(error.message));
window.addEventListener("online", () => wire?.connect());
window.addEventListener("pageshow", (event) => {
  if (event.persisted && wire) {
    wire.stopped = false;
    wire.connect();
  }
});
window.addEventListener("beforeunload", (event) => {
  if (!signingOut && (recording || (!saved && $("draft").value))) {
    event.preventDefault();
    event.returnValue = "";
  }
});
window.addEventListener("pagehide", () => {
  if (recording) speech?.finish(false).catch(() => {});
  wire?.close();
});
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible" && wire) {
    wire.connect();
    loadHistory().catch(() => {});
    if (recording && !wakeLock && "wakeLock" in navigator)
      navigator.wakeLock
        .request("screen")
        .then((lock) => {
          wakeLock = lock;
        })
        .catch(() => {});
  }
});

let installPrompt;
window.addEventListener("beforeinstallprompt", (event) => {
  event.preventDefault();
  installPrompt = event;
});
window.addEventListener("appinstalled", () => {
  $("install").hidden = true;
});
$("install").hidden =
  matchMedia("(display-mode: standalone)").matches ||
  navigator.standalone === true;
$("install").onclick = async () => {
  if (installPrompt) {
    await installPrompt.prompt();
    installPrompt = undefined;
    return;
  }
  $("install-help").textContent = /iPhone|iPad|iPod/.test(navigator.userAgent)
    ? "In Safari, choose Share → Add to Home Screen, then Add. Sign in again if the installed app asks."
    : "Open your browser menu and choose Install app or Add to Home screen. On iPhone and iPad, use Safari’s Share menu.";
  $("install-dialog").showModal();
};
if ("serviceWorker" in navigator)
  navigator.serviceWorker
    .register("/sw.js", { updateViaCache: "none" })
    .catch(() => {});

async function connectWire() {
  wire?.close();
  wire = new DeviceWire({
    config,
    api,
    deviceId,
    onEvent: event,
    onState: (online, label) => {
      state(online, label);
      buttonState();
    },
  });
  await wire.connect();
}

if (!config.api || !config.websocket || !config.clientId || !config.login)
  $("configuration").hidden = false;
else if (!auth) {
  document.body.classList.add("auth-page");
  document.title = "Mluva · Sign in";
  $("welcome").hidden = false;
  if (config.dev) {
    $("signin").textContent = "Open local test workspace";
    $("create-account").hidden = true;
    const warning = document.createElement("p");
    warning.className = "small";
    warning.textContent =
      "Local preview · synthetic speech · no real account or cloud writes";
    document.querySelector(".signin-form").append(warning);
  }
} else {
  $("workspace").hidden = false;
  $("signout").hidden = false;
  $("show-settings").hidden = false;
  $("show-history").hidden = false;
  try {
    deviceId =
      localStorage.getItem(`mluva-device:${auth.owner}`) ?? crypto.randomUUID();
    const exists = await refreshDevices();
    if (!exists) {
      deviceId = crypto.randomUUID();
      $("name").value = /Mobi|Android|iPhone/.test(navigator.userAgent)
        ? "My phone"
        : "My laptop";
      $("kind").value = /Mobi|Android|iPhone/.test(navigator.userAgent)
        ? "phone"
        : "laptop";
      $("device-dialog").showModal();
      $("device-dialog").addEventListener("cancel", (event) =>
        event.preventDefault(),
      );
      await new Promise((resolve) => {
        $("device-form").onsubmit = async (event) => {
          event.preventDefault();
          const button = $("device-form").querySelector("button");
          button.disabled = true;
          try {
            await api("/devices", "POST", {
              id: deviceId,
              name: $("name").value,
              kind: $("kind").value,
            });
            localStorage.setItem(`mluva-device:${auth.owner}`, deviceId);
            $("device-dialog").close();
            resolve();
          } catch (error) {
            $("device-error").textContent = error.message;
          } finally {
            button.disabled = false;
          }
        };
      });
    }
    const pending = await draftStorage(auth.owner);
    if (pending) {
      noteState = pending.noteState ?? null;
      pendingSave = pending.pendingSave;
      $("note-title").value = pending.title ?? "";
      $("task-mode").value = pending.mode ?? "dictation";
      recordingId = pending.id;
      rawText = pending.rawText;
      $("draft").value = pending.text;
      audio = pending.audio;
      recovery("An unfinished draft was recovered. Review it before saving.");
      $("draft-state").textContent = "Recovered draft";
    }
    await refreshDevices();
    await loadHistory();
    await connectWire();
    buttonState();
    if (config.dev)
      notice(
        "Local preview uses synthetic speech and in-memory accounts. No cloud resources or real microphone are required for automated tests.",
      );
  } catch (error) {
    state(false, "Offline");
    notice(error.message);
  }
}

function markdown(note) {
  return `# ${note.title}\n\n## Working text\n\n${note.text}\n\n## Original recognition\n\n${note.rawText}\n\n## Delivered text\n\n${note.deliveredText ?? note.text}\n${(note.rewrites ?? []).map((r) => `\n## Rewrite · ${r.model}\n\n${r.instruction}\n\n${r.text}\n`).join("")}${(note.segments ?? []).map((r) => `\n## Original segment\n\n${r.rawText}\n`).join("")}`;
}
function renderOriginal() {
  $("original-text").textContent =
    rawText +
    (noteState?.segments ?? [])
      .map((s, i) => `\n\nRecording ${i + 2}\n${s.rawText}`)
      .join("");
}
function showRewrites() {
  $("rewrites").replaceChildren();
  for (const r of noteState?.rewrites ?? []) {
    const details = document.createElement("details"),
      title = document.createElement("summary"),
      text = document.createElement("pre"),
      copyButton = document.createElement("button");
    title.textContent = `Rewrite · ${r.model}`;
    text.textContent = r.text;
    copyButton.textContent = "Copy rewrite";
    copyButton.className = "quiet";
    copyButton.onclick = () => copy(r.text);
    details.append(title, text, copyButton);
    $("rewrites").append(details);
  }
}
async function openNote(note) {
  if (recording || saving) return;
  if (
    !saved &&
    $("draft").value &&
    !(await confirm(
      "Open another conversation?",
      "Save or export your unsaved draft first to keep it.",
    ))
  )
    return;
  noteState = structuredClone(note);
  recordingId = note.id;
  rawText = note.rawText;
  saved = true;
  deleted = false;
  pendingSave = undefined;
  editGeneration++;
  rewriteGeneration++;
  $("draft").value = note.text;
  $("note-title").value = note.title;
  renderOriginal();
  $("workspace-title").textContent = "Workspace";
  $("task-mode").value = note.mode ?? "dictation";
  $("draft-state").textContent = `Saved · revision ${note.revision}`;
  $("recovery").hidden = true;
  $("rewrite-result").hidden = true;
  document.body.classList.remove("history-open");
  showRewrites();
  renderHistory();
  buttonState();
}
$("new-note").onclick = async () => {
  if (
    !saved &&
    $("draft").value &&
    !(await confirm(
      "Start a new note?",
      "Save or export this draft first if you want to keep it.",
    ))
  )
    return;
  noteState = null;
  pendingSave = undefined;
  recordingId = crypto.randomUUID();
  rawText = "";
  saved = false;
  deleted = false;
  audio = undefined;
  editGeneration++;
  rewriteGeneration++;
  $("draft").value = "";
  $("note-title").value = "";
  $("original-text").textContent = "";
  $("workspace-title").textContent = "New conversation";
  $("rewrite-result").hidden = true;
  $("recovery").hidden = true;
  showRewrites();
  await persist();
  buttonState();
};
$("note-title").oninput = () => {
  saved = false;
  editGeneration++;
  persist();
};
$("task-mode").onchange = () => {
  saved = false;
  editGeneration++;
  persist();
  if ($("task-mode").value === "command")
    notice(
      "Command: paste the text to change, then record or type a rewrite instruction. Nothing is inserted into another app.",
    );
};
$("copy-draft").onclick = () => copy($("draft").value);
$("export-json").onclick = () =>
  download(
    `mluva-${recordingId}.json`,
    JSON.stringify(
      {
        ...noteState,
        id: recordingId,
        rawText,
        text: $("draft").value,
        title: $("note-title").value,
      },
      null,
      2,
    ),
    "application/json",
  );
$("cancel-recording").onclick = async () => {
  rewriteGeneration++;
  await stopRecording(false);
  if (continuation) {
    const before = continuation.note;
    continuation = null;
    saved = true;
    await openNote(before);
  } else {
    rawText = "";
    $("draft").value = "";
  }
  audio = undefined;
  $("recovery").hidden = true;
  await persist();
  notice("Recording cancelled. No note was saved.");
};
$("continue-recording").onclick = () => {
  continueRequested = true;
  $("speech-dialog").showModal();
};
$("merge-note").onclick = async () => {
  const other = historyEntries.find((e) => e.id === $("merge-with").value);
  if (!other || !noteState || !saved) return;
  if (
    !(await confirm(
      "Merge these conversations?",
      "The other conversation becomes an original segment of this note. This change syncs to your devices.",
    ))
  )
    return;
  const current = structuredClone(noteState);
  const merged = {
    ...current,
    text: `${current.text}\n\n${other.text}`,
    deliveredText: `${current.deliveredText}\n\n${other.deliveredText}`,
    rewrites: [...current.rewrites, ...other.rewrites],
    segments: [
      ...current.segments,
      { id: other.id, rawText: other.rawText },
      ...other.segments,
    ],
  };
  $("draft").value = merged.text;
  noteState = merged;
  pendingSave = {
    ...merged,
    baseRevision: current.revision,
    operationId: crypto.randomUUID(),
    mergedSources: [{ id: other.id, baseRevision: other.revision }],
  };
  saved = false;
  editGeneration++;
  rewriteGeneration++;
  await saveDraft();
  showRewrites();
};
$("conflict-local").onclick = () => {
  noteState = conflictNote;
  pendingSave = undefined;
  $("conflict-dialog").close();
  saveDraft();
};
$("conflict-remote").onclick = () => {
  saved = true;
  $("conflict-dialog").close();
  openNote(conflictNote);
  persist();
};
$("conflict-later").onclick = () => $("conflict-dialog").close();
$("show-settings").onclick = () => $("settings-dialog").showModal();
$("settings-content").append(
  $("device-settings"),
  document.querySelector(".privacy"),
);
$("show-history").onclick = () =>
  document.body.classList.toggle("history-open");
function theme(value) {
  document.documentElement.dataset.theme = value;
}
theme(localStorage.getItem("mluva-theme") ?? "system");
$("theme").value = localStorage.getItem("mluva-theme") ?? "system";
$("theme").onchange = () => {
  localStorage.setItem("mluva-theme", $("theme").value);
  theme($("theme").value);
};
$("incognito").onchange = async () => {
  if (recording || saving) {
    $("incognito").checked = !$("incognito").checked;
    return;
  }
  buttonState();
  if ($("incognito").checked) {
    wire?.close();
    incoming.clear();
    renderIncoming();
    state(false, "Incognito");
    $("retain").checked = false;
    rewriteGeneration++;
    await draftStorage(auth.owner, null);
    notice(
      "Incognito: new words stay in memory. No account sync, saved history, rewrites or recovery. Speech still uses ElevenLabs.",
    );
  } else {
    await connectWire();
    await loadHistory().catch((e) => notice(e.message));
  }
  buttonState();
};
const templateKey = () => `mluva-templates:${auth?.owner ?? "signed-out"}`;
let savedTemplates;
try {
  savedTemplates = JSON.parse(localStorage.getItem(templateKey()) ?? "{}");
} catch {
  savedTemplates = {};
}
function chooseTemplate() {
  $("instruction").value =
    savedTemplates[$("template").value] ?? templates[$("template").value];
}
$("template").onchange = chooseTemplate;
chooseTemplate();
$("save-template").onclick = () => {
  if ($("incognito").checked) return;
  savedTemplates[$("template").value] = $("instruction").value;
  localStorage.setItem(templateKey(), JSON.stringify(savedTemplates));
  notice("Template saved on this device.");
};
$("rewrite").onclick = async () => {
  if ($("incognito").checked || recording) return;
  const generation = ++rewriteGeneration,
    revision = editGeneration,
    id = recordingId,
    source = $("draft").value,
    instruction = $("instruction").value;
  if (!source.trim() || !instruction.trim()) {
    notice("Add text and an instruction first.");
    return;
  }
  $("rewrite-cancel").hidden = false;
  $("rewrite").disabled = true;
  $("rewrite-result").hidden = true;
  try {
    const result = await api("/rewrite", "POST", {
      text: source,
      instruction,
      consent: true,
    });
    if (
      generation !== rewriteGeneration ||
      revision !== editGeneration ||
      id !== recordingId ||
      deleted
    ) {
      notice("The source changed. The old rewrite was discarded.");
      return;
    }
    rewriteResult = {
      id: new Date().toISOString(),
      createdAt: new Date().toISOString(),
      instruction,
      text: result.text,
      model: result.model,
      sourceRevision: revision,
    };
    rewriteResult.id = rewriteResult.createdAt;
    $("rewrite-text").value = result.text;
    $("rewrite-result").hidden = false;
  } catch (e) {
    if (generation === rewriteGeneration) notice(e.message);
  } finally {
    if (generation === rewriteGeneration) {
      $("rewrite-cancel").hidden = true;
      buttonState();
    }
  }
};
$("rewrite-cancel").onclick = () => {
  rewriteGeneration++;
  $("rewrite-cancel").hidden = true;
  $("rewrite-result").hidden = true;
  buttonState();
  notice("Rewrite cancelled. The source is unchanged.");
};
$("apply-rewrite").onclick = async () => {
  if (!rewriteResult || rewriteResult.sourceRevision !== editGeneration) {
    notice("The source changed. Rewrite again before applying.");
    return;
  }
  if (!noteState) await saveDraft();
  if (!noteState || deleted) return;
  const reply = { ...rewriteResult };
  delete reply.sourceRevision;
  reply.text = $("rewrite-text").value;
  noteState.rewrites = [...(noteState.rewrites ?? []), reply];
  noteState.deliveredText = reply.text;
  saved = false;
  pendingSave = undefined;
  await saveDraft();
  showRewrites();
  $("rewrite-result").hidden = true;
};
