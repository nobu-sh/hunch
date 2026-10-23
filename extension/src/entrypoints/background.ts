import { browser } from 'wxt/browser';
import { defineBackground } from 'wxt/utils/define-background';
import type { OffscreenPingRequest, PingReply } from '@/lib/messages';
import { closeOffscreenDocument, ensureOffscreenDocument } from '@/lib/offscreen';
import { getEnabledTabs, removeTab, toggleTab } from '@/lib/tab-toggle';

async function pingOffscreen(): Promise<void> {
  const request: OffscreenPingRequest = { target: 'offscreen', type: 'ping' };
  const reply = await browser.runtime.sendMessage<OffscreenPingRequest, PingReply>(request);
  console.log('Hunch offscreen ping reply', reply);
}

async function closeOffscreenIfIdle(): Promise<void> {
  const enabledTabs = await getEnabledTabs();
  if (enabledTabs.length > 0) return;
  await closeOffscreenDocument();
}

async function handleActionClick(tabId: number): Promise<void> {
  const isEnabled = await toggleTab(tabId);
  if (!isEnabled) {
    await closeOffscreenIfIdle();
    return;
  }
  await ensureOffscreenDocument();
  await pingOffscreen();
}

async function handleTabRemoved(tabId: number): Promise<void> {
  await removeTab(tabId);
  await closeOffscreenIfIdle();
}

export default defineBackground(() => {
  browser.action.onClicked.addListener((tab) => {
    const tabId = tab.id;
    if (tabId === undefined) return;
    handleActionClick(tabId).catch((error: unknown) => {
      console.error(`Hunch failed to toggle tab ${String(tabId)}`, error);
    });
  });

  browser.tabs.onRemoved.addListener((tabId) => {
    handleTabRemoved(tabId).catch((error: unknown) => {
      console.error(`Hunch failed to clean up closed tab ${String(tabId)}`, error);
    });
  });
});
