// A local page that synthesizes a known beat with Web Audio, so Hunch has something to hear.
import { createServer } from 'node:http';

export const TEST_BEAT = {
  bpm: 120,
  /** A natural minor: the overlay should land on A minor or its relative, C major. */
  scale: 'A minor',
};

const PAGE = /* html */ `<!doctype html>
<meta charset="utf-8" />
<title>Hunch test beat</title>
<style>body { font: 16px system-ui; margin: 40px; background: #fafafa; }</style>
<h1>Hunch test beat</h1>
<audio controls loop></audio>
<p>${String(TEST_BEAT.bpm)} BPM, ${TEST_BEAT.scale}: Am, Dm, Em, Am, with kick, snare and hats.</p>
<script>
const BPM = ${String(TEST_BEAT.bpm)};
const BEAT = 60 / BPM;
const BAR = BEAT * 4;
const CHORDS = [
  [45, 57, 60, 64], // Am: A2 bass, A3 C4 E4
  [50, 62, 65, 69], // Dm
  [52, 64, 67, 71], // Em
  [45, 57, 60, 64], // Am
];
const LOOP_BARS = 16;
const SAMPLE_RATE = 44100;

// Rendered offline and played through a looping <audio> element: tab capture misses Web Audio
// contexts that were already running when capture started, but always hears media elements.
const length = LOOP_BARS * BAR;
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

for (let bar = 0; bar < LOOP_BARS; bar++) {
  const barStart = bar * BAR;
  const [bass, ...chord] = CHORDS[bar % CHORDS.length];
  tone({ midi: bass, start: barStart, length: BAR, gain: 0.25, type: 'triangle' });
  for (const midi of chord) tone({ midi, start: barStart, length: BAR, gain: 0.08, type: 'sawtooth' });
  for (let beat = 0; beat < 4; beat++) {
    const beatStart = barStart + beat * BEAT;
    if (beat % 2 === 0) kick(beatStart);
    else hit({ start: beatStart, length: 0.15, gain: 0.5, highpass: 1500 });
    hit({ start: beatStart, length: 0.04, gain: 0.15, highpass: 7000 });
    hit({ start: beatStart + BEAT / 2, length: 0.04, gain: 0.1, highpass: 7000 });
  }
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

/** Serves the test page on a free localhost port. Overlays only inject into http(s) pages. */
export async function startTestPage() {
  const server = createServer((_request, response) => {
    response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
    response.end(PAGE);
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const { port } = server.address();
  return {
    url: `http://127.0.0.1:${String(port)}/`,
    close: () => new Promise((resolve) => server.close(resolve)),
  };
}
