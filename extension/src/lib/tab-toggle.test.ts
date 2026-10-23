import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fakeBrowser } from 'wxt/testing/fake-browser';
import { getEnabledTabs, removeTab, toggleTab } from './tab-toggle';

const TAB_A = 1;
const TAB_B = 2;

describe('tab toggle', () => {
  beforeEach(() => {
    fakeBrowser.reset();
  });

  it('enables a tab on first toggle and shows the ON badge', async () => {
    const setBadgeText = vi.spyOn(fakeBrowser.action, 'setBadgeText');

    const isEnabled = await toggleTab(TAB_A);

    expect(isEnabled).toBe(true);
    expect(await getEnabledTabs()).toEqual([TAB_A]);
    expect(setBadgeText).toHaveBeenCalledWith({ tabId: TAB_A, text: 'ON' });
  });

  it('disables a tab on second toggle and clears its badge', async () => {
    await toggleTab(TAB_A);
    const setBadgeText = vi.spyOn(fakeBrowser.action, 'setBadgeText');

    const isEnabled = await toggleTab(TAB_A);

    expect(isEnabled).toBe(false);
    expect(await getEnabledTabs()).toEqual([]);
    expect(setBadgeText).toHaveBeenCalledWith({ tabId: TAB_A, text: '' });
  });

  it('keeps tabs independent', async () => {
    await toggleTab(TAB_A);
    await toggleTab(TAB_B);
    await toggleTab(TAB_A);

    expect(await getEnabledTabs()).toEqual([TAB_B]);
  });

  it('drops a removed tab', async () => {
    await toggleTab(TAB_A);
    await toggleTab(TAB_B);

    await removeTab(TAB_A);

    expect(await getEnabledTabs()).toEqual([TAB_B]);
  });

  it('ignores removal of a tab that was never enabled', async () => {
    await toggleTab(TAB_A);

    await removeTab(TAB_B);

    expect(await getEnabledTabs()).toEqual([TAB_A]);
  });
});
