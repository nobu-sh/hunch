import type { ReactNode, Ref } from 'react';
import type { OverlayView } from '../overlay-view';
import { BpmReadout } from './BpmReadout';
import { Header, type CloseLabels } from './Header';
import { History } from './History';
import { KeyReadout } from './KeyReadout';
import { ScaleNotes } from './ScaleNotes';

export interface PanelProps {
  view: OverlayView;
  onCollapse: () => void;
  /** Turns Hunch off for the source, or quits; `closeLabels` should say which. */
  onClose: () => void;
  /** Writes text to the clipboard; a rejection shows as `Failed` on the button. */
  onCopy: (text: string) => Promise<void>;
  headerRef?: Ref<HTMLDivElement>;
  /** Extra header buttons, before collapse and close. */
  headerActions?: ReactNode;
  closeLabels?: CloseLabels;
  /** A banner under the header, such as a `Notice`. */
  notice?: ReactNode;
  /** Shown instead of the readouts while given, such as a `SourcePicker`. */
  body?: ReactNode;
}

function Readouts({ view, onCopy }: Pick<PanelProps, 'view' | 'onCopy'>) {
  return (
    <>
      <div className="readouts">
        <KeyReadout view={view.key} tuning={view.tuning} />
        <BpmReadout view={view.bpm} />
      </div>
      <ScaleNotes notes={view.key?.notes ?? null} onCopy={onCopy} />
      <History rows={view.history} onCopy={onCopy} />
    </>
  );
}

/** The expanded overlay. */
export function Panel({
  view,
  onCollapse,
  onClose,
  onCopy,
  headerRef,
  headerActions,
  closeLabels,
  notice,
  body,
}: PanelProps) {
  return (
    <div className="panel">
      <Header
        status={view.status}
        statusTone={view.statusTone}
        onCollapse={onCollapse}
        onClose={onClose}
        actions={headerActions}
        closeLabels={closeLabels}
        ref={headerRef}
      />
      {notice}
      {body ?? <Readouts view={view} onCopy={onCopy} />}
    </div>
  );
}
