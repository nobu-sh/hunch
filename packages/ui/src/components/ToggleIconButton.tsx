import type { ReactNode } from 'react';

export interface ToggleIconButtonProps {
  isOn: boolean;
  onToggle: (isOn: boolean) => void;
  /** The accessible name. */
  label: string;
  /** The tooltip. */
  title: string;
  /** Lets scripts and tests find the button. */
  dataRef?: string;
  /** The glyph, an inline SVG sized to the header's icons. */
  children: ReactNode;
}

/** A header icon button that stays pressed while its setting is on. */
export function ToggleIconButton({
  isOn,
  onToggle,
  label,
  title,
  dataRef,
  children,
}: ToggleIconButtonProps) {
  return (
    <button
      className="icon"
      data-ref={dataRef}
      title={title}
      aria-label={label}
      aria-pressed={isOn}
      onClick={() => {
        onToggle(!isOn);
      }}
    >
      {children}
    </button>
  );
}
