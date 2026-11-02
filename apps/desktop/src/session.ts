// The backend's session events, folded into the shared session state.
import { applySessionEvent, type SessionState } from '@hunch/ui';
import type { Backend } from './backend';

/**
 * Subscribes, then asks the backend to start listening, so no event is missed. Resolves to the
 * unsubscribe function.
 */
export async function followSession(params: {
  backend: Backend;
  onChange: (update: (state: SessionState) => SessionState) => void;
}): Promise<() => void> {
  const { backend, onChange } = params;
  const unlisten = await backend.onSessionEvent((event) => {
    onChange((state) => applySessionEvent({ state, event, now: Date.now() }));
  });
  try {
    await backend.listen();
  } catch (error) {
    unlisten();
    throw error;
  }
  return unlisten;
}
