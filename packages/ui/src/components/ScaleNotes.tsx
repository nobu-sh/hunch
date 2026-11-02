import { EMPTY_VALUE } from '../overlay-view';
import { CopyButton } from './CopyButton';

export interface ScaleNotesProps {
  /** Space-separated note names, or null before a key is known. */
  notes: string | null;
  onCopy: (text: string) => Promise<void>;
}

export function ScaleNotes({ notes, onCopy }: ScaleNotesProps) {
  return (
    <div className="notes">
      <span className="notes-text" data-ref="notes">
        {notes ?? EMPTY_VALUE}
      </span>
      <CopyButton text={notes} onCopy={onCopy} />
    </div>
  );
}
