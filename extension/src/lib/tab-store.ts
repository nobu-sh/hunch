import { browser } from 'wxt/browser';
import { createSessionState, type SessionState } from '@hunch/ui';

const pendingUpdates = new Map<number, Promise<SessionState>>();

function storageKey(tabId: number): string {
  return `tabState:${String(tabId)}`;
}

export async function getTabState(tabId: number): Promise<SessionState | null> {
  const key = storageKey(tabId);
  const stored = await browser.storage.session.get(key);
  return (stored[key] as SessionState | undefined) ?? null;
}

async function applyUpdate(params: {
  tabId: number;
  update: (state: SessionState) => SessionState;
}): Promise<SessionState> {
  const current = (await getTabState(params.tabId)) ?? createSessionState(params.tabId);
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
  update: (state: SessionState) => SessionState;
}): Promise<SessionState> {
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
