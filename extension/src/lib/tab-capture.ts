import workletUrl from '@/worklets/downmix.ts?worker&url';
import { ANALYSIS_SAMPLE_RATE, DOWNMIX_PROCESSOR_NAME } from './capture-config';

/** Chrome's legacy constraint shape for tab capture; the standard types don't know it. */
interface TabAudioConstraints {
  audio: { mandatory: { chromeMediaSource: 'tab'; chromeMediaSourceId: string } };
}

/** A live tab capture in the offscreen document. */
export interface TabCapture {
  /** Receives 100 ms mono chunks at `ANALYSIS_SAMPLE_RATE`. Transfer it to the analysis Worker. */
  chunkPort: MessagePort;
  stop(): Promise<void>;
}

function getTabStream(streamId: string): Promise<MediaStream> {
  const constraints: TabAudioConstraints = {
    audio: { mandatory: { chromeMediaSource: 'tab', chromeMediaSourceId: streamId } },
  };
  return navigator.mediaDevices.getUserMedia(constraints as MediaStreamConstraints);
}

/**
 * Captures a tab's audio. Playback goes through a default-rate context so the user keeps hearing
 * the tab at full quality (capture mutes it otherwise); analysis runs in a separate 22,050 Hz
 * context whose worklet downmixes and posts chunks to `chunkPort`.
 */
export async function startTabCapture(params: {
  streamId: string;
  onEnded: () => void;
}): Promise<TabCapture> {
  const stream = await getTabStream(params.streamId);
  const playback = new AudioContext();
  const analysis = new AudioContext({ sampleRate: ANALYSIS_SAMPLE_RATE });

  const release = async (): Promise<void> => {
    for (const track of stream.getTracks()) {
      track.removeEventListener('ended', params.onEnded);
      track.stop();
    }
    await Promise.all([playback.close(), analysis.close()]);
  };

  try {
    playback.createMediaStreamSource(stream).connect(playback.destination);

    await analysis.audioWorklet.addModule(workletUrl);
    const downmix = new AudioWorkletNode(analysis, DOWNMIX_PROCESSOR_NAME, {
      numberOfOutputs: 0,
    });
    analysis.createMediaStreamSource(stream).connect(downmix);
    const channel = new MessageChannel();
    downmix.port.postMessage(null, [channel.port1]);

    // Offscreen documents have no user gesture, so make sure neither context starts suspended.
    await Promise.all([playback.resume(), analysis.resume()]);
    for (const track of stream.getTracks()) track.addEventListener('ended', params.onEnded);

    return {
      chunkPort: channel.port2,
      stop: async () => {
        downmix.port.close();
        downmix.disconnect();
        await release();
      },
    };
  } catch (error) {
    await release();
    throw error;
  }
}
