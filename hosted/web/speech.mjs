export class CloudSpeech {
  constructor({ token, endpoint, onPartial, onSegment, onFailure }) {
    Object.assign(this, { token, endpoint, onPartial, onSegment, onFailure });
    this.context = new AudioContext();
    this.resumed = this.context.resume(); // called directly from the Record gesture
    this.media = navigator.mediaDevices.getUserMedia({
      audio: {
        channelCount: 1,
        echoCancellation: true,
        noiseSuppression: true,
      },
    });
    this.media.catch(() => {});
    this.closed = false;
    this.hasPartial = false;
    this.bytes = 0;
    this.pcmSinceCommit = 0;
    this.commitsSent = 0;
    this.commitsSeen = 0;
  }
  async start() {
    const media = await this.media;
    this.stream = media;
    if (this.closed) {
      media.getTracks().forEach((track) => track.stop());
      return;
    }
    await this.resumed;
    await this.context.audioWorklet.addModule("/pcm-worklet.js");
    const token = await this.token;
    const url = new URL(
      this.endpoint ?? "wss://api.elevenlabs.io/v1/speech-to-text/realtime",
    );
    url.search = new URLSearchParams({
      model_id: "scribe_v2_realtime",
      token,
      audio_format: "pcm_16000",
      commit_strategy: "manual",
    });
    this.socket = new WebSocket(url);
    await new Promise((resolve, reject) => {
      const timeout = setTimeout(
        () =>
          reject(
            new Error(
              "Speech did not connect. Your microphone will be released.",
            ),
          ),
        12000,
      );
      this.socket.onerror = () => {
        clearTimeout(timeout);
        reject(new Error("Speech connection failed. Try again."));
      };
      this.socket.onclose = () => {
        clearTimeout(timeout);
        if (!this.closed) {
          const error = new Error(
            "Speech disconnected. Settled words and stopped audio are kept here.",
          );
          reject(error);
          this.onFailure(error);
        }
      };
      this.socket.onmessage = ({ data }) => {
        let event;
        try {
          event = JSON.parse(data);
        } catch {
          return;
        }
        if (event.message_type === "session_started") {
          clearTimeout(timeout);
          resolve();
        } else if (event.message_type === "partial_transcript") {
          this.hasPartial = Boolean(event.text);
          this.onPartial(event.text);
        } else if (event.message_type === "committed_transcript") {
          this.hasPartial = false;
          this.commitsSeen++;
          this.onSegment(event.text);
          this.committed?.();
        } else {
          clearTimeout(timeout);
          const error = new Error(
            "Speech could not continue. Keep your draft or audio and try again.",
          );
          reject(error);
          this.onFailure(error);
        }
      };
    });
    if (this.closed) return;
    this.source = this.context.createMediaStreamSource(media);
    this.node = new AudioWorkletNode(this.context, "mluva-pcm", {
      channelCount: 1,
      channelCountMode: "explicit",
    });
    this.node.port.onmessage = ({ data }) => {
      if (data.pcm) {
        if (
          this.socket.readyState !== WebSocket.OPEN ||
          this.socket.bufferedAmount > 160000
        ) {
          this.onFailure(
            new Error(
              "Speech connection is too slow. Keep the recording and retry.",
            ),
          );
          return;
        }
        this.pcmSinceCommit += data.pcm.byteLength;
        const commit = this.pcmSinceCommit >= 5 * 16000 * 2;
        if (commit) {
          this.commitsSent++;
          this.pcmSinceCommit = 0;
        }
        this.socket.send(
          JSON.stringify({
            message_type: "input_audio_chunk",
            audio_base_64: btoa(
              String.fromCharCode(...new Uint8Array(data.pcm)),
            ),
            sample_rate: 16000,
            commit,
          }),
        );
      }
      if (data.flushed) this.flushed?.();
    };
    this.node.onprocessorerror = () =>
      this.onFailure(
        new Error("Audio capture stopped. Your settled words stay here."),
      );
    this.source.connect(this.node);
    this.node.connect(this.context.destination);
    const mime = [
      "audio/webm;codecs=opus",
      "audio/mp4",
      "audio/ogg;codecs=opus",
    ].find((type) => MediaRecorder.isTypeSupported(type));
    this.backup = new MediaRecorder(media, {
      ...(mime ? { mimeType: mime } : {}),
      audioBitsPerSecond: 48000,
    });
    this.chunks = [];
    this.backup.ondataavailable = ({ data }) => {
      if (data.size) {
        this.chunks.push(data);
        this.bytes += data.size;
        if (this.bytes > 89 * 1024 * 1024)
          this.onFailure(
            new Error(
              "Audio backup reached its limit. Keep or download this recording.",
            ),
          );
      }
    };
    this.backup.onerror = () =>
      this.onFailure(
        new Error(
          "Audio backup was interrupted. Your settled words stay here.",
        ),
      );
    this.backup.start(1000);
    media.getTracks().forEach((track) =>
      track.addEventListener("ended", () => {
        if (!this.closed)
          this.onFailure(
            new Error(
              "The microphone was interrupted. Settled words and stopped audio stay here.",
            ),
          );
      }),
    );
  }
  async finish(commit = true) {
    if (this.closed) return;
    // Flush PCM before final commit. Always release microphone even on a provider error.
    let failure;
    try {
      if (this.node && this.socket?.readyState === WebSocket.OPEN && commit) {
        await new Promise((resolve, reject) => {
          const timeout = setTimeout(
            () =>
              reject(
                new Error(
                  "Audio could not finish. Download the backup before leaving.",
                ),
              ),
            2000,
          );
          this.flushed = () => {
            clearTimeout(timeout);
            resolve();
          };
          this.node.port.postMessage("flush");
        });
        // Manual commits have an ordered response count. An older delayed commit
        // cannot settle the final audio that was flushed just above.
        const needsCommit = this.pcmSinceCommit > 0 || this.commitsSent === 0;
        if (needsCommit) this.commitsSent++;
        const target = this.commitsSent;
        await new Promise((resolve, reject) => {
          const timeout = setTimeout(
            () =>
              reject(
                new Error(
                  "Final words were not confirmed. Keep the audio backup; only settled words can be saved.",
                ),
              ),
            8000,
          );
          this.committed = () => {
            if (this.commitsSeen >= target) {
              clearTimeout(timeout);
              resolve();
            }
          };
          if (needsCommit)
            this.socket.send(
              JSON.stringify({
                message_type: "input_audio_chunk",
                audio_base_64: "",
                sample_rate: 16000,
                commit: true,
              }),
            );
          this.committed();
        });
      }
    } catch (error) {
      failure = error;
    } finally {
      this.closed = true;
      if (this.backup?.state === "recording")
        await new Promise((resolve) => {
          this.backup.addEventListener("stop", resolve, { once: true });
          this.backup.stop();
        });
      this.audio = new Blob(this.chunks ?? [], {
        type: this.backup?.mimeType ?? "audio/webm",
      });
      this.source?.disconnect();
      this.node?.disconnect();
      this.socket?.close();
      this.stream?.getTracks().forEach((track) => track.stop());
      if (this.context.state !== "closed") await this.context.close();
    }
    if (failure) throw failure;
  }
}
