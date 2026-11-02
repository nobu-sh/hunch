import { describe, expect, it } from 'vitest';
import {
  SILENT_SOURCE_MESSAGE,
  errorMessage,
  sourceBadges,
  sourceNotice,
  type AudioSource,
  type SourceStatus,
} from './sources';

const SPOTIFY: AudioSource = {
  id: 'app:1',
  kind: 'app',
  name: 'Spotify',
  isDefault: false,
  isOutput: true,
};
const SPEAKERS: AudioSource = {
  id: 'device:1',
  kind: 'device',
  name: 'Speakers',
  isDefault: true,
  isOutput: true,
};
const MIC: AudioSource = {
  ...SPEAKERS,
  id: 'device:2',
  name: 'Microphone',
  isDefault: false,
  isOutput: false,
};

function status(state: SourceStatus['state'], message: string | null = null): SourceStatus {
  return { sourceId: 'x', state, message };
}

describe('sourceBadges', () => {
  it('marks the default device and whether it is an output or input', () => {
    expect(sourceBadges(SPEAKERS)).toEqual(['default', 'output']);
    expect(sourceBadges(MIC)).toEqual(['input']);
  });

  it('gives apps no direction', () => {
    expect(sourceBadges(SPOTIFY)).toEqual([]);
  });
});

describe('sourceNotice', () => {
  it('says nothing while listening', () => {
    expect(sourceNotice({ status: status('listening'), source: SPOTIFY })).toBeNull();
  });

  it('names an app that closed or a device that went away', () => {
    expect(sourceNotice({ status: status('ended'), source: SPOTIFY })?.message).toBe(
      'Spotify closed',
    );
    expect(sourceNotice({ status: status('ended'), source: SPEAKERS })?.message).toBe(
      'Speakers disconnected',
    );
    expect(sourceNotice({ status: status('ended'), source: null })?.message).toBe(
      'The source stopped',
    );
  });

  it('suggests protected audio when a source is silent', () => {
    expect(sourceNotice({ status: status('silent'), source: SPOTIFY })).toEqual({
      tone: 'warning',
      message: SILENT_SOURCE_MESSAGE,
      detail: null,
    });
  });

  it('keeps the backend message as detail when capture fails', () => {
    expect(sourceNotice({ status: status('failed', 'device in use'), source: SPEAKERS })).toEqual({
      tone: 'error',
      message: "Couldn't listen to Speakers",
      detail: 'device in use',
    });
  });
});

describe('errorMessage', () => {
  it('reads strings, errors and anything else', () => {
    expect(errorMessage('no such source')).toBe('no such source');
    expect(errorMessage(new Error('boom'))).toBe('boom');
    expect(errorMessage(42)).toBe('42');
  });
});
