"use strict";

const el = (id) => document.getElementById(id);
const record = el("record"), status = el("status"), retry = el("retry");
const recordLabel = el("record-label");
const MAX_RECORDING_SECONDS = 2 * 60 * 60;
const MAX_RECORDING_BYTES = 89 * 1024 * 1024;
let recorder, stream, pending, currentId, timer, started, wakeLock, downloadUrl;
let transferring = false;
let store;
let installPrompt;

function installed() {
  return window.matchMedia("(display-mode: standalone)").matches || navigator.standalone === true;
}

el("install").hidden = installed();
window.addEventListener("beforeinstallprompt", (event) => {
  event.preventDefault();
  installPrompt = event;
});
window.addEventListener("appinstalled", () => { el("install").hidden = true; installPrompt = undefined; });
el("install").addEventListener("click", async () => {
  if (installPrompt) {
    const prompt = installPrompt;
    installPrompt = undefined;
    try { await prompt.prompt(); return; }
    catch { /* The browser-menu instructions remain available. */ }
  }
  const apple = /iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.platform === "MacIntel" && navigator.maxTouchPoints > 1);
  el("install-instructions").textContent = apple
    ? "In Safari, open Share, choose Add to Home Screen, leave Open as Web App on if shown, then Add. Open the Mluva icon next time. You may need to sign in once in the app and allow its microphone."
    : "Open your browser menu and choose Install app or Add to Home screen. Then open the Mluva icon to record. If you don’t see that option, try Chrome. On iPhone or iPad, use Safari’s Share menu → Add to Home Screen.";
  el("install-help").showModal();
});

if ("serviceWorker" in navigator) {
  navigator.serviceWorker.register("/sw.js", { updateViaCache: "none" }).catch(() => {});
}

async function openStore() {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open("mluva-recorder", 1);
    request.onupgradeneeded = () => request.result.createObjectStore("pending");
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(new Error("Local recording recovery is unavailable."));
  });
}

async function savedRecording(value) {
  if (!store) return;
  return new Promise((resolve, reject) => {
    const transaction = store.transaction("pending", value === undefined ? "readonly" : "readwrite");
    const records = transaction.objectStore("pending");
    const request = value === undefined ? records.get("recording") : value === null ? records.delete("recording") : records.put(value, "recording");
    let found;
    request.onsuccess = () => { found = request.result; };
    transaction.oncomplete = () => resolve(found);
    transaction.onerror = () => reject(new Error("Could not save local recording recovery. Download the audio before closing."));
  });
}

async function api(path, options = {}) {
  const response = await fetch(path, { ...options, credentials: "same-origin", cache: "no-store" });
  if (!response.headers.get("content-type")?.includes("application/json")) {
    throw new Error("Sign-in expired or the PC is offline. Keep or download this recording, then refresh to reconnect.");
  }
  const payload = await response.json();
  if (!response.ok) throw new Error(payload.message || "Transfer failed. Keep the recording and retry.");
  return payload;
}

function showAudio() {
  if (downloadUrl) URL.revokeObjectURL(downloadUrl);
  downloadUrl = URL.createObjectURL(pending.audio);
  const link = el("download");
  link.href = downloadUrl;
  const extension = pending.audio.type.includes("mp4") ? "m4a" : pending.audio.type.includes("ogg") ? "ogg" : "webm";
  link.download = `mluva-${pending.identifier}.${extension}`;
  link.hidden = false;
  el("discard").hidden = false;
}

async function stop() {
  record.disabled = true;
  if (recorder?.state === "recording") recorder.stop();
}

async function start() {
  record.disabled = true;
  status.textContent = "Allow microphone access to start.";
  let chunks = [], bytes = 0;
  try {
    stream = await navigator.mediaDevices.getUserMedia({ audio: { channelCount: 1 } });
    const mime = ["audio/webm;codecs=opus", "audio/mp4", "audio/ogg;codecs=opus"].find((type) => MediaRecorder.isTypeSupported(type));
    recorder = new MediaRecorder(stream, { ...(mime ? { mimeType: mime } : {}), audioBitsPerSecond: 48000 });
    const identifier = crypto.randomUUID();
    let recordingError = false;
    recorder.addEventListener("dataavailable", (event) => {
      if (event.data.size) { chunks.push(event.data); bytes += event.data.size; }
      if (bytes >= MAX_RECORDING_BYTES && recorder.state === "recording") stop();
    });
    recorder.addEventListener("error", () => { recordingError = true; });
    recorder.addEventListener("stop", async () => {
      clearInterval(timer);
      stream.getTracks().forEach((track) => track.stop());
      if (wakeLock) { await wakeLock.release().catch(() => {}); wakeLock = undefined; }
      record.dataset.recording = "false";
      recordLabel.textContent = "Record";
      pending = { identifier, audio: new Blob(chunks, { type: recorder.mimeType || "audio/webm" }) };
      showAudio();
      try { await savedRecording(pending); }
      catch (error) {
        status.textContent = error.message;
        retry.hidden = false;
        return;
      }
      if (recordingError) {
        status.textContent = "Recording was interrupted. Download the captured audio or transfer it with Retry.";
        retry.hidden = false;
      } else await transfer();
    }, { once: true });
    stream.getTracks().forEach((track) => track.addEventListener("ended", () => { if (recorder.state === "recording") stop(); }));
    recorder.start(1000);
    started = Date.now();
    el("timer").textContent = "0:00";
    el("timer").hidden = false;
    timer = setInterval(() => {
      const seconds = Math.floor((Date.now() - started) / 1000);
      const minutes = Math.floor(seconds / 60);
      const prefix = seconds >= 3600 ? `${Math.floor(seconds / 3600)}:${String(minutes % 60).padStart(2, "0")}` : String(minutes);
      el("timer").textContent = `${prefix}:${String(seconds % 60).padStart(2, "0")}`;
      if (seconds >= MAX_RECORDING_SECONDS) stop();
    }, 250);
    if (navigator.wakeLock) navigator.wakeLock.request("screen").then((lock) => { wakeLock = lock; }).catch(() => {});
    record.dataset.recording = "true";
    recordLabel.textContent = "Stop";
    record.disabled = false;
    status.textContent = "Recording on this device…";
  } catch (error) {
    stream?.getTracks().forEach((track) => track.stop());
    status.textContent = error.name === "NotAllowedError" ? "Microphone access was denied. Allow it in your browser and try again." : "Could not start recording on this device.";
    record.disabled = false;
  }
}

async function transfer() {
  if (!pending || transferring) return;
  transferring = true;
  retry.hidden = true;
  record.disabled = true;
  el("discard").disabled = true;
  currentId = pending.identifier;
  status.textContent = "Transferring audio to your PC…";
  try {
    let job = await api(`/api/recordings/${pending.identifier}`, {
      method: "POST", headers: { "Content-Type": pending.audio.type || "audio/webm" }, body: pending.audio,
    });
    while (job.phase === "processing") {
      status.textContent = "Transcribing on your PC… You can keep the audio here.";
      await new Promise((resolve) => setTimeout(resolve, 1200));
      job = await api(`/api/recordings/${pending.identifier}`);
    }
    if (job.phase !== "completed") throw new Error(job.message);
    el("text").value = job.text;
    el("result").hidden = false;
    status.textContent = job.message || (job.copied ? "Copied to your PC clipboard." : "Text is ready. Use Copy to PC to copy it again.");
    await savedRecording(null);
    pending = undefined;
    el("download").hidden = true;
    el("discard").hidden = true;
    if (downloadUrl) { URL.revokeObjectURL(downloadUrl); downloadUrl = undefined; }
    record.disabled = false;
    await refreshHistory().catch(() => {});
  } catch (error) {
    status.textContent = error.message;
    retry.hidden = false;
  } finally { transferring = false; el("discard").disabled = false; }
}

async function copyPc(identifier) {
  try {
    const result = await api(`/api/recordings/${identifier}/copy`, { method: "POST" });
    status.textContent = result.copied ? "Copied to your PC clipboard." : "PC clipboard is unavailable. Copy on this device instead.";
  } catch (error) { status.textContent = error.message; }
}

async function refreshHistory() {
  const recordings = await api("/api/recordings");
  const history = el("history");
  history.replaceChildren();
  if (!recordings.length) { const empty = document.createElement("p"); empty.textContent = "No browser recordings yet."; history.append(empty); }
  for (const recording of recordings) {
    const article = document.createElement("article"), text = document.createElement("p"), copy = document.createElement("button");
    text.textContent = recording.text;
    copy.type = "button";
    copy.textContent = "Copy to PC";
    copy.addEventListener("click", () => copyPc(recording.identifier));
    article.append(text, copy);
    history.append(article);
  }
}

record.addEventListener("click", () => recorder?.state === "recording" ? stop() : start());
retry.addEventListener("click", transfer);
el("copy-pc").addEventListener("click", () => copyPc(currentId));
el("copy-device").addEventListener("click", async () => {
  try { await navigator.clipboard.writeText(el("text").value); status.textContent = "Copied on this device."; }
  catch { status.textContent = "Select the text above and copy it manually."; }
});
el("discard").addEventListener("click", async () => {
  if (transferring) return;
  try { await savedRecording(null); }
  catch (error) { status.textContent = error.message; return; }
  pending = undefined;
  retry.hidden = true;
  el("download").hidden = true;
  el("discard").hidden = true;
  if (downloadUrl) { URL.revokeObjectURL(downloadUrl); downloadUrl = undefined; }
  record.disabled = false;
  status.textContent = "Local recording discarded. Ready to record.";
});
window.addEventListener("beforeunload", (event) => {
  if (recorder?.state === "recording" || transferring) { event.preventDefault(); event.returnValue = ""; }
});

(async () => {
  if (!navigator.mediaDevices?.getUserMedia || !window.MediaRecorder) {
    status.textContent = "Recording needs a browser with microphone support and an HTTPS connection.";
    return;
  }
  try {
    store = await openStore();
    pending = await savedRecording();
    if (pending) {
      showAudio(); retry.hidden = false;
      status.textContent = "An unfinished recording is saved on this device. Retry its transfer or download it.";
    } else { record.disabled = false; status.textContent = "Ready to record."; }
    await refreshHistory();
    setInterval(() => { if (!transferring && recorder?.state !== "recording") refreshHistory().catch(() => {}); }, 5000);
  } catch (error) { status.textContent = error.message; }
})();
