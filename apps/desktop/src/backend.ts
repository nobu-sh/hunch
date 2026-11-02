// The Tauri backend's commands and events, typed. The JSON is camelCase on both sides.
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { AudioSource, SessionEvent, SourceList, SourceStatus } from '@hunch/ui';
import type { SourceClient } from '@hunch/ui/components';

/** Must match the event names in src-tauri. */
const SESSION_EVENT = 'session-event';
const SOURCE_STATUS_EVENT = 'source-status';

export interface Backend extends SourceClient {
  /** Starts a fresh session on the current source. Call it once subscribed. */
  listen: () => Promise<void>;
  onSessionEvent: (handler: (event: SessionEvent) => void) => Promise<UnlistenFn>;
  onSourceStatus: (handler: (status: SourceStatus) => void) => Promise<UnlistenFn>;
}

export const backend: Backend = {
  listSources: () => invoke<SourceList>('list_sources'),
  currentSource: () => invoke<AudioSource | null>('current_source'),
  selectSource: (id) => invoke('select_source', { id }),
  listen: () => invoke('listen'),
  onSessionEvent: (handler) =>
    listen<SessionEvent>(SESSION_EVENT, ({ payload }) => {
      handler(payload);
    }),
  onSourceStatus: (handler) =>
    listen<SourceStatus>(SOURCE_STATUS_EVENT, ({ payload }) => {
      handler(payload);
    }),
};
