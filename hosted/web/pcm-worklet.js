// Downsample the microphone to Mluva's 16 kHz mono PCM16 contract.
class MluvaPcm extends AudioWorkletProcessor {
  constructor() {
    super();
    this.samples = new Int16Array(800); // 50 ms audio frames, directly to the speech provider
    this.used = 0;
    this.sum = 0;
    this.weight = 0;
    this.remaining = sampleRate / 16000;
    this.active = true;
    this.port.onmessage = ({ data }) => {
      if (data === "flush") {
        this.active = false;
        this.emit();
        this.port.postMessage({ flushed: true });
      }
    };
  }
  emit() {
    if (!this.used) return;
    const bytes = new ArrayBuffer(this.used * 2),
      view = new DataView(bytes);
    for (let i = 0; i < this.used; i++)
      view.setInt16(i * 2, this.samples[i], true);
    this.port.postMessage({ pcm: bytes }, [bytes]);
    this.used = 0;
  }
  process(inputs) {
    if (!this.active) return true;
    const channel = inputs[0]?.[0];
    if (!channel) return true;
    for (const input of channel) {
      let fraction = 1;
      while (fraction > 0) {
        const take = Math.min(fraction, this.remaining);
        this.sum += input * take;
        this.weight += take;
        this.remaining -= take;
        fraction -= take;
        if (this.remaining < 1e-8) {
          const value = Math.max(-1, Math.min(1, this.sum / this.weight));
          this.samples[this.used++] = Math.round(
            value * (value < 0 ? 32768 : 32767),
          );
          this.sum = this.weight = 0;
          this.remaining = sampleRate / 16000;
          if (this.used === this.samples.length) this.emit();
        }
      }
    }
    return true;
  }
}
registerProcessor("mluva-pcm", MluvaPcm);
