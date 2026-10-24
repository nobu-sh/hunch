import init, { Analyzer, Session, version } from '@/wasm/pkg/hunch_wasm.js';
import wasmUrl from '@/wasm/pkg/hunch_wasm_bg.wasm?url';
import { ANALYSIS_SAMPLE_RATE } from '@/lib/capture-config';
import type {
  PingReply,
  SessionEventMessage,
  WorkerRequest,
  WorkerStartRequest,
} from '@/lib/messages';

const PING_SILENCE_SECONDS = 1.5;

interface TabSession {
  session: Session;
  /** Receives mono chunks from the tab's AudioWorklet. */
  chunkPort: MessagePort;
}

// Passing the URL explicitly makes Vite emit the .wasm as a build asset.
const ready = init({ module_or_path: wasmUrl });
const tabSessions = new Map<number, TabSession>();

async function handlePing(): Promise<PingReply> {
  await ready;
  const analyzer = new Analyzer(ANALYSIS_SAMPLE_RATE);
  try {
    const silence = new Float32Array(Math.round(ANALYSIS_SAMPLE_RATE * PING_SILENCE_SECONDS));
    const estimate = analyzer.push(silence);
    return { version: version(), estimate };
  } finally {
    analyzer.free();
  }
}

function stopSession(tabId: number): void {
  const tabSession = tabSessions.get(tabId);
  if (!tabSession) return;
  tabSessions.delete(tabId);
  tabSession.chunkPort.close();
  tabSession.session.free();
}

async function startSession(params: {
  request: WorkerStartRequest;
  chunkPort: MessagePort;
}): Promise<void> {
  const { request, chunkPort } = params;
  await ready;
  stopSession(request.tabId);
  const session = new Session(ANALYSIS_SAMPLE_RATE);
  tabSessions.set(request.tabId, { session, chunkPort });

  chunkPort.onmessage = (event: MessageEvent<Float32Array>) => {
    for (const sessionEvent of session.push(event.data)) {
      const message: SessionEventMessage = {
        type: 'session-event',
        tabId: request.tabId,
        event: sessionEvent,
      };
      self.postMessage(message);
    }
  };
}

// Each request carries a port: the reply port for `ping`, the worklet's chunk port for `start`.
self.addEventListener('message', (event: MessageEvent<WorkerRequest>) => {
  const request = event.data;
  const [port] = event.ports;
  switch (request.type) {
    case 'ping':
      if (port === undefined) return;
      handlePing()
        .then((reply) => {
          port.postMessage(reply);
        })
        .catch((error: unknown) => {
          console.error('Hunch analysis ping failed', error);
        });
      return;
    case 'start':
      if (port === undefined) return;
      startSession({ request, chunkPort: port }).catch((error: unknown) => {
        console.error(`Hunch failed to start the session for tab ${String(request.tabId)}`, error);
      });
      return;
    case 'stop':
      stopSession(request.tabId);
      return;
  }
});
