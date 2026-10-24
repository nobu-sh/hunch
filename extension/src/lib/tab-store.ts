import { browser } from 'wxt/browser';
import { createTabState, type TabState } from './tab-state';

const pendingUpdates = new Map<number, Promise<TabState>>();

function storageKey(tabId: number): string {
  return `tabState:${String(tabId)}`;
}

export async function getTabState(tabId: number): Promise<TabState | null> {
  const key = storageKey(tabId);
  const stored = await browser.storage.session.get(key);
  return (stored[key] as TabState | undefined) ?? null;
}

async function applyUpdate(params: {
  tabId: number;
  update: (state: TabState) => TabState;
}): Promise<TabState> {
  const current = (await getTabState(params.tabId)) ?? createTabState(params.tabId);
  const next = params.update(current);
  await browser.storage.session.set({ [storageKey(params.tabId)]: next });
  return next;
}

/**
 * Read-modify-writes a tab's state. Updates for the same tab run one at a time, since session
 * events can arrive back to back (a window ending and the next starting in one chunk).
 */
export function updateTabState(params: {
  tabId: number;
  update: (state: TabState) => TabState;
}): Promise<TabState> {
  const previous = pendingUpdates.get(params.tabId) ?? Promise.resolve(null);
  const next = previous.catch(() => null).then(() => applyUpdate(params));
  pendingUpdates.set(params.tabId, next);
  void next.finally(() => {
    if (pendingUpdates.get(params.tabId) === next) pendingUpdates.delete(params.tabId);
  });
  return next;
}

export async function clearTabState(tabId: number): Promise<void> {
  await pendingUpdates.get(tabId)?.catch(() => null);
  await browser.storage.session.remove(storageKey(tabId));
}
