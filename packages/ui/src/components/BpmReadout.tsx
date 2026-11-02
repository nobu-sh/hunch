import { EMPTY_VALUE, type BpmView } from '../overlay-view';
import { ConfidenceMeter } from './ConfidenceMeter';

export interface BpmReadoutProps {
  view: BpmView | null;
}

export function BpmReadout({ view }: BpmReadoutProps) {
  const isLocked = view?.isLocked ?? false;
  return (
    <div className="bpm">
      <div className="label">BPM</div>
      <div className="value" data-ref="bpmValue" data-locked={String(isLocked)}>
        <span data-ref="bpm">{view?.label ?? EMPTY_VALUE}</span>
        <span className="sub" data-ref="alternate">
          {view?.alternate}
        </span>
      </div>
      <ConfidenceMeter name="bpm" confidence={view?.confidence ?? 0} isLocked={isLocked} />
    </div>
  );
}
