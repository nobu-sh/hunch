import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import {
  DEVICES_ONLY_LIST,
  HEADSET_SOURCE,
  SOURCE_LIST,
  SPEAKERS_SOURCE,
  SPOTIFY_SOURCE,
} from './fixtures';
import { SourcePicker, type SourcePickerProps } from './SourcePicker';

function renderPicker(overrides: Partial<SourcePickerProps> = {}) {
  const props: SourcePickerProps = {
    sources: SOURCE_LIST,
    currentId: null,
    pendingId: null,
    error: null,
    onSelect: vi.fn(),
    onBack: vi.fn(),
    ...overrides,
  };
  render(<SourcePicker {...props} />);
  return props;
}

const optionNames = () =>
  screen.getAllByRole('option').map((option) => option.querySelector('.option-label')?.textContent);

describe('SourcePicker tabs', () => {
  it('opens on the apps tab and lists only apps there', () => {
    renderPicker();

    expect(screen.getByRole('tab', { name: 'Apps' })).toHaveAttribute('aria-selected', 'true');
    expect(optionNames()).toEqual(['Spotify', 'Google Chrome']);
  });

  it('opens on the tab of the current source and marks it', () => {
    renderPicker({ currentId: HEADSET_SOURCE.id });

    expect(screen.getByRole('tab', { name: 'Devices' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('option', { name: /Headset/ })).toHaveAttribute(
      'aria-selected',
      'true',
    );
  });

  it('lists devices with their default and direction badges', () => {
    renderPicker();

    fireEvent.click(screen.getByRole('tab', { name: 'Devices' }));

    expect(optionNames()).toEqual([
      'Speakers (Realtek Audio)',
      'Headset Earphone',
      'Microphone Array',
    ]);
    expect(screen.getByRole('option', { name: /Speakers/ })).toHaveTextContent('defaultoutput');
    expect(screen.getByRole('option', { name: /Microphone/ })).toHaveTextContent('input');
  });

  it('hides the apps tab when apps cannot be listed', () => {
    renderPicker({ sources: DEVICES_ONLY_LIST, initialTab: 'app' });

    expect(screen.queryByRole('tab', { name: 'Apps' })).not.toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Devices' })).toHaveAttribute('aria-selected', 'true');
  });

  it('says when a tab has nothing to pick', () => {
    renderPicker({ sources: { sources: [SPEAKERS_SOURCE], listsApps: true } });

    expect(screen.getByText('No apps are playing sound.')).toBeInTheDocument();
  });
});

describe('SourcePicker states', () => {
  it('shows loading until the sources arrive', () => {
    renderPicker({ sources: null });

    expect(screen.getByText('Loading…')).toBeInTheDocument();
  });

  it('calls back with the picked source', () => {
    const props = renderPicker();

    fireEvent.click(screen.getByRole('option', { name: /Spotify/ }));

    expect(props.onSelect).toHaveBeenCalledWith(SPOTIFY_SOURCE.id);
  });

  it('shows the source being switched to', () => {
    renderPicker({ pendingId: SPOTIFY_SOURCE.id });

    expect(screen.getByRole('option', { name: /Spotify/ })).toHaveTextContent('switching…');
  });

  it('shows an error', () => {
    renderPicker({ error: "Couldn't switch: device in use" });

    expect(screen.getByRole('alert')).toHaveTextContent("Couldn't switch: device in use");
  });

  it('goes back from the back button or Escape', () => {
    const props = renderPicker();

    fireEvent.click(screen.getByRole('button', { name: 'Back' }));
    fireEvent.keyDown(screen.getByRole('tab', { name: 'Apps' }), { key: 'Escape' });

    expect(props.onBack).toHaveBeenCalledTimes(2);
  });
});
