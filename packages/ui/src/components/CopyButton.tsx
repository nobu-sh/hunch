import { useEffect, useRef, useState } from 'react';

const COPIED_FEEDBACK_MS = 1200;
const IDLE_LABEL = 'Copy';

type CopyLabel = typeof IDLE_LABEL | 'Copied' | 'Failed';

export interface CopyButtonProps {
  /** Hidden while null. */
  text: string | null;
  /** Writes `text` to the clipboard; a rejection shows as `Failed`. */
  onCopy: (text: string) => Promise<void>;
  title?: string;
}

/** Copies its text and briefly says whether that worked. */
export function CopyButton({ text, onCopy, title }: CopyButtonProps) {
  const [label, setLabel] = useState<CopyLabel>(IDLE_LABEL);
  const resetTimer = useRef<number | undefined>(undefined);

  useEffect(
    () => () => {
      window.clearTimeout(resetTimer.current);
    },
    [],
  );

  const copy = async (value: string): Promise<void> => {
    try {
      await onCopy(value);
      setLabel('Copied');
    } catch (error) {
      // The async clipboard needs a secure, focused page.
      console.warn('Hunch could not copy to the clipboard', error);
      setLabel('Failed');
    }
    window.clearTimeout(resetTimer.current);
    resetTimer.current = window.setTimeout(() => {
      setLabel(IDLE_LABEL);
    }, COPIED_FEEDBACK_MS);
  };

  return (
    <button
      className="copy"
      title={title}
      hidden={text === null}
      onClick={() => {
        if (text !== null) void copy(text);
      }}
    >
      {label}
    </button>
  );
}
