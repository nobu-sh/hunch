import { browser } from 'wxt/browser';
import { isOffscreenPingRequest, type PingReply, type WorkerPingRequest } from '@/lib/messages';

const worker = new Worker(new URL('../../workers/analysis.ts', import.meta.url), {
  type: 'module',
});

worker.addEventListener('error', (event) => {
  console.error('Hunch analysis worker error', event.message);
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

browser.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (!isOffscreenPingRequest(message)) return;
  void pingWorker().then(sendResponse);
  // Keeps the message channel open for the async sendResponse.
  return true;
});
