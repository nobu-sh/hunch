import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ToggleIconButton } from './ToggleIconButton';

function renderToggle(isOn: boolean) {
  const onToggle = vi.fn();
  render(
    <ToggleIconButton isOn={isOn} onToggle={onToggle} label="Pin" title="Pin it" dataRef="pin">
      <svg />
    </ToggleIconButton>,
  );
  return { onToggle, button: screen.getByRole('button', { name: 'Pin' }) };
}

describe('ToggleIconButton', () => {
  it('shows its state as pressed', () => {
    const { button } = renderToggle(true);

    expect(button).toHaveAttribute('aria-pressed', 'true');
    expect(button).toHaveAttribute('title', 'Pin it');
    expect(button).toHaveAttribute('data-ref', 'pin');
  });

  it('asks to turn on when off', () => {
    const { button, onToggle } = renderToggle(false);

    fireEvent.click(button);
    expect(onToggle).toHaveBeenCalledWith(true);
  });

  it('asks to turn off when on', () => {
    const { button, onToggle } = renderToggle(true);

    fireEvent.click(button);
    expect(onToggle).toHaveBeenCalledWith(false);
  });
});
