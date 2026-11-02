import type { Ref } from 'react';
import type { OverlayView } from '../overlay-view';
import { Panel } from './Panel';
import { Pill } from './Pill';

export interface OverlayProps {
  view: OverlayView;
  isCollapsed: boolean;
  onCollapse: () => void;
  onExpand: () => void;
  /** Turns Hunch off for this source. */
  onClose: () => void;
  /** Writes text to the clipboard; a rejection shows as `Failed` on the button. */
  onCopy: (text: string) => Promise<void>;
  /** Drag handles. Mounting, dragging and position stay with the app. */
  headerRef?: Ref<HTMLDivElement>;
  pillRef?: Ref<HTMLButtonElement>;
}

/** The whole overlay: the panel, or the pill while collapsed. Both stay mounted and CSS shows one. */
export function Overlay({ view, isCollapsed, onExpand, pillRef, ...panelProps }: OverlayProps) {
  return (
    <div className="root" data-collapsed={String(isCollapsed)}>
      <Panel view={view} {...panelProps} />
      <Pill text={view.pill} statusTone={view.statusTone} onExpand={onExpand} ref={pillRef} />
    </div>
  );
}
