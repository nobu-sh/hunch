import { browser } from 'wxt/browser';
import type { OverlayMessage } from './messages';

const OVERLAY_SCRIPT = '/overlay.js';

/** Injects the overlay. Pages that block scripts (chrome://, the Web Store) keep the badge only. */
export async function injectOverlay(tabId: number): Promise<void> {
  try {
    await browser.scripting.executeScript({ target: { tabId }, files: [OVERLAY_SCRIPT] });
  } catch (error) {
    console.debug(`Hunch can't show its overlay in tab ${String(tabId)}; badge only`, error);
  }
}

/** Sends to the tab's overlay, if one is mounted. */
export async function sendToOverlay(params: {
  tabId: number;
  message: OverlayMessage;
}): Promise<void> {
  try {
    await browser.tabs.sendMessage(params.tabId, params.message);
  } catch {
    // No overlay in the tab: not injected yet, or the page blocks scripts.
  }
}
