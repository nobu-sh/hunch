import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { OverlayView } from '../overlay-view';
import { ANALYZING_VIEW, ERROR_VIEW, LISTENING_VIEW, LOCKED_VIEW } from './fixtures';
import { Overlay, type OverlayProps } from './Overlay';

function renderOverlay(overrides: Partial<OverlayProps> & { view: OverlayView }) {
  const props: OverlayProps = {
    isCollapsed: false,
    onCollapse: vi.fn(),
    onExpand: vi.fn(),
    onClose: vi.fn(),
    onCopy: vi.fn(() => Promise.resolve()),
    ...overrides,
  };
  const { container } = render(<Overlay {...props} />);
  const byRef = (name: string): HTMLElement => {
    const element = container.querySelector<HTMLElement>(`[data-ref="${name}"]`);
    if (!element) throw new Error(`No [data-ref="${name}"] in the overlay`);
    return element;
  };
  return { props, container, byRef };
}

function historyRows(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>('.history li'));
}

afterEach(() => {
  vi.useRealTimers();
});

describe('Overlay readouts', () => {
  it('shows the key, bpm, tuning and notes from the view', () => {
    const { byRef } = renderOverlay({ view: ANALYZING_VIEW });

    expect(byRef('key')).toHaveTextContent('A minor');
    expect(byRef('relative')).toHaveTextContent('C');
    expect(byRef('tuning')).toHaveTextContent('+23¢');
    expect(byRef('bpm')).toHaveTextContent('120');
    expect(byRef('alternate')).toHaveTextContent('×2 240');
    expect(byRef('notes')).toHaveTextContent('A B C D E F G');
    expect(byRef('pillText')).toHaveTextContent('120 · Am');
  });

  it('shows placeholders and hides copy and history before anything is known', () => {
    const { byRef, container } = renderOverlay({ view: LISTENING_VIEW });

    expect(byRef('key')).toHaveTextContent('—');
    expect(byRef('relative')).toBeEmptyDOMElement();
    expect(byRef('tuning')).toBeEmptyDOMElement();
    expect(byRef('bpm')).toHaveTextContent('—');
    expect(byRef('alternate')).toBeEmptyDOMElement();
    expect(byRef('notes')).toHaveTextContent('—');
    expect(container.querySelector('.notes .copy')).toHaveAttribute('hidden');
    expect(byRef('history')).toHaveAttribute('hidden');
  });

  it('fills the meters to the rounded confidence', () => {
    const { byRef } = renderOverlay({ view: ANALYZING_VIEW });

    expect(byRef('keyBar')).toHaveStyle({ width: '42%' });
    expect(byRef('bpmBar')).toHaveStyle({ width: '62%' });
  });

  it.each([
    { view: ANALYZING_VIEW, isLocked: 'false' },
    { view: LOCKED_VIEW, isLocked: 'true' },
  ])('marks values and meters locked: $isLocked', ({ view, isLocked }) => {
    const { byRef } = renderOverlay({ view });

    for (const name of ['keyValue', 'keyMeter', 'bpmValue', 'bpmMeter']) {
      expect(byRef(name)).toHaveAttribute('data-locked', isLocked);
    }
  });

  it('gives the status and pill dot the view tone', () => {
    const { byRef } = renderOverlay({ view: ERROR_VIEW });

    expect(byRef('status')).toHaveTextContent('Tab capture failed');
    expect(byRef('status')).toHaveAttribute('title', 'Tab capture failed');
    expect(byRef('status')).toHaveAttribute('data-tone', 'error');
    expect(byRef('pillDot')).toHaveAttribute('data-tone', 'error');
  });
});

describe('Overlay collapsing and closing', () => {
  it('marks the root collapsed or expanded', () => {
    const { container, props } = renderOverlay({ view: ANALYZING_VIEW, isCollapsed: true });
    const root = container.querySelector('.root');
    expect(root).toHaveAttribute('data-collapsed', 'true');

    render(<Overlay {...props} isCollapsed={false} />, { container });
    expect(root).toHaveAttribute('data-collapsed', 'false');
  });

  it('calls back from the collapse, close and pill buttons', () => {
    const { props } = renderOverlay({ view: ANALYZING_VIEW });

    fireEvent.click(screen.getByRole('button', { name: 'Collapse' }));
    fireEvent.click(screen.getByRole('button', { name: 'Turn off' }));
    fireEvent.click(screen.getByTitle('Expand Hunch'));

    expect(props.onCollapse).toHaveBeenCalledOnce();
    expect(props.onClose).toHaveBeenCalledOnce();
    expect(props.onExpand).toHaveBeenCalledOnce();
  });

  it('hands the header and pill to the app as drag handles', () => {
    const headerRef = { current: null as HTMLDivElement | null };
    const pillRef = { current: null as HTMLButtonElement | null };
    renderOverlay({ view: ANALYZING_VIEW, headerRef, pillRef });

    expect(headerRef.current).toHaveAttribute('data-ref', 'header');
    expect(pillRef.current).toHaveAttribute('data-ref', 'pill');
  });
});

describe('Overlay copy buttons', () => {
  it('copies the scale notes and confirms, then resets', async () => {
    vi.useFakeTimers();
    const { props, container } = renderOverlay({ view: ANALYZING_VIEW });
    const button = container.querySelector<HTMLElement>('.notes .copy');
    if (!button) throw new Error('No notes copy button');

    await act(async () => {
      fireEvent.click(button);
      await Promise.resolve();
    });
    expect(props.onCopy).toHaveBeenCalledWith('A B C D E F G');
    expect(button).toHaveTextContent('Copied');

    act(() => {
      vi.advanceTimersByTime(1200);
    });
    expect(button).toHaveTextContent('Copy');
  });

  it('says the copy failed when the clipboard rejects', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const onCopy = vi.fn(() => Promise.reject(new Error('Document is not focused')));
    const { container } = renderOverlay({ view: ANALYZING_VIEW, onCopy });
    const button = container.querySelector<HTMLElement>('.notes .copy');
    if (!button) throw new Error('No notes copy button');

    fireEvent.click(button);

    expect(await within(button).findByText('Failed')).toBeInTheDocument();
  });
});

describe('Overlay history', () => {
  it('renders one row per entry with its time, key and bpm', () => {
    const { container, byRef } = renderOverlay({ view: LOCKED_VIEW });

    expect(byRef('history')).not.toHaveAttribute('hidden');
    const cells = historyRows(container).map((row) =>
      ['.time', '.key-cell', '.bpm-cell'].map((cell) => row.querySelector(cell)?.textContent),
    );
    expect(cells).toEqual([
      ['21:04', 'A minor', '120'],
      ['20:58', '—', '96'],
    ]);
  });

  it('copies a row as its copy text', () => {
    const { container, props } = renderOverlay({ view: LOCKED_VIEW });
    const secondRow = historyRows(container)[1];
    if (!secondRow) throw new Error('No second history row');

    const copy = within(secondRow).getByRole('button', { name: 'Copy' });
    expect(copy).toHaveAttribute('title', '96 BPM');
    fireEvent.click(copy);

    expect(props.onCopy).toHaveBeenCalledWith('96 BPM');
  });
});
