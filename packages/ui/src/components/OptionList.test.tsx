import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { OptionList, type OptionListProps } from './OptionList';

const OPTIONS = [
  { id: 'a', label: 'Alpha', badges: ['default', 'output'] },
  { id: 'b', label: 'Beta', detail: 'second line' },
  { id: 'c', label: 'Gamma' },
];

function renderList(overrides: Partial<OptionListProps> = {}) {
  const props: OptionListProps = {
    options: OPTIONS,
    selectedId: 'b',
    onSelect: vi.fn(),
    label: 'Letters',
    emptyText: 'Nothing here.',
    ...overrides,
  };
  render(<OptionList {...props} />);
  return props;
}

describe('OptionList', () => {
  it('shows each option with its badges and detail, and marks the selected one', () => {
    renderList();

    const options = screen.getAllByRole('option');
    expect(options.map((option) => option.getAttribute('aria-selected'))).toEqual([
      'false',
      'true',
      'false',
    ]);
    expect(within(options[0] as HTMLElement).getByText('default')).toBeInTheDocument();
    expect(within(options[0] as HTMLElement).getByText('output')).toBeInTheDocument();
    expect(within(options[1] as HTMLElement).getByText('second line')).toBeInTheDocument();
  });

  it('starts the tab order on the selected option', () => {
    renderList();

    expect(screen.getByRole('option', { name: /Beta/ })).toHaveAttribute('tabindex', '0');
    expect(screen.getByRole('option', { name: /Alpha/ })).toHaveAttribute('tabindex', '-1');
  });

  it('picks on click', () => {
    const props = renderList();

    fireEvent.click(screen.getByRole('option', { name: /Gamma/ }));

    expect(props.onSelect).toHaveBeenCalledWith('c');
  });

  it('moves focus with the arrow keys and picks with Enter or Space', () => {
    const props = renderList();
    const beta = screen.getByRole('option', { name: /Beta/ });
    const gamma = screen.getByRole('option', { name: /Gamma/ });
    const alpha = screen.getByRole('option', { name: /Alpha/ });

    fireEvent.keyDown(beta, { key: 'ArrowDown' });
    expect(gamma).toHaveFocus();
    fireEvent.keyDown(gamma, { key: 'ArrowDown' });
    expect(alpha).toHaveFocus();
    fireEvent.keyDown(alpha, { key: 'Enter' });
    expect(props.onSelect).toHaveBeenLastCalledWith('a');

    fireEvent.keyDown(alpha, { key: 'End' });
    expect(gamma).toHaveFocus();
    fireEvent.keyDown(gamma, { key: ' ' });
    expect(props.onSelect).toHaveBeenLastCalledWith('c');
  });

  it('shows the empty text when there are no options', () => {
    renderList({ options: [] });

    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
    expect(screen.getByText('Nothing here.')).toBeInTheDocument();
  });

  it('marks the pending option and ignores picks until it settles', () => {
    const props = renderList({ pendingId: 'c' });

    expect(screen.getByRole('listbox')).toHaveAttribute('aria-busy', 'true');
    const gamma = screen.getByRole('option', { name: /Gamma/ });
    expect(within(gamma).getByText('switching…')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('option', { name: /Alpha/ }));
    expect(props.onSelect).not.toHaveBeenCalled();
  });
});
