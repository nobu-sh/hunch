import { ToggleIconButton } from './ToggleIconButton';

export interface SourcesButtonProps {
  /** Whether the picker it opens is showing. */
  isOpen: boolean;
  onClick: () => void;
}

/** A header button for the source picker: a speaker glyph. */
export function SourcesButton({ isOpen, onClick }: SourcesButtonProps) {
  return (
    <ToggleIconButton
      isOn={isOpen}
      onToggle={onClick}
      label="Sources"
      title="Choose what to listen to"
      dataRef="sources"
    >
      <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true">
        <path d="M2 6h3l4-3v10l-4-3H2z" fill="currentColor" />
        <path
          d="M11.5 5.5a3.5 3.5 0 0 1 0 5M13.5 3.5a6.5 6.5 0 0 1 0 9"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.4"
          strokeLinecap="round"
        />
      </svg>
    </ToggleIconButton>
  );
}
