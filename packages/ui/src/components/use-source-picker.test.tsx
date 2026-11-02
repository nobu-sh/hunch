import { act, fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { SOURCE_LIST, SPEAKERS_SOURCE, SPOTIFY_SOURCE } from './fixtures';
import { SourcePicker } from './SourcePicker';
import { useSourcePicker, type SourceClient } from './use-source-picker';

function fakeClient(overrides: Partial<SourceClient> = {}): SourceClient {
  return {
    listSources: vi.fn(() => Promise.resolve(SOURCE_LIST)),
    currentSource: vi.fn(() => Promise.resolve(SPEAKERS_SOURCE)),
    selectSource: vi.fn(() => Promise.resolve()),
    ...overrides,
  };
}

function Harness(props: { client: SourceClient; onSwitch?: (id: string) => void }) {
  const picker = useSourcePicker(props);
  return (
    <>
      <button onClick={picker.open}>Open</button>
      <button onClick={picker.refreshCurrent}>Refresh</button>
      <output>{picker.current?.name ?? 'none'}</output>
      {picker.isOpen && <SourcePicker {...picker.pickerProps} />}
    </>
  );
}

async function renderAndOpen(params: { client: SourceClient; onSwitch?: (id: string) => void }) {
  render(<Harness {...params} />);
  await act(async () => {
    fireEvent.click(screen.getByRole('button', { name: 'Open' }));
    await Promise.resolve();
  });
}

describe('useSourcePicker', () => {
  it('reads the current source on mount', async () => {
    render(<Harness client={fakeClient()} />);

    expect(
      await screen.findByText(SPEAKERS_SOURCE.name, { selector: 'output' }),
    ).toBeInTheDocument();
  });

  it('reads the current source again on refresh', async () => {
    const currentSource = vi
      .fn<SourceClient['currentSource']>()
      .mockResolvedValueOnce(null)
      .mockResolvedValue(SPOTIFY_SOURCE);
    render(<Harness client={fakeClient({ currentSource })} />);
    expect(await screen.findByText('none', { selector: 'output' })).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));

    expect(
      await screen.findByText(SPOTIFY_SOURCE.name, { selector: 'output' }),
    ).toBeInTheDocument();
  });

  it('loads the sources when it opens and marks the current one', async () => {
    const client = fakeClient();
    await renderAndOpen({ client });

    expect(client.listSources).toHaveBeenCalledOnce();
    expect(await screen.findByRole('option', { name: /Speakers/ })).toHaveAttribute(
      'aria-selected',
      'true',
    );
  });

  it('switches, resets the session first, then closes on the new source', async () => {
    const onSwitch = vi.fn();
    const client = fakeClient();
    await renderAndOpen({ client, onSwitch });
    fireEvent.click(await screen.findByRole('tab', { name: 'Apps' }));

    await act(async () => {
      fireEvent.click(screen.getByRole('option', { name: /Spotify/ }));
      await Promise.resolve();
    });

    expect(onSwitch).toHaveBeenCalledWith(SPOTIFY_SOURCE.id);
    expect(client.selectSource).toHaveBeenCalledWith(SPOTIFY_SOURCE.id);
    expect(screen.queryByRole('tablist')).not.toBeInTheDocument();
    expect(screen.getByText('Spotify', { selector: 'output' })).toBeInTheDocument();
  });

  it('stays open with the reason when a switch is rejected', async () => {
    const client = fakeClient({
      selectSource: vi.fn(() => Promise.reject(new Error('device in use'))),
    });
    await renderAndOpen({ client });
    fireEvent.click(await screen.findByRole('tab', { name: 'Apps' }));

    await act(async () => {
      fireEvent.click(screen.getByRole('option', { name: /Spotify/ }));
      await Promise.resolve();
    });

    expect(await screen.findByRole('alert')).toHaveTextContent("Couldn't switch: device in use");
    expect(screen.getByRole('option', { name: /Spotify/ })).not.toHaveTextContent('switching…');
  });

  it('shows an empty list with the reason when the sources cannot be listed', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const client = fakeClient({
      listSources: vi.fn(() => Promise.reject(new Error('Command list_sources not found'))),
      currentSource: vi.fn(() => Promise.reject(new Error('Command current_source not found'))),
    });
    await renderAndOpen({ client });

    expect(await screen.findByRole('alert')).toHaveTextContent(
      "Couldn't list sources: Command list_sources not found",
    );
    expect(screen.getByText('No audio devices found.')).toBeInTheDocument();
  });

  it('just closes when the current source is picked again', async () => {
    const onSwitch = vi.fn();
    const client = fakeClient();
    await renderAndOpen({ client, onSwitch });

    fireEvent.click(await screen.findByRole('option', { name: /Speakers/ }));

    expect(client.selectSource).not.toHaveBeenCalled();
    expect(onSwitch).not.toHaveBeenCalled();
    expect(screen.queryByRole('tablist')).not.toBeInTheDocument();
  });
});
