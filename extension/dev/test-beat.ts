// Local pages that synthesize known music with Web Audio, so Hunch has something to hear: the
// test beat, and the test mix, which goes from the test beat straight into a faster song in
// another key, with no gap. apps/desktop/src-tauri/src/test_beat.rs renders the same audio.
import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import type { Page } from 'puppeteer-core';

const LOCAL_HOST = '127.0.0.1';

export interface TestBeatServer {
  url: string;
  close: () => Promise<void>;
}

/** One song of a test page: a looped chord progression over the drum pattern. */
interface Song {
  bpm: number;
  /** The overlay should land on this key or its relative. */
  scale: string;
  bars: number;
  /** MIDI notes per bar: the bass, then the chord. */
  chords: number[][];
  description: string;
}

const A_MINOR_CHORDS = [
  [45, 57, 60, 64], // Am: A2 bass, A3 C4 E4
  [50, 62, 65, 69], // Dm
  [52, 64, 67, 71], // Em
  [45, 57, 60, 64], // Am
];
/** The A minor progression a minor third down. */
const F_SHARP_MINOR_CHORDS = [
  [42, 54, 57, 61], // F#m
  [47, 59, 62, 66], // Bm
  [49, 61, 64, 68], // C#m
  [42, 54, 57, 61], // F#m
];

export const TEST_BEAT = {
  bpm: 120,
  /** A natural minor: the overlay should land on A minor or its relative, C major. */
  scale: 'A minor',
  bars: 16,
  chords: A_MINOR_CHORDS,
  description: 'Am, Dm, Em, Am, with kick, snare and hats',
} satisfies Song;

const MIX_OPENER = { ...TEST_BEAT, bars: 20 } satisfies Song;

/** 40 s of the test beat, then 41 s at 140 BPM in F# minor, looping: Hunch should split on each change. */
export const TEST_MIX: [Song, Song] = [
  MIX_OPENER,
  {
    bpm: 140,
    scale: 'F# minor',
    bars: 24,
    chords: F_SHARP_MINOR_CHORDS,
    description: 'F#m, Bm, C#m, F#m',
  },
];

const BEATS_PER_BAR = 4;

function songSeconds(song: Song): number {
  return (song.bars * BEATS_PER_BAR * 60) / song.bpm;
}

/** Seconds into the test mix where its first song gives way to the second. */
export const TEST_MIX_CHANGE_SECONDS = songSeconds(MIX_OPENER);

function songLine(song: Song): string {
  return `<li>${String(song.bpm)} BPM, ${song.scale}: ${song.description}.</li>`;
}

function page(params: { title: string; songs: Song[] }): string {
  const { title, songs } = params;
  return /* html */ `<!doctype html>
<meta charset="utf-8" />
<title>${title}</title>
<style>body { font: 16px system-ui; margin: 40px; background: #fafafa; }</style>
<h1>${title}</h1>
<audio controls loop></audio>
<ul>${songs.map(songLine).join('')}</ul>
<script>
const SONGS = ${JSON.stringify(songs.map(({ bpm, bars, chords }) => ({ bpm, bars, chords })))};
const SAMPLE_RATE = 44100;
const songSeconds = ({ bpm, bars }) => (bars * 4 * 60) / bpm;

// Rendered offline and played through a looping <audio> element: tab capture misses Web Audio
// contexts that were already running when capture started, but always hears media elements.
const length = SONGS.reduce((total, song) => total + songSeconds(song), 0);
const context = new OfflineAudioContext(1, Math.ceil(length * SAMPLE_RATE), SAMPLE_RATE);
const master = context.createGain();
master.gain.value = 0.5;
master.connect(context.destination);
const noise = context.createBuffer(1, SAMPLE_RATE * 0.2, SAMPLE_RATE);
noise.getChannelData(0).forEach((_, i, data) => (data[i] = Math.random() * 2 - 1));

const hz = (midi) => 440 * 2 ** ((midi - 69) / 12);

function tone({ midi, start, length, gain, type }) {
  const oscillator = context.createOscillator();
  const envelope = context.createGain();
  oscillator.type = type;
  oscillator.frequency.value = hz(midi);
  envelope.gain.setValueAtTime(0, start);
  envelope.gain.linearRampToValueAtTime(gain, start + 0.02);
  envelope.gain.setTargetAtTime(0, start + length - 0.1, 0.05);
  oscillator.connect(envelope).connect(master);
  oscillator.start(start);
  oscillator.stop(start + length);
}

function kick(start) {
  const oscillator = context.createOscillator();
  const envelope = context.createGain();
  oscillator.frequency.setValueAtTime(120, start);
  oscillator.frequency.exponentialRampToValueAtTime(40, start + 0.15);
  envelope.gain.setValueAtTime(1, start);
  envelope.gain.exponentialRampToValueAtTime(0.001, start + 0.3);
  oscillator.connect(envelope).connect(master);
  oscillator.start(start);
  oscillator.stop(start + 0.3);
}

function hit({ start, length, gain, highpass }) {
  const source = context.createBufferSource();
  const filter = context.createBiquadFilter();
  const envelope = context.createGain();
  source.buffer = noise;
  filter.type = 'highpass';
  filter.frequency.value = highpass;
  envelope.gain.setValueAtTime(gain, start);
  envelope.gain.exponentialRampToValueAtTime(0.001, start + length);
  source.connect(filter).connect(envelope).connect(master);
  source.start(start);
  source.stop(start + length);
}

let songStart = 0;
for (const song of SONGS) {
  const beat = 60 / song.bpm;
  const bar = beat * 4;
  for (let index = 0; index < song.bars; index++) {
    const barStart = songStart + index * bar;
    const [bass, ...chord] = song.chords[index % song.chords.length];
    tone({ midi: bass, start: barStart, length: bar, gain: 0.25, type: 'triangle' });
    for (const midi of chord) tone({ midi, start: barStart, length: bar, gain: 0.08, type: 'sawtooth' });
    for (let step = 0; step < 4; step++) {
      const beatStart = barStart + step * beat;
      if (step % 2 === 0) kick(beatStart);
      else hit({ start: beatStart, length: 0.15, gain: 0.5, highpass: 1500 });
      hit({ start: beatStart, length: 0.04, gain: 0.15, highpass: 7000 });
      hit({ start: beatStart + beat / 2, length: 0.04, gain: 0.1, highpass: 7000 });
    }
  }
  songStart += songSeconds(song);
}

/** 16-bit mono PCM WAV. */
function encodeWav(samples) {
  const view = new DataView(new ArrayBuffer(44 + samples.length * 2));
  const ascii = (offset, text) => [...text].forEach((c, i) => view.setUint8(offset + i, c.charCodeAt(0)));
  ascii(0, 'RIFF');
  view.setUint32(4, 36 + samples.length * 2, true);
  ascii(8, 'WAVEfmt ');
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, SAMPLE_RATE, true);
  view.setUint32(28, SAMPLE_RATE * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  ascii(36, 'data');
  view.setUint32(40, samples.length * 2, true);
  samples.forEach((sample, i) => view.setInt16(44 + i * 2, Math.max(-1, Math.min(1, sample)) * 0x7fff, true));
  return new Blob([view], { type: 'audio/wav' });
}

context.startRendering().then((rendered) => {
  const audio = document.querySelector('audio');
  audio.src = URL.createObjectURL(encodeWav(rendered.getChannelData(0)));
  return audio.play();
});
</script>
`;
}

/** Serves a test page on a free localhost port. Overlays only inject into http(s) pages. */
async function serve(html: string): Promise<TestBeatServer> {
  const server = createServer((_request, response) => {
    response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
    response.end(html);
  });
  await new Promise<void>((resolve) => {
    server.listen(0, LOCAL_HOST, resolve);
  });
  // Serving alone shouldn't keep node running once the browser has closed.
  server.unref();
  const { port } = server.address() as AddressInfo;
  return {
    url: `http://${LOCAL_HOST}:${String(port)}/`,
    close: () =>
      new Promise<void>((resolve) => {
        server.close(() => {
          resolve();
        });
      }),
  };
}

export function startTestBeat(): Promise<TestBeatServer> {
  return serve(page({ title: 'Hunch test beat', songs: [TEST_BEAT] }));
}

export function startTestMix(): Promise<TestBeatServer> {
  return serve(page({ title: 'Hunch test mix', songs: TEST_MIX }));
}

/** Resolves once the test page's audio is playing. */
export async function waitForTestBeat(page: Page): Promise<void> {
  await page.waitForFunction(() => document.querySelector('audio')?.paused === false);
}
