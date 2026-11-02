import { EMPTY_VALUE, type KeyView } from '../overlay-view';
import { ConfidenceMeter } from './ConfidenceMeter';

export interface KeyReadoutProps {
  view: KeyView | null;
  /** Tuning offset such as `+23¢`, or null when it's too small to show. */
  tuning: string | null;
}

export function KeyReadout({ view, tuning }: KeyReadoutProps) {
  const isLocked = view?.isLocked ?? false;
  return (
    <div className="key">
      <div className="label">
        Key{' '}
        <span className="tuning" data-ref="tuning">
          {tuning}
        </span>
      </div>
      <div className="value" data-ref="keyValue" data-locked={String(isLocked)}>
        <span data-ref="key">{view?.label ?? EMPTY_VALUE}</span>
        <span className="sub" data-ref="relative">
          {view?.relative}
        </span>
      </div>
      <ConfidenceMeter name="key" confidence={view?.confidence ?? 0} isLocked={isLocked} />
    </div>
  );
}
