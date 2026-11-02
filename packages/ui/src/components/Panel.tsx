import type { Ref } from 'react';
import type { OverlayView } from '../overlay-view';
import { BpmReadout } from './BpmReadout';
import { Header } from './Header';
import { History } from './History';
import { KeyReadout } from './KeyReadout';
import { ScaleNotes } from './ScaleNotes';

export interface PanelProps {
  view: OverlayView;
  onCollapse: () => void;
  onClose: () => void;
  onCopy: (text: string) => Promise<void>;
  headerRef?: Ref<HTMLDivElement>;
}

/** The expanded overlay. */
export function Panel({ view, onCollapse, onClose, onCopy, headerRef }: PanelProps) {
  return (
    <div className="panel">
      <Header
        status={view.status}
        statusTone={view.statusTone}
        onCollapse={onCollapse}
        onClose={onClose}
        ref={headerRef}
      />
      <div className="readouts">
        <KeyReadout view={view.key} tuning={view.tuning} />
        <BpmReadout view={view.bpm} />
      </div>
      <ScaleNotes notes={view.key?.notes ?? null} onCopy={onCopy} />
      <History rows={view.history} onCopy={onCopy} />
    </div>
  );
}
