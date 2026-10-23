import init, { Analyzer, version } from '@/wasm/pkg/hunch_wasm.js';
import wasmUrl from '@/wasm/pkg/hunch_wasm_bg.wasm?url';
import type { Estimate, PingReply, WorkerPingRequest } from '@/lib/messages';

const SAMPLE_RATE = 22_050;
const PING_SILENCE_SECONDS = 1.5;

// Passing the URL explicitly makes Vite emit the .wasm as a build asset.
const ready = init({ module_or_path: wasmUrl });

async function handlePing(): Promise<PingReply> {
  await ready;
  const analyzer = new Analyzer(SAMPLE_RATE);
  try {
    const silence = new Float32Array(Math.round(SAMPLE_RATE * PING_SILENCE_SECONDS));
    const estimate: Estimate | undefined = analyzer.push(silence);
    return { version: version(), estimate };
  } finally {
    analyzer.free();
  }
}

// `ping` is the only request for now; each carries the port to reply on.
self.addEventListener('message', (event: MessageEvent<WorkerPingRequest>) => {
  const [replyPort] = event.ports;
  if (replyPort === undefined) return;
  handlePing()
    .then((reply) => {
      replyPort.postMessage(reply);
    })
    .catch((error: unknown) => {
      console.error('Hunch analysis ping failed', error);
    });
});
