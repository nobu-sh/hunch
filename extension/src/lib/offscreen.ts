import { browser } from 'wxt/browser';
import type { OffscreenRequest } from './messages';

const OFFSCREEN_PATH = '/offscreen.html';

let creating: Promise<void> | undefined;

async function hasOffscreenDocument(): Promise<boolean> {
  const contexts = await browser.runtime.getContexts({
    contextTypes: [browser.runtime.ContextType.OFFSCREEN_DOCUMENT],
    documentUrls: [browser.runtime.getURL(OFFSCREEN_PATH)],
  });
  return contexts.length > 0;
}

async function createOffscreenDocument(): Promise<void> {
  await browser.offscreen.createDocument({
    url: OFFSCREEN_PATH,
    reasons: [browser.offscreen.Reason.USER_MEDIA],
    justification: 'Analyzes the captured tab audio locally to detect its key and BPM.',
  });
}

/** Creates the offscreen document unless it exists or is already being created. */
export async function ensureOffscreenDocument(): Promise<void> {
  if (await hasOffscreenDocument()) return;
  creating ??= createOffscreenDocument().finally(() => {
    creating = undefined;
  });
  await creating;
}

export async function closeOffscreenDocument(): Promise<void> {
  if (!(await hasOffscreenDocument())) return;
  await browser.offscreen.closeDocument();
}

/** Sends a request to the offscreen document, which must exist. */
export async function sendOffscreenRequest(request: OffscreenRequest): Promise<void> {
  await browser.runtime.sendMessage(request);
}
