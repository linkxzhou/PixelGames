//! RIFF/WAVE writer and reader for the one format this pipeline emits:
//! 16-bit PCM, little endian, 44.1 kHz, mono or stereo.

use crate::error::{AudioError, AudioResult};
use crate::limits::SAMPLE_RATE;

pub const HEADER_BYTES: usize = 44;

/// Quantize float PCM to signed 16-bit. Rounds half away from zero with no
/// dither, so the same floats always give the same bytes. Returns the samples
/// and the number of samples that exceeded full scale *before* clamping.
pub fn quantize(pcm: &[f32]) -> (Vec<i16>, u64) {
    let mut clipped = 0u64;
    let out = pcm
        .iter()
        .map(|&sample| {
            let scaled = sample as f64 * 32768.0;
            let rounded = scaled.round();
            if !(-32768.0..=32767.0).contains(&rounded) {
                clipped += 1;
            }
            rounded.clamp(-32768.0, 32767.0) as i16
        })
        .collect();
    (out, clipped)
}

pub fn encode(samples: &[i16], channels: u16) -> AudioResult<Vec<u8>> {
    if channels == 0 || channels > 2 {
        return Err(AudioError::semantic("wav writer supports 1 or 2 channels"));
    }
    if (samples.len()) % (channels as usize) != 0 {
        return Err(AudioError::semantic(
            "sample count is not a multiple of the channel count",
        ));
    }
    let data_bytes = samples.len() * 2;
    if data_bytes > u32::MAX as usize - 36 {
        return Err(AudioError::limit("wav data exceeds 4 GiB"));
    }
    let block_align = channels * 2;
    let mut out = Vec::with_capacity(HEADER_BYTES + data_bytes);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data_bytes) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * block_align as u32).to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_bytes as u32).to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wav {
    pub channels: u16,
    pub sample_rate: u32,
    pub frames: u64,
    /// The interleaved little-endian PCM bytes, exactly as stored.
    pub pcm: Vec<u8>,
}

impl Wav {
    pub fn samples(&self) -> Vec<i16> {
        self.pcm
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect()
    }
}

/// Parse a canonical 44-byte-header PCM WAV. Anything else is rejected rather
/// than guessed at.
pub fn decode(bytes: &[u8]) -> AudioResult<Wav> {
    if bytes.len() < HEADER_BYTES {
        return Err(AudioError::semantic(
            "wav is shorter than the 44-byte header",
        ));
    }
    if &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(AudioError::semantic("not a RIFF/WAVE file"));
    }
    if &bytes[12..16] != b"fmt " || u32::from_le_bytes(bytes[16..20].try_into().unwrap()) != 16 {
        return Err(AudioError::semantic(
            "unsupported fmt chunk; expected a 16-byte PCM fmt",
        ));
    }
    let format = u16::from_le_bytes(bytes[20..22].try_into().unwrap());
    let channels = u16::from_le_bytes(bytes[22..24].try_into().unwrap());
    let sample_rate = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
    let byte_rate = u32::from_le_bytes(bytes[28..32].try_into().unwrap());
    let block_align = u16::from_le_bytes(bytes[32..34].try_into().unwrap());
    let bits = u16::from_le_bytes(bytes[34..36].try_into().unwrap());
    if format != 1 || bits != 16 {
        return Err(AudioError::semantic("only 16-bit integer PCM is supported"));
    }
    if channels == 0 || channels > 2 {
        return Err(AudioError::semantic("only mono or stereo wav is supported"));
    }
    if block_align != channels * 2 || byte_rate != sample_rate * block_align as u32 {
        return Err(AudioError::semantic("wav header fields are inconsistent"));
    }
    if &bytes[36..40] != b"data" {
        return Err(AudioError::semantic(
            "expected a data chunk directly after fmt",
        ));
    }
    let data_len = u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize;
    if bytes.len() != HEADER_BYTES + data_len {
        return Err(AudioError::semantic(format!(
            "wav data chunk says {data_len} bytes but the file holds {}",
            bytes.len() - HEADER_BYTES
        )));
    }
    let riff = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    if riff != 36 + data_len {
        return Err(AudioError::semantic(
            "RIFF size does not match the data size",
        ));
    }
    if (data_len) % (block_align as usize) != 0 {
        return Err(AudioError::semantic(
            "data length is not a whole number of frames",
        ));
    }
    Ok(Wav {
        channels,
        sample_rate,
        frames: (data_len / block_align as usize) as u64,
        pcm: bytes[HEADER_BYTES..].to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantize_rounds_and_counts_overs_before_clamping() {
        let (q, clipped) = quantize(&[0.0, 0.5, -0.5, 1.0, 1.5, -2.0]);
        assert_eq!(q, vec![0, 16384, -16384, 32767, 32767, -32768]);
        // 1.0 -> 32768 is over full scale, 1.5 and -2.0 as well; -0.5 and 0.5 are not.
        assert_eq!(clipped, 3);
    }

    #[test]
    fn encode_decode_roundtrip_mono_and_stereo() {
        let mono = encode(&[1, -2, 3], 1).unwrap();
        let w = decode(&mono).unwrap();
        assert_eq!((w.channels, w.frames), (1, 3));
        assert_eq!(w.samples(), vec![1, -2, 3]);
        let stereo = encode(&[1, 2, 3, 4], 2).unwrap();
        assert_eq!(decode(&stereo).unwrap().frames, 2);
    }

    #[test]
    fn decode_rejects_truncated_and_inconsistent_files() {
        let mut wav = encode(&[1, 2, 3, 4], 2).unwrap();
        wav.pop();
        assert!(decode(&wav).is_err());
        let mut wav = encode(&[1, 2], 1).unwrap();
        wav[34] = 8;
        assert!(decode(&wav).is_err());
        assert!(decode(b"nope").is_err());
        assert!(encode(&[1, 2, 3], 2).is_err());
    }

    #[test]
    fn identical_input_gives_identical_bytes() {
        let (q, _) = quantize(&[0.1, -0.2, 0.3, 0.4]);
        assert_eq!(encode(&q, 2).unwrap(), encode(&q, 2).unwrap());
    }
}
