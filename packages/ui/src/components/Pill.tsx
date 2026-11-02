import type { Ref } from 'react';
import type { StatusTone } from '../overlay-view';

export interface PillProps {
  /** Short readout such as `128 · Am`. */
  text: string;
  statusTone: StatusTone;
  /** Fires on click. An app that drags the pill should swallow the click that ends a drag. */
  onExpand: () => void;
  ref?: Ref<HTMLButtonElement>;
}

/** The collapsed overlay. */
export function Pill({ text, statusTone, onExpand, ref }: PillProps) {
  return (
    <button className="pill" data-ref="pill" title="Expand Hunch" ref={ref} onClick={onExpand}>
      <span className="dot" data-ref="pillDot" data-tone={statusTone} />
      <span data-ref="pillText">{text}</span>
    </button>
  );
}
