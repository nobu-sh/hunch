import { ToggleIconButton } from './ToggleIconButton';

export interface PinButtonProps {
  isPinned: boolean;
  onToggle: (isPinned: boolean) => void;
}

/** A header toggle for keeping the overlay above other windows: a pushpin glyph. */
export function PinButton({ isPinned, onToggle }: PinButtonProps) {
  return (
    <ToggleIconButton
      isOn={isPinned}
      onToggle={onToggle}
      label="Always on top"
      title={isPinned ? 'Stop keeping Hunch on top' : 'Keep Hunch on top of other windows'}
      dataRef="pin"
    >
      <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true">
        <path d="M5.5 2h5l-1 4 2.5 2.5v1.5H4V8.5L6.5 6z" fill="currentColor" />
        <path d="M8 10v4.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
      </svg>
    </ToggleIconButton>
  );
}
