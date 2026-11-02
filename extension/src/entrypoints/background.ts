import { browser } from 'wxt/browser';
import { defineBackground } from 'wxt/utils/define-background';
import { applySessionEvent, createSessionState, withError, type SessionState } from '@hunch/ui';
import { badgeText } from '@/lib/badge';
import {
  isBackgroundMessage,
  isOverlayRequest,
  type BackgroundMessage,
  type OffscreenPingRequest,
  type PingReply,
} from '@/lib/messages';
import {
  closeOffscreenDocument,
  ensureOffscreenDocument,
  sendOffscreenRequest,
} from '@/lib/offscreen';
import { injectOverlay, sendToOverlay } from '@/lib/overlay-injection';
import { clearTabState, getTabState, updateTabState } from '@/lib/tab-store';
import { getEnabledTabs, removeTab, toggleTab } from '@/lib/tab-toggle';

async function pingOffscreen(): Promise<void> {
  const request: OffscreenPingRequest = { target: 'offscreen', type: 'ping' };
  const reply = await browser.runtime.sendMessage<OffscreenPingRequest, PingReply>(request);
  console.log('Hunch offscreen ping reply', reply);
}

async function isTabEnabled(tabId: number): Promise<boolean> {
  const enabledTabs = await getEnabledTabs();
  return enabledTabs.includes(tabId);
}

async function closeOffscreenIfIdle(): Promise<void> {
  const enabledTabs = await getEnabledTabs();
  if (enabledTabs.length > 0) return;
  await closeOffscreenDocument();
}

async function publishTabState(state: SessionState): Promise<void> {
  const { tabId } = state;
  await Promise.all([
    browser.action.setBadgeText({ tabId, text: badgeText(state) }),
    sendToOverlay({ tabId, message: { target: 'overlay', type: 'state', state } }),
  ]);
}

async function updateAndPublish(params: {
  tabId: number;
  update: (state: SessionState) => SessionState;
}): Promise<void> {
  const state = await updateTabState(params);
  await publishTabState(state);
}

async function reportTabError(params: { tabId: number; error: unknown }): Promise<void> {
  const { tabId, error } = params;
  const message = error instanceof Error ? error.message : String(error);
  console.error(`Hunch capture failed for tab ${String(tabId)}`, error);
  await updateAndPublish({ tabId, update: (state) => withError({ state, error: message }) });
}

async function enableTab(params: { tabId: number; streamId: Promise<string> }): Promise<void> {
  const { tabId } = params;
  // Starts fresh, so a late event from a previous session can't leak into this one.
  await updateAndPublish({ tabId, update: () => createSessionState(tabId) });
  await injectOverlay(tabId);
  try {
    const streamId = await params.streamId;
    await ensureOffscreenDocument();
    await sendOffscreenRequest({ target: 'offscreen', type: 'start', tabId, streamId });
    await pingOffscreen();
  } catch (error) {
    await reportTabError({ tabId, error });
  }
}

/** Stops a tab's capture and forgets its state. The tab must already be off in `tab-toggle`. */
async function teardownTab(tabId: number): Promise<void> {
  await sendToOverlay({ tabId, message: { target: 'overlay', type: 'unmount' } });
  try {
    await sendOffscreenRequest({ target: 'offscreen', type: 'stop', tabId });
  } catch (error) {
    // No offscreen document means there is nothing left to stop.
    console.debug(`Hunch had no capture to stop for tab ${String(tabId)}`, error);
  }
  await clearTabState(tabId);
  await closeOffscreenIfIdle();
}

async function handleActionClick(params: {
  tabId: number;
  streamId: Promise<string>;
}): Promise<void> {
  const isEnabled = await toggleTab(params.tabId);
  if (isEnabled) {
    await enableTab(params);
    return;
  }
  await teardownTab(params.tabId);
}

/** The overlay's close button: same as clicking the icon on an enabled tab. */
async function handleOverlayClose(tabId: number): Promise<void> {
  if (!(await isTabEnabled(tabId))) return;
  await toggleTab(tabId);
  await teardownTab(tabId);
}

async function getOverlayState(tabId: number): Promise<SessionState | null> {
  if (!(await isTabEnabled(tabId))) return null;
  return getTabState(tabId);
}

/** A full page load drops the overlay with the old document, so put it back. */
async function handleTabLoaded(tabId: number): Promise<void> {
  if (!(await isTabEnabled(tabId))) return;
  await injectOverlay(tabId);
}

async function handleTabRemoved(tabId: number): Promise<void> {
  if (!(await isTabEnabled(tabId))) return;
  await removeTab(tabId);
  await teardownTab(tabId);
}

async function handleBackgroundMessage(message: BackgroundMessage): Promise<void> {
  const { tabId } = message;
  // Events can trail a tab being turned off; they belong to a session that no longer exists.
  if (!(await isTabEnabled(tabId))) return;
  if (message.type === 'capture-error') {
    await reportTabError({ tabId, error: message.message });
    return;
  }
  await updateAndPublish({
    tabId,
    update: (state) => applySessionEvent({ state, event: message.event, now: Date.now() }),
  });
}

export default defineBackground(() => {
  browser.action.onClicked.addListener((tab) => {
    const tabId = tab.id;
    if (tabId === undefined) return;
    // tabCapture needs the invocation's grant, so ask in the click's own turn, before any await.
    const streamId = browser.tabCapture.getMediaStreamId({ targetTabId: tabId });
    // Marks the rejection handled until enableTab awaits it; turning off never does.
    streamId.catch(() => undefined);
    handleActionClick({ tabId, streamId }).catch((error: unknown) => {
      console.error(`Hunch failed to toggle tab ${String(tabId)}`, error);
    });
  });

  browser.tabs.onRemoved.addListener((tabId) => {
    handleTabRemoved(tabId).catch((error: unknown) => {
      console.error(`Hunch failed to clean up closed tab ${String(tabId)}`, error);
    });
  });

  browser.tabs.onUpdated.addListener((tabId, changeInfo) => {
    if (changeInfo.status !== 'complete') return;
    handleTabLoaded(tabId).catch((error: unknown) => {
      console.error(`Hunch failed to restore the overlay in tab ${String(tabId)}`, error);
    });
  });

  browser.runtime.onMessage.addListener((message, sender, sendResponse) => {
    const senderTabId = sender.tab?.id;
    if (isOverlayRequest(message) && senderTabId !== undefined) {
      if (message.type === 'overlay-close') {
        handleOverlayClose(senderTabId).catch((error: unknown) => {
          console.error(`Hunch failed to turn off tab ${String(senderTabId)}`, error);
        });
        return;
      }
      void getOverlayState(senderTabId).then(sendResponse);
      // Keeps the message channel open for the async sendResponse.
      return true;
    }
    if (!isBackgroundMessage(message)) return;
    handleBackgroundMessage(message).catch((error: unknown) => {
      console.error(
        `Hunch failed to handle ${message.type} for tab ${String(message.tabId)}`,
        error,
      );
    });
  });
});
