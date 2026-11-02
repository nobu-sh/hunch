import { useCallback, useEffect, useState } from 'react';
import { errorMessage, type AudioSource, type SourceList } from '../sources';
import type { SourcePickerProps } from './SourcePicker';

/** What the picker needs from a backend. Rejections carry a message for the user. */
export interface SourceClient {
  listSources: () => Promise<SourceList>;
  currentSource: () => Promise<AudioSource | null>;
  /** Resolves once the new source is being listened to. */
  selectSource: (id: string) => Promise<void>;
}

export interface SourcePickerControls {
  isOpen: boolean;
  /** Opens the picker and loads the sources fresh. */
  open: () => void;
  close: () => void;
  /** The source being listened to, as far as the client has said. */
  current: AudioSource | null;
  /** Ready to spread on `SourcePicker`. */
  pickerProps: SourcePickerProps;
}

/**
 * Runs a `SourcePicker` against a client: loads sources when it opens, switches on a pick and
 * closes once the switch lands. `onSwitch` fires as a switch starts, when a fresh session is due.
 */
export function useSourcePicker(params: {
  client: SourceClient;
  onSwitch?: (id: string) => void;
}): SourcePickerControls {
  const { client, onSwitch } = params;
  const [isOpen, setIsOpen] = useState(false);
  const [sources, setSources] = useState<SourceList | null>(null);
  const [current, setCurrent] = useState<AudioSource | null>(null);
  const [pendingId, setPendingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let isCancelled = false;
    client
      .currentSource()
      .then((source) => {
        if (!isCancelled) setCurrent(source);
      })
      .catch((reason: unknown) => {
        console.warn('Hunch could not read the current source:', reason);
      });
    return () => {
      isCancelled = true;
    };
  }, [client]);

  const open = useCallback(() => {
    setIsOpen(true);
    setSources(null);
    setError(null);
    Promise.all([client.listSources(), client.currentSource()])
      .then(([list, source]) => {
        setSources(list);
        setCurrent(source);
      })
      .catch((reason: unknown) => {
        setSources({ sources: [], listsApps: false });
        setError(`Couldn't list sources: ${errorMessage(reason)}`);
      });
  }, [client]);

  const close = useCallback(() => {
    setIsOpen(false);
    setError(null);
  }, []);

  const select = (id: string) => {
    if (id === current?.id) {
      close();
      return;
    }
    setPendingId(id);
    setError(null);
    onSwitch?.(id);
    client
      .selectSource(id)
      .then(() => {
        setCurrent(sources?.sources.find((source) => source.id === id) ?? null);
        setIsOpen(false);
      })
      .catch((reason: unknown) => {
        setError(`Couldn't switch: ${errorMessage(reason)}`);
      })
      .finally(() => {
        setPendingId(null);
      });
  };

  return {
    isOpen,
    open,
    close,
    current,
    pickerProps: {
      sources,
      currentId: current?.id ?? null,
      pendingId,
      error,
      onSelect: select,
      onBack: close,
    },
  };
}
