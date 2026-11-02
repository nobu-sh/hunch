// The backend's session events, folded into the shared session state.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { applySessionEvent, type SessionEvent, type SessionState } from '@hunch/ui';

/** Must match `SESSION_EVENT` in src-tauri/src/listener.rs. */
const SESSION_EVENT = 'session-event';
/** The backend command that starts a fresh session. */
const LISTEN_COMMAND = 'listen';

/**
 * Subscribes, then asks the backend to start listening, so no event is missed. Resolves to the
 * unsubscribe function.
 */
export async function followSession(
  onChange: (update: (state: SessionState) => SessionState) => void,
): Promise<() => void> {
  const unlisten = await listen<SessionEvent>(SESSION_EVENT, ({ payload }) => {
    onChange((state) => applySessionEvent({ state, event: payload, now: Date.now() }));
  });
  try {
    await invoke(LISTEN_COMMAND);
  } catch (error) {
    unlisten();
    throw error;
  }
  return unlisten;
}
