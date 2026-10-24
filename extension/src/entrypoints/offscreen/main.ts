import { browser } from 'wxt/browser';
import {
  isOffscreenRequest,
  type CaptureErrorMessage,
  type PingReply,
  type SessionEventMessage,
  type WorkerPingRequest,
  type WorkerStartRequest,
  type WorkerStopRequest,
} from '@/lib/messages';
import { startTabCapture, type TabCapture } from '@/lib/tab-capture';

const worker = new Worker(new URL('../../workers/analysis.ts', import.meta.url), {
  type: 'module',
});
const captures = new Map<number, Promise<TabCapture>>();

worker.addEventListener('error', (event) => {
  console.error('Hunch analysis worker error', event.message);
});

// The Worker only emits session events; relay them to the background unchanged.
worker.addEventListener('message', (event: MessageEvent<SessionEventMessage>) => {
  void browser.runtime.sendMessage(event.data);
});

/** Sends a ping with its own reply port so concurrent requests can't cross. */
function pingWorker(): Promise<PingReply> {
  const channel = new MessageChannel();
  const reply = new Promise<PingReply>((resolve) => {
    channel.port1.onmessage = (event: MessageEvent<PingReply>) => {
      channel.port1.close();
      resolve(event.data);
    };
  });
  const request: WorkerPingRequest = { type: 'ping' };
  worker.postMessage(request, [channel.port2]);
  return reply;
}

async function stopCapture(tabId: number): Promise<void> {
  const pending = captures.get(tabId);
  if (!pending) return;
  captures.delete(tabId);
  const capture = await pending.catch(() => null);
  if (!capture) return;
  const request: WorkerStopRequest = { type: 'stop', tabId };
  worker.postMessage(request);
  await capture.stop();
}

async function startCapture(params: { tabId: number; streamId: string }): Promise<void> {
  const { tabId, streamId } = params;
  await stopCapture(tabId);
  const pending = startTabCapture({
    streamId,
    onEnded: () => {
      reportCaptureError({ tabId, error: new Error('The tab audio capture ended') });
    },
  });
  // Tracked before it settles, so a `stop` arriving mid-start can still tear it down.
  captures.set(tabId, pending);
  try {
    const capture = await pending;
    if (captures.get(tabId) !== pending) return;
    // The worklet posts chunks into one end, the Worker's Session reads the other.
    const request: WorkerStartRequest = { type: 'start', tabId };
    worker.postMessage(request, [capture.chunkPort]);
  } catch (error) {
    if (captures.get(tabId) === pending) captures.delete(tabId);
    throw error;
  }
}

function reportCaptureError(params: { tabId: number; error: unknown }): void {
  const { tabId, error } = params;
  console.error(`Hunch failed to capture tab ${String(tabId)}`, error);
  const message: CaptureErrorMessage = {
    type: 'capture-error',
    tabId,
    message: error instanceof Error ? error.message : String(error),
  };
  void browser.runtime.sendMessage(message);
}

browser.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (!isOffscreenRequest(message)) return;
  switch (message.type) {
    case 'ping':
      void pingWorker().then(sendResponse);
      // Keeps the message channel open for the async sendResponse.
      return true;
    case 'start':
      startCapture(message).catch((error: unknown) => {
        reportCaptureError({ tabId: message.tabId, error });
      });
      return;
    case 'stop':
      stopCapture(message.tabId).catch((error: unknown) => {
        console.error(`Hunch failed to stop capturing tab ${String(message.tabId)}`, error);
      });
      return;
  }
});
