import type { Ref } from 'react';
import { Panel, type PanelProps } from './Panel';
import { Pill } from './Pill';

export interface OverlayProps extends PanelProps {
  isCollapsed: boolean;
  onExpand: () => void;
  pillRef?: Ref<HTMLButtonElement>;
}

/**
 * The whole overlay: the panel, or the pill while collapsed. Both stay mounted and CSS shows one.
 * The header and pill refs are drag handles; mounting, dragging and position stay with the app.
 */
export function Overlay({ isCollapsed, onExpand, pillRef, ...panelProps }: OverlayProps) {
  const { view } = panelProps;
  return (
    <div className="root" data-collapsed={String(isCollapsed)}>
      <Panel {...panelProps} />
      <Pill text={view.pill} statusTone={view.statusTone} onExpand={onExpand} ref={pillRef} />
    </div>
  );
}
