//! Minimal WAV reader: PCM 16-bit mono at a known rate, nothing else.

/// Why a WAV could not be used.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum WavError {
    #[error("not a RIFF/WAVE file")]
    NotWav,
    #[error("no `fmt ` chunk before the audio")]
    NoFormat,
    #[error("no `data` chunk")]
    NoData,
    #[error("unsupported format: {0}")]
    Unsupported(String),
}

/// Decodes a WAV that must be 16-bit PCM, mono, at `expected_rate` Hz.
///
/// Chunks are walked one by one rather than assuming a 44-byte header: servers
/// add `LIST` chunks, and a streaming writer may leave the `data` size at a
/// placeholder, in which case the rest of the file is the audio.
pub fn decode_pcm16_mono(bytes: &[u8], expected_rate: u32) -> Result<Vec<i16>, WavError> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(WavError::NotWav);
    }
    let mut pos = 12;
    let mut format_ok = false;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let declared = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        let body_start = pos + 8;
        let available = bytes.len() - body_start;
        match id {
            b"fmt " => {
                let body = bytes
                    .get(body_start..body_start + declared.min(available))
                    .filter(|b| b.len() >= 16)
                    .ok_or_else(|| WavError::Unsupported("short fmt chunk".into()))?;
                let tag = u16::from_le_bytes([body[0], body[1]]);
                let channels = u16::from_le_bytes([body[2], body[3]]);
                let rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
                let bits = u16::from_le_bytes([body[14], body[15]]);
                if tag != 1 {
                    return Err(WavError::Unsupported(format!("format tag {tag}")));
                }
                if channels != 1 {
                    return Err(WavError::Unsupported(format!("{channels} channels")));
                }
                if rate != expected_rate {
                    return Err(WavError::Unsupported(format!("{rate} Hz")));
                }
                if bits != 16 {
                    return Err(WavError::Unsupported(format!("{bits}-bit samples")));
                }
                format_ok = true;
            }
            b"data" => {
                if !format_ok {
                    return Err(WavError::NoFormat);
                }
                let len = declared.min(available);
                return Ok(bytes[body_start..body_start + len]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|s| i16::from_le_bytes(*s))
                    .collect());
            }
            _ => {}
        }
        // Chunks are padded to an even size.
        pos = body_start.saturating_add(declared + (declared & 1));
    }
    Err(if format_ok {
        WavError::NoData
    } else {
        WavError::NoFormat
    })
}

/// A WAV file for tests and fixtures.
#[cfg(test)]
pub fn encode_pcm16(samples: &[i16], rate: u32, channels: u16, bits: u16) -> Vec<u8> {
    let data_len = u32::try_from(samples.len() * 2).unwrap();
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_samples() {
        let wav = encode_pcm16(&[1, -2, 300], 24_000, 1, 16);
        assert_eq!(decode_pcm16_mono(&wav, 24_000).unwrap(), [1, -2, 300]);
    }

    #[test]
    fn skips_other_chunks_before_the_audio() {
        let plain = encode_pcm16(&[5, 6], 24_000, 1, 16);
        let mut wav = plain[..36].to_vec();
        wav.extend_from_slice(b"LIST");
        wav.extend_from_slice(&3u32.to_le_bytes());
        wav.extend_from_slice(b"abc\0"); // odd size, padded
        wav.extend_from_slice(&plain[36..]);
        assert_eq!(decode_pcm16_mono(&wav, 24_000).unwrap(), [5, 6]);
    }

    #[test]
    fn a_placeholder_data_size_means_the_rest_of_the_file() {
        let mut wav = encode_pcm16(&[5, 6, 7], 24_000, 1, 16);
        wav[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(decode_pcm16_mono(&wav, 24_000).unwrap(), [5, 6, 7]);
    }

    #[test]
    fn rejects_other_formats() {
        let bad = |wav: Vec<u8>| decode_pcm16_mono(&wav, 24_000).unwrap_err();
        assert!(matches!(
            bad(encode_pcm16(&[1], 44_100, 1, 16)),
            WavError::Unsupported(_)
        ));
        assert!(matches!(
            bad(encode_pcm16(&[1, 2], 24_000, 2, 16)),
            WavError::Unsupported(_)
        ));
        assert!(matches!(
            bad(encode_pcm16(&[1], 24_000, 1, 8)),
            WavError::Unsupported(_)
        ));
        assert_eq!(bad(b"not a wav at all".to_vec()), WavError::NotWav);
        assert_eq!(bad(b"RIFF\0\0\0\0WAVE".to_vec()), WavError::NoFormat);
    }

    #[test]
    fn no_data_chunk_is_an_error() {
        let wav = encode_pcm16(&[], 24_000, 1, 16);
        // `data` chunk with zero length is present and fine.
        assert_eq!(decode_pcm16_mono(&wav, 24_000).unwrap(), Vec::<i16>::new());
        assert_eq!(decode_pcm16_mono(&wav[..36], 24_000), Err(WavError::NoData));
    }
}
