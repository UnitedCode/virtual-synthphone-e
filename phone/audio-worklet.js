// AudioWorkletProcessor is a JavaScript-only browser interface. This bridge only
// moves PCM; the original ARM firmware in the Rust worker performs every effect.
class FirmwareAudio extends AudioWorkletProcessor {
  constructor() {
    super();
    this.queue = [];
    this.offset = 0;
    this.queued = 0;
    this.capture = new Float32Array(128);
    this.at = 0;
    this.underruns = 0;
    this.playing = false;
    this.enabled = false;
    this.prebufferFrames = 0;
    this.captureFrames = 128;
    this.maxQueuedFrames = sampleRate * 0.04;
    this.resyncs = 0;
    this.test = null;
    this.testMuted = false;
    this.port.onmessage = ({ data }) => {
      if (data.audio) {
        this.queue.push(data.audio);
        this.queued += data.audio.length;
        // A stalled tab must not replay seconds of old microphone audio. Only
        // overflow recovery discards PCM; report it, and invalidate calibration.
        if (this.queued > this.maxQueuedFrames) {
          let discard = this.queued - this.prebufferFrames;
          while (discard > 0 && this.queue.length) {
            const n = Math.min(discard, this.queue[0].length - this.offset);
            this.offset += n; this.queued -= n; discard -= n;
            if (this.offset === this.queue[0].length) { this.queue.shift(); this.offset = 0; }
          }
          this.resyncs++;
        }
      }
      if (data.reset) this.reset();
      if (data.start) {
        this.captureFrames = data.captureFrames;
        this.prebufferFrames = data.prebufferFrames;
        this.maxQueuedFrames = data.maxQueuedFrames;
        this.capture = new Float32Array(this.captureFrames);
        this.enabled = true; this.at = 0;
      }
      if (data.probe) {
        this.test = {
          reference: data.probe,
          synthetic: data.synthetic === true,
          mic: new Float32Array(data.probe.length),
          processed: new Float32Array(data.probe.length),
          at: 0, underruns: this.underruns, resyncs: this.resyncs
        };
        this.testMuted = true;
      }
      if (data.testFinished) this.testMuted = false;
    };
  }
  reset() {
    this.queue = [];
    this.queued = 0;
    this.offset = 0;
    this.playing = false;
  }
  process(inputs, outputs) {
    const out = outputs[0][0];
    const input = inputs[0]?.[0];
    if (!this.playing && this.queued >= this.prebufferFrames && this.queued) this.playing = true;
    for (let i = 0; i < out.length; i++) {
      const test = this.test;
      const sample = test?.synthetic ? test.reference[test.at] : (input?.[i] ?? 0);
      if (this.playing && this.queue.length) {
        const block = this.queue[0];
        out[i] = block[this.offset++];
        this.queued--;
        if (this.offset === block.length) {
          this.queue.shift();
          this.offset = 0;
        }
      } else {
        out[i] = 0;
        if (this.playing) {
          this.playing = false;
          this.underruns++;
        }
      }
      if (this.enabled) this.capture[this.at++] = sample;
      if (this.enabled && this.at === this.capture.length) {
        this.port.postMessage({ mic: this.capture, underruns: this.underruns,
          queuedFrames: this.queued, resyncs: this.resyncs, capturedAt: Date.now() }, [this.capture.buffer]);
        this.capture = new Float32Array(this.captureFrames);
        this.at = 0;
      }
      // Calibration records the real mic and the normal firmware render on the
      // same frame clock. Only the quiet probe reaches the speakers: no feedback.
      if (test) {
        const t = test;
        t.mic[t.at] = sample;
        t.processed[t.at] = out[i];
        out[i] = t.synthetic ? 0 : t.reference[t.at];
        t.at++;
        if (t.at === t.reference.length) {
          this.port.postMessage({ latencyResult: true, micRecording: t.mic,
            processedRecording: t.processed,
            underruns: this.underruns - t.underruns + this.resyncs - t.resyncs },
            [t.mic.buffer, t.processed.buffer]);
          this.test = null;
        }
      } else if (this.testMuted) out[i] = 0;
    }
    // The original instrument emits the same signal to both output channels.
    for (let channel = 1; channel < outputs[0].length; channel++) outputs[0][channel].set(out);
    return true;
  }
}
registerProcessor('firmware-audio', FirmwareAudio);
