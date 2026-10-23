import { browser } from 'wxt/browser';

const ENABLED_TABS_KEY = 'enabledTabs';
const BADGE_ON_TEXT = 'ON';

export async function getEnabledTabs(): Promise<number[]> {
  const stored = await browser.storage.session.get(ENABLED_TABS_KEY);
  const tabIds = stored[ENABLED_TABS_KEY] as number[] | undefined;
  return tabIds ?? [];
}

async function setEnabledTabs(tabIds: number[]): Promise<void> {
  await browser.storage.session.set({ [ENABLED_TABS_KEY]: tabIds });
}

/** Flips Hunch for a tab, updates its badge, and returns whether it is now enabled. */
export async function toggleTab(tabId: number): Promise<boolean> {
  const enabledTabs = await getEnabledTabs();
  const isEnabled = !enabledTabs.includes(tabId);
  const nextTabs = isEnabled ? [...enabledTabs, tabId] : enabledTabs.filter((id) => id !== tabId);

  await setEnabledTabs(nextTabs);
  await browser.action.setBadgeText({ tabId, text: isEnabled ? BADGE_ON_TEXT : '' });
  return isEnabled;
}

/** Forgets a closed tab. No badge update: Chrome drops per-tab badges with the tab. */
export async function removeTab(tabId: number): Promise<void> {
  const enabledTabs = await getEnabledTabs();
  if (!enabledTabs.includes(tabId)) return;
  await setEnabledTabs(enabledTabs.filter((id) => id !== tabId));
}
