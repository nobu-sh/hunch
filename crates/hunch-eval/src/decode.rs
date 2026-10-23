use std::fs::File;
use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub struct MonoAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

/// Decodes the first audio track of `path` and averages its channels down to mono.
pub fn decode_mono(path: &Path) -> Result<MonoAudio, String> {
    let file = File::open(path).map_err(|error| format!("cannot open: {error}"))?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
        hint.with_extension(extension);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|error| format!("unrecognized format: {error}"))?;
    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or("no audio track")?;
    let track_id = track.id;
    let sample_rate = track
        .codec_params
        .sample_rate
        .ok_or("unknown sample rate")?;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|error| format!("unsupported codec: {error}"))?;

    let mut samples = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            // Symphonia signals end of stream as an unexpected EOF.
            Err(SymphoniaError::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(error) => return Err(format!("read failed: {error}")),
        };
        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            // A corrupt packet shouldn't sink the whole file.
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(error) => return Err(format!("decode failed: {error}")),
        };

        let channel_count = decoded.spec().channels.count();
        let mut buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
        buffer.copy_interleaved_ref(decoded);
        downmix_into(&mut samples, buffer.samples(), channel_count);
    }

    Ok(MonoAudio {
        samples,
        sample_rate,
    })
}

fn downmix_into(mono: &mut Vec<f32>, interleaved: &[f32], channel_count: usize) {
    let channel_count = channel_count.max(1);
    mono.extend(
        interleaved
            .chunks_exact(channel_count)
            .map(|frame| frame.iter().sum::<f32>() / channel_count as f32),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RATE: u32 = 44_100;
    const SINE_HZ: f32 = 440.0;
    const AMPLITUDE: f32 = 0.5;

    fn write_stereo_sine_wav(path: &Path, seconds: u32) {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).expect("create wav");
        for n in 0..SAMPLE_RATE * seconds {
            let t = n as f32 / SAMPLE_RATE as f32;
            let sample = (t * SINE_HZ * std::f32::consts::TAU).sin() * AMPLITUDE;
            let value = (sample * f32::from(i16::MAX)) as i16;
            writer.write_sample(value).expect("left sample");
            writer.write_sample(value).expect("right sample");
        }
        writer.finalize().expect("finalize wav");
    }

    #[test]
    fn decodes_stereo_wav_to_mono() {
        let path = std::env::temp_dir().join("hunch-eval-decode-test.wav");
        write_stereo_sine_wav(&path, 3);

        let audio = decode_mono(&path).expect("decode");
        std::fs::remove_file(&path).ok();

        assert_eq!(audio.sample_rate, SAMPLE_RATE);
        assert_eq!(audio.samples.len(), (SAMPLE_RATE * 3) as usize);
        let peak = audio.samples.iter().fold(0.0f32, |max, s| max.max(s.abs()));
        assert!((peak - AMPLITUDE).abs() < 0.01, "peak was {peak}");
    }

    #[test]
    fn rejects_non_audio_file() {
        let path = std::env::temp_dir().join("hunch-eval-not-audio.wav");
        std::fs::write(&path, b"definitely not audio").expect("write file");

        let result = decode_mono(&path);
        std::fs::remove_file(&path).ok();

        assert!(result.is_err());
    }

    #[test]
    fn downmix_averages_channels() {
        let mut mono = Vec::new();
        downmix_into(&mut mono, &[1.0, 0.0, 0.5, 0.5], 2);
        assert_eq!(mono, vec![0.5, 0.5]);
    }
}
