import { useEffect, useRef } from 'react';

/**
 * Holds an async subscription for the component's lifetime, and unsubscribes even if it unmounts
 * before the subscription resolves. Both functions are read once, on mount.
 */
export function useSubscription(params: {
  subscribe: () => Promise<() => void>;
  onError: (error: unknown) => void;
}): void {
  const initial = useRef(params);

  useEffect(() => {
    let unsubscribe: (() => void) | null = null;
    let isCancelled = false;
    initial.current
      .subscribe()
      .then((cleanup) => {
        if (isCancelled) cleanup();
        else unsubscribe = cleanup;
      })
      .catch((error: unknown) => {
        initial.current.onError(error);
      });
    return () => {
      isCancelled = true;
      unsubscribe?.();
    };
  }, []);
}
