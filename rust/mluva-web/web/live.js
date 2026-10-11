"use strict";

// The desktop owns recognition, finalization and delivery. This sends PCM only.
class LiveMicrophone {
  constructor(identifier) {
    this.identifier = identifier;
    this.context = new AudioContext();
    this.resumed = this.context.resume(); // directly in the Record gesture
    this.sequence = 0;
    this.queued = 0;
    this.sent = Promise.resolve();
    this.failure = "";
    this.pcStopped = false;
  }
  async start(stream) {
    await this.resumed;
    await this.context.audioWorklet.addModule("/pcm-worklet.js", { credentials: "same-origin" });
    let reply = await api(`/api/live/${this.identifier}`, { method: "POST" });
    const deadline = Date.now() + 90_000;
    while (["starting", "preparing"].includes(reply.phase)) {
      if (Date.now() > deadline) throw new Error("PC recognition is taking too long to start. Try again.");
      status.textContent = "Preparing Mluva’s recording widget on your PC…";
      await new Promise((resolve) => setTimeout(resolve, 300));
      reply = await api(`/api/live/${this.identifier}`, reply.phase === "starting" ? { method: "POST" } : {});
    }
    this.incognito = reply.incognito;
    if (reply.phase !== "recording") throw new Error(reply.message || "The PC recording could not start.");
    this.node = new AudioWorkletNode(this.context, "mluva-pcm", { channelCount: 1, channelCountMode: "explicit" });
    this.node.port.onmessage = ({ data }) => {
      if (data.pcm) this.enqueue(data.pcm);
      if (data.flushed) this.flushed?.();
    };
    this.node.addEventListener("processorerror", () => this.fail("The phone audio stream stopped. Your local recording is available below."));
    this.source = this.context.createMediaStreamSource(stream);
    this.source.connect(this.node);
    this.node.connect(this.context.destination); // worklet emits silence; no microphone feedback
    this.poll = setInterval(async () => {
      if (this.polling || this.failure || this.pcStopped) return;
      this.polling = true;
      try { this.observe(await api(`/api/live/${this.identifier}`)); }
      catch (error) { this.fail(error.message); }
      finally { this.polling = false; }
    }, 1000);
  }
  observe(reply) {
    if (reply.phase === "recording") {
      el("live-preview").textContent = reply.text || "Listening on your PC…";
    } else if (["processing", "completed"].includes(reply.phase)) {
      this.pcStopped = true;
      if (recorder?.state === "recording") stop();
    } else if (["failed", "cancelled"].includes(reply.phase)) this.fail(reply.message || "Desktop recording ended. Keep the audio to retry.");
  }
  fail(message) {
    if (this.failure) return;
    this.failure = message;
    if (recorder?.state === "recording") stop();
  }
  enqueue(pcm) {
    if (this.failure || this.pcStopped) return;
    if (++this.queued > 16) { this.fail("Connection is too slow for live audio. Your phone recording is kept for Retry."); return; }
    const sequence = this.sequence++;
    this.sent = this.sent.then(async () => {
      if (this.failure || this.pcStopped) return;
      // Retrying the same sequence is safe if an acknowledgement was lost.
      let reply;
      for (let attempt = 0; attempt < 2; attempt++) {
        try {
          reply = await api(`/api/live/${this.identifier}/chunks/${sequence}`, {
            method: "POST", headers: { "Content-Type": "application/octet-stream" }, body: pcm,
          });
          break;
        } catch (error) { if (attempt) throw error; }
      }
      this.observe(reply);
      if (!this.pcStopped && reply.sequence !== sequence + 1) throw new Error("PC audio acknowledgement was incomplete. Keep the phone recording.");
    }).catch((error) => this.fail(error.message)).finally(() => { this.queued--; });
  }
  async closeAudio() {
    clearInterval(this.poll);
    this.source?.disconnect();
    if (this.node && !this.failure && !this.pcStopped) {
      await new Promise((resolve, reject) => {
        const timeout = setTimeout(() => reject(new Error("Phone audio could not finish. Keep the local recording.")), 2000);
        this.flushed = () => { clearTimeout(timeout); resolve(); };
        this.node.port.postMessage("flush");
      });
    }
    this.node?.disconnect();
    await this.context.close().catch(() => {});
    await this.sent;
  }
  async cancel() {
    clearInterval(this.poll);
    this.source?.disconnect();
    this.node?.disconnect();
    await this.context.close().catch(() => {});
    await api(`/api/live/${this.identifier}/cancel`, { method: "POST" }).catch(() => {});
  }
  async finish() {
    await this.closeAudio();
    if (this.failure) { await this.cancel(); throw new Error(this.failure); }
    return api(`/api/live/${this.identifier}/stop`, {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ sequence: this.sequence }),
    });
  }
}
