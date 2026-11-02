export interface ConfidenceMeterProps {
  /** Prefix for the `data-ref` names: `key` gives `keyMeter` and `keyBar`. */
  name: string;
  /** 0 to 1. */
  confidence: number;
  isLocked: boolean;
}

export function ConfidenceMeter({ name, confidence, isLocked }: ConfidenceMeterProps) {
  const width = `${String(Math.round(confidence * 100))}%`;
  return (
    <div className="meter" data-ref={`${name}Meter`} data-locked={String(isLocked)}>
      <div data-ref={`${name}Bar`} style={{ width }} />
    </div>
  );
}
