// The Tauri backend's commands and events, typed. The JSON is camelCase on both sides.
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  AudioSource,
  OverlayPosition,
  SessionEvent,
  SourceList,
  SourceStatus,
} from '@hunch/ui';
import type { SourceClient } from '@hunch/ui/components';

/** Must match the event names in src-tauri. */
const SESSION_EVENT = 'session-event';
const SOURCE_STATUS_EVENT = 'source-status';
const ALWAYS_ON_TOP_EVENT = 'always-on-top';

/** Logical offsets from a corner of a monitor's work area. */
export interface SavedPosition extends OverlayPosition {
  /** The monitor's name; `null` means the primary monitor. */
  monitor: string | null;
}

export interface WindowSettings {
  /** `null` until the window is first dragged. */
  position: SavedPosition | null;
  alwaysOnTop: boolean;
}

/** Where the window's client area goes, in physical pixels. */
export interface WindowBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Backend extends SourceClient {
  /** Starts a fresh session on the current source. Call it once subscribed. */
  listen: () => Promise<void>;
  onSessionEvent: (handler: (event: SessionEvent) => void) => Promise<UnlistenFn>;
  onSourceStatus: (handler: (status: SourceStatus) => void) => Promise<UnlistenFn>;
  windowSettings: () => Promise<WindowSettings>;
  saveWindowPosition: (position: SavedPosition) => Promise<void>;
  /** Moves and sizes the window's client area in one step. */
  setWindowBounds: (bounds: WindowBounds) => Promise<void>;
  setAlwaysOnTop: (enabled: boolean) => Promise<void>;
  /** Hides the window from the screen, keeping its size and webview, while it resizes. */
  setWindowCloaked: (cloaked: boolean) => Promise<void>;
  /** Hides the window to the tray; its session keeps running. */
  hideToTray: () => Promise<void>;
  /** Hears always-on-top changes from the header toggle and the tray alike. */
  onAlwaysOnTop: (handler: (enabled: boolean) => void) => Promise<UnlistenFn>;
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
  windowSettings: () => invoke<WindowSettings>('window_settings'),
  saveWindowPosition: (position) => invoke('save_window_position', { position }),
  setWindowBounds: (bounds) => invoke('set_window_bounds', { bounds }),
  setAlwaysOnTop: (enabled) => invoke('set_always_on_top', { enabled }),
  setWindowCloaked: (cloaked) => invoke('set_window_cloaked', { cloaked }),
  hideToTray: () => invoke('hide_to_tray'),
  onAlwaysOnTop: (handler) =>
    listen<boolean>(ALWAYS_ON_TOP_EVENT, ({ payload }) => {
      handler(payload);
    }),
};
