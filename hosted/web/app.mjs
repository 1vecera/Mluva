import { account, accessToken, signIn, signOut } from "./auth.mjs";
import { draftStorage } from "./storage.mjs";
import { DeviceWire } from "./wire.mjs";
import { CloudSpeech } from "./speech.mjs";

const $ = (id) => document.getElementById(id);
const config = await (
  await fetch("/config.json", { cache: "no-store" })
).json();
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
    wire.stopped ||
    !devices.some((d) => d.id === deviceId) ||
    !config.speechEnabled;
  $("send").disabled =
    recording ||
    saving ||
    sendingText ||
    !wire ||
    wire.socket?.readyState !== WebSocket.OPEN;
  $("save").disabled = recording || saving || stopping;
  $("clear").disabled = recording || saving || stopping;
  $("draft").readOnly = recording || saving || stopping;
  $("retain").disabled = recording || saving || stopping;
}
async function api(path, method = "GET", body) {
  const token = await accessToken(config);
  const reply = await fetch(`${config.api}${path}`, {
    method,
    headers: {
      authorization: `Bearer ${token}`,
      ...(body ? { "content-type": "application/json" } : {}),
    },
    ...(body ? { body: JSON.stringify(body) } : {}),
    cache: "no-store",
    signal: AbortSignal.timeout(15000),
  });
  let result;
  try {
    result = await reply.json();
  } catch {
    throw new Error(
      "The host did not respond. Your words stay here; reconnect to retry.",
    );
  }
  if (!reply.ok)
    throw new Error(
      result.message ?? "Connection failed. Keep your draft and retry.",
    );
  return result;
}
function persist() {
  if (!auth) return Promise.resolve();
  const value =
    $("retain").checked && ($("draft").value || audio)
      ? { id: recordingId, rawText, text: $("draft").value, audio }
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
  const current = devices.find((device) => device.id === deviceId);
  $("device-name").textContent = current?.name ?? "this device";
  $("record-hint").textContent = config.speechEnabled
    ? "Keep this screen open while recording."
    : "Cloud speech is awaiting host activation. Send text below, or use native Mluva.";
  buttonState();
  return Boolean(current);
}
function renderHistory() {
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
      download(
        `mluva-${entry.id}.md`,
        `# ${entry.title}\n\n${entry.text}\n\n## Original words\n\n${entry.rawText}\n`,
      );
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
    actions.append(copyButton, exportButton, remove);
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
  } else if (message.type === "history.changed")
    loadHistory().catch((error) => notice(error.message));
  else if (
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
      liveFailure = true;
      notice(
        "Live connection interrupted. Recording stays on this device; save to history after reconnecting.",
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
  if (liveFailure) return Promise.resolve();
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
  if (saving || recording) return;
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
  buttonState();
  try {
    await persist().catch(() => {});
    await api("/history", "POST", {
      id: recordingId,
      deviceId,
      rawText: rawText || content,
      text: content,
    });
    saved = true;
    audio = undefined;
    await draftStorage(auth.owner, null);
    $("recovery").hidden = true;
    $("draft-state").textContent = "Saved to history";
    notice("Saved. Your words are available on all your devices.");
    await loadHistory();
  } catch (error) {
    recovery(error.message);
    notice(error.message);
  } finally {
    saving = false;
    buttonState();
  }
}
async function startRecording() {
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
  if (
    !saved &&
    $("draft").value &&
    !(await confirm(
      "Start a fresh recording?",
      "Download or save the current draft first if you want to keep it. Starting will clear it.",
    ))
  )
    return;
  recordingId = crypto.randomUUID();
  rawText = "";
  audio = undefined;
  saved = false;
  $("draft").value = "";
  $("partial").hidden = true;
  $("recovery").hidden = true;
  recording = true;
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
      rawText += `${rawText ? "\n" : ""}${value}`;
      $("draft").value = rawText;
      $("partial").hidden = true;
      requestAnimationFrame(() => {
        samples.push(performance.now() - observed);
        if (samples.length > 200) samples.shift();
      });
      for (const segment of segments(value)) publish("segment", segment);
      persist().catch(() => {});
      if (
        new TextEncoder().encode(rawText).length > 85000 &&
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
    await beginLive();
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
  await publish("stopped");
  await persist().catch(() => {});
  recovery(
    failed || !commit
      ? "Speech was interrupted. Download the audio before leaving; the draft contains settled words only."
      : "Your recording is ready to save or download.",
  );
  buttonState();
  if (commit && !failed) await saveDraft(true);
  await refreshDevices().catch(() => {});
}

$("signin").onclick = async () => {
  try {
    if (config.dev) {
      const payload = await (
        await fetch("/dev/session", { method: "POST" })
      ).json();
      sessionStorage.setItem("mluva-auth", JSON.stringify(payload));
      location.reload();
    } else await signIn(config);
  } catch (error) {
    notice(error.message);
  }
};
$("signout").onclick = async () => {
  if (recording) await stopRecording(false);
  await persist().catch(() => {});
  wire?.close();
  await signOut(config);
};
$("record").onclick = () => {
  if (recording) stopRecording();
  else {
    $("speech-dialog").showModal();
  }
};
$("speech-cancel").onclick = () => $("speech-dialog").close();
$("speech-start").onclick = () => {
  $("speech-dialog").close();
  startRecording();
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
    recordingId = crypto.randomUUID();
  }
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
  if (recording || (!saved && $("draft").value)) {
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

if (!config.api || !config.websocket || !config.clientId || !config.login)
  $("configuration").hidden = false;
else if (!auth) {
  $("welcome").hidden = false;
  if (config.dev) {
    $("signin").textContent = "Open local test workspace";
    const warning = document.createElement("p");
    warning.className = "small";
    warning.textContent =
      "Local preview · synthetic speech · no real account or cloud writes";
    $("welcome").append(warning);
  }
} else {
  $("workspace").hidden = false;
  $("signout").hidden = false;
  try {
    deviceId =
      localStorage.getItem(`mluva-device:${auth.owner}`) ?? crypto.randomUUID();
    const exists = await refreshDevices();
    if (!exists) {
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
      recordingId = pending.id;
      rawText = pending.rawText;
      $("draft").value = pending.text;
      audio = pending.audio;
      recovery("An unfinished draft was recovered. Review it before saving.");
      $("draft-state").textContent = "Recovered draft";
    }
    await refreshDevices();
    await loadHistory();
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
