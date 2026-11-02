import type { ReactNode, Ref } from 'react';
import type { StatusTone } from '../overlay-view';

/** The close button's words, which depend on what closing does. */
export interface CloseLabels {
  /** The accessible name. */
  label: string;
  /** The hover text. */
  title: string;
}

/** The extension's close button turns Hunch off for the tab. */
export const TURN_OFF_LABELS: CloseLabels = {
  label: 'Turn off',
  title: 'Turn off Hunch for this tab',
};

export interface HeaderProps {
  status: string;
  statusTone: StatusTone;
  onCollapse: () => void;
  onClose: () => void;
  /** Extra buttons, placed before collapse and close. */
  actions?: ReactNode;
  closeLabels?: CloseLabels;
  /** The panel's drag handle; presses on its buttons should stay clicks. */
  ref?: Ref<HTMLDivElement>;
}

export function Header({
  status,
  statusTone,
  onCollapse,
  onClose,
  actions,
  closeLabels = TURN_OFF_LABELS,
  ref,
}: HeaderProps) {
  return (
    <div className="header" data-ref="header" ref={ref}>
      <span className="brand">Hunch</span>
      <span className="status" data-ref="status" data-tone={statusTone} title={status}>
        {status}
      </span>
      {actions}
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
        title={closeLabels.title}
        aria-label={closeLabels.label}
        onClick={onClose}
      >
        ×
      </button>
    </div>
  );
}
