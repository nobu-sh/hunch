import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { Header, type HeaderProps } from './Header';

function renderHeader(overrides: Partial<HeaderProps> = {}) {
  const props: HeaderProps = {
    status: 'listening…',
    statusTone: 'idle',
    onCollapse: vi.fn(),
    onClose: vi.fn(),
    ...overrides,
  };
  render(<Header {...props} />);
  return props;
}

describe('Header', () => {
  it('keeps the extension wording for close by default', () => {
    renderHeader();

    const close = screen.getByRole('button', { name: 'Turn off' });
    expect(close).toHaveAttribute('title', 'Turn off Hunch for this tab');
    expect(close).toHaveAttribute('data-ref', 'close');
  });

  it('takes the close wording from props', () => {
    const props = renderHeader({ closeLabels: { label: 'Quit', title: 'Quit Hunch' } });

    const close = screen.getByRole('button', { name: 'Quit' });
    expect(close).toHaveAttribute('title', 'Quit Hunch');
    fireEvent.click(close);
    expect(props.onClose).toHaveBeenCalledOnce();
  });

  it('puts actions before collapse and close', () => {
    renderHeader({ actions: <button>Sources</button> });

    const names = screen.getAllByRole('button').map((button) => button.textContent);
    expect(names).toEqual(['Sources', '–', '×']);
  });
});
