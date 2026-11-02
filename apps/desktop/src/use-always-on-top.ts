import { useState } from 'react';
import { backend } from './backend';
import { useSubscription } from './use-subscription';

/** The backend's default, shown until the saved setting loads. */
const DEFAULT_ALWAYS_ON_TOP = true;

/**
 * Whether the window stays on top, kept in step with the tray menu, and a setter that asks the
 * backend to change it.
 */
export function useAlwaysOnTop(onError: (error: unknown) => void): {
  isOnTop: boolean;
  setOnTop: (enabled: boolean) => void;
} {
  const [isOnTop, setIsOnTop] = useState(DEFAULT_ALWAYS_ON_TOP);

  useSubscription({
    // Subscribed before reading, so a change in between isn't missed.
    subscribe: async () => {
      const unsubscribe = await backend.onAlwaysOnTop(setIsOnTop);
      setIsOnTop((await backend.windowSettings()).alwaysOnTop);
      return unsubscribe;
    },
    onError,
  });

  const setOnTop = (enabled: boolean) => {
    backend.setAlwaysOnTop(enabled).catch(onError);
  };
  return { isOnTop, setOnTop };
}
