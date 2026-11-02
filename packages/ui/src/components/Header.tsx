import type { Ref } from 'react';
import type { StatusTone } from '../overlay-view';

export interface HeaderProps {
  status: string;
  statusTone: StatusTone;
  onCollapse: () => void;
  onClose: () => void;
  /** The panel's drag handle; presses on its buttons should stay clicks. */
  ref?: Ref<HTMLDivElement>;
}

export function Header({ status, statusTone, onCollapse, onClose, ref }: HeaderProps) {
  return (
    <div className="header" data-ref="header" ref={ref}>
      <span className="brand">Hunch</span>
      <span className="status" data-ref="status" data-tone={statusTone} title={status}>
        {status}
      </span>
      <button
        className="icon"
        data-ref="collapse"
        title="Collapse"
        aria-label="Collapse"
        onClick={onCollapse}
      >
        –
      </button>
      <button
        className="icon"
        data-ref="close"
        title="Turn off Hunch for this tab"
        aria-label="Turn off"
        onClick={onClose}
      >
        ×
      </button>
    </div>
  );
}
