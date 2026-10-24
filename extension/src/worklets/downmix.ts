// Runs in the AudioWorkletGlobalScope of the 22,050 Hz analysis context. Downmixes to mono and
// posts 100 ms chunks straight to the analysis Worker over a MessagePort handed in at start.

import { DOWNMIX_PROCESSOR_NAME } from '@/lib/capture-config';

// TypeScript's DOM lib has no AudioWorkletGlobalScope types; these are the parts used here.
declare abstract class AudioWorkletProcessor {
  readonly port: MessagePort;
  abstract process(inputs: Float32Array[][]): boolean;
}
declare function registerProcessor(
  name: string,
  processorCtor: new () => AudioWorkletProcessor,
): void;
declare const sampleRate: number;

const CHUNK_SECONDS = 0.1;

class DownmixProcessor extends AudioWorkletProcessor {
  private readonly chunkLength = Math.round(sampleRate * CHUNK_SECONDS);
  private chunk = new Float32Array(this.chunkLength);
  private filled = 0;
  private workerPort: MessagePort | undefined;

  constructor() {
    super();
    this.port.onmessage = (event: MessageEvent) => {
      const [workerPort] = event.ports;
      this.workerPort = workerPort;
    };
  }

  process(inputs: Float32Array[][]): boolean {
    const channels = inputs[0] ?? [];
    const frameCount = channels[0]?.length ?? 0;
    const hasDestination = this.workerPort !== undefined;
    if (!hasDestination || frameCount === 0) return true;

    for (let frame = 0; frame < frameCount; frame++) {
      let sum = 0;
      for (const channel of channels) sum += channel[frame] ?? 0;
      this.chunk[this.filled] = sum / channels.length;
      this.filled++;
      if (this.filled === this.chunkLength) this.flush();
    }
    return true;
  }

  private flush(): void {
    const full = this.chunk;
    this.workerPort?.postMessage(full, [full.buffer]);
    this.chunk = new Float32Array(this.chunkLength);
    this.filled = 0;
  }
}

registerProcessor(DOWNMIX_PROCESSOR_NAME, DownmixProcessor);
