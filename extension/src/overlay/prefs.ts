import { browser } from 'wxt/browser';
import { DEFAULT_POSITION, type OverlayPosition } from '@/lib/overlay-position';

const POSITION_KEY = 'overlayPosition';
const COLLAPSED_KEY = 'overlayCollapsed';

/** Overlay placement shared by every tab, kept in `storage.local`. */
export interface OverlayPrefs {
  position: OverlayPosition;
  isCollapsed: boolean;
}

export async function loadOverlayPrefs(): Promise<OverlayPrefs> {
  const stored = await browser.storage.local.get([POSITION_KEY, COLLAPSED_KEY]);
  return {
    position: (stored[POSITION_KEY] as OverlayPosition | undefined) ?? DEFAULT_POSITION,
    isCollapsed: (stored[COLLAPSED_KEY] as boolean | undefined) ?? false,
  };
}

export async function savePosition(position: OverlayPosition): Promise<void> {
  await browser.storage.local.set({ [POSITION_KEY]: position });
}

export async function saveCollapsed(isCollapsed: boolean): Promise<void> {
  await browser.storage.local.set({ [COLLAPSED_KEY]: isCollapsed });
}
