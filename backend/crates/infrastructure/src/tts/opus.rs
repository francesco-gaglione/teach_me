//! Ogg Opus muxing (RFC 7845) of mono PCM at 24 kHz.

use std::io::Cursor;

use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use opus::{Application, Bitrate, Channels, Encoder};

/// Rate of the PCM the engine returns; Opus takes it natively.
pub const SAMPLE_RATE: u32 = 24_000;
/// Target bit rate of the clips (ADR 0013 §12).
pub const BITRATE: i32 = 32_000;
/// 20 ms frames.
const FRAME: usize = 480;
/// Granule positions of Ogg Opus always count 48 kHz samples.
const GRANULE_PER_FRAME: u64 = 960;
const SERIAL: u32 = 0x7465_6163; // arbitrary: one logical stream per file

#[derive(Debug, thiserror::Error)]
pub enum OpusError {
    #[error("there is no audio to encode")]
    Empty,
    #[error("opus encoder: {0}")]
    Encoder(String),
    #[error("ogg muxer: {0}")]
    Mux(String),
}

/// A finished Ogg Opus file and how long it plays.
#[derive(Debug)]
pub struct Encoded {
    pub audio: Vec<u8>,
    pub duration_ms: u32,
}

/// Encodes `pcm` (mono, [`SAMPLE_RATE`]) as Ogg Opus at [`BITRATE`].
pub fn encode_ogg_opus(pcm: &[i16]) -> Result<Encoded, OpusError> {
    if pcm.is_empty() {
        return Err(OpusError::Empty);
    }
    let enc_err = |e: opus::Error| OpusError::Encoder(e.to_string());
    let mux_err = |e: std::io::Error| OpusError::Mux(e.to_string());

    let mut encoder =
        Encoder::new(SAMPLE_RATE, Channels::Mono, Application::Audio).map_err(enc_err)?;
    encoder
        .set_bitrate(Bitrate::Bits(BITRATE))
        .map_err(enc_err)?;
    let lookahead = u64::try_from(encoder.get_lookahead().map_err(enc_err)?)
        .map_err(|_| OpusError::Encoder("negative lookahead".into()))?;
    // Pre-skip is in 48 kHz samples; the lookahead is in input samples.
    let scale = u64::from(48_000 / SAMPLE_RATE);
    let pre_skip = lookahead * scale;
    let samples = pcm.len() as u64;
    let end_granule = pre_skip + samples * scale;
    // Enough frames to flush the encoder delay, so the tail is not lost.
    let frames = (samples + lookahead).div_ceil(FRAME as u64);

    let mut writer = PacketWriter::new(Cursor::new(Vec::new()));
    let pre_skip_field =
        u16::try_from(pre_skip).map_err(|_| OpusError::Encoder("pre-skip out of range".into()))?;
    let mut head = Vec::with_capacity(19);
    head.extend_from_slice(b"OpusHead");
    head.push(1); // version
    head.push(1); // channels
    head.extend_from_slice(&pre_skip_field.to_le_bytes());
    head.extend_from_slice(&SAMPLE_RATE.to_le_bytes()); // informational
    head.extend_from_slice(&0i16.to_le_bytes()); // output gain
    head.push(0); // channel mapping family 0
    writer
        .write_packet(head, SERIAL, PacketWriteEndInfo::EndPage, 0)
        .map_err(mux_err)?;

    let vendor = b"teach_me";
    let mut tags = Vec::new();
    tags.extend_from_slice(b"OpusTags");
    tags.extend_from_slice(&u32::try_from(vendor.len()).unwrap_or(0).to_le_bytes());
    tags.extend_from_slice(vendor);
    tags.extend_from_slice(&0u32.to_le_bytes()); // no user comments
    writer
        .write_packet(tags, SERIAL, PacketWriteEndInfo::EndPage, 0)
        .map_err(mux_err)?;

    let mut out = [0u8; 1500];
    let mut frame = [0i16; FRAME];
    for n in 0..frames {
        let start = usize::try_from(n)
            .unwrap_or(usize::MAX)
            .saturating_mul(FRAME);
        frame.fill(0);
        if start < pcm.len() {
            let end = (start + FRAME).min(pcm.len());
            frame[..end - start].copy_from_slice(&pcm[start..end]);
        }
        let len = encoder.encode(&frame, &mut out).map_err(enc_err)?;
        let last = n + 1 == frames;
        let (info, granule) = if last {
            (PacketWriteEndInfo::EndStream, end_granule)
        } else {
            (
                PacketWriteEndInfo::NormalPacket,
                (n + 1) * GRANULE_PER_FRAME,
            )
        };
        writer
            .write_packet(out[..len].to_vec(), SERIAL, info, granule)
            .map_err(mux_err)?;
    }

    let duration_ms =
        u32::try_from((samples * 1000 + u64::from(SAMPLE_RATE) / 2) / u64::from(SAMPLE_RATE))
            .map_err(|_| OpusError::Encoder("clip too long".into()))?;
    Ok(Encoded {
        audio: writer.into_inner().into_inner(),
        duration_ms: duration_ms.max(1),
    })
}

#[cfg(test)]
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
mod tests {
    use ogg::reading::PacketReader;
    use opus::Decoder;

    use super::*;

    fn tone(samples: usize) -> Vec<i16> {
        (0..samples)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                ((t * 440.0 * std::f32::consts::TAU).sin() * 12_000.0) as i16
            })
            .collect()
    }

    /// Reads the file back with an independent path: ogg demux + libopus decode.
    fn decode(audio: &[u8]) -> (Vec<u8>, Vec<u8>, usize, u64) {
        let mut reader = PacketReader::new(Cursor::new(audio));
        let head = reader.read_packet_expected().unwrap();
        let tags = reader.read_packet_expected().unwrap();
        assert!(head.first_in_stream() && !tags.first_in_stream());
        assert!(head.data.starts_with(b"OpusHead"));
        assert!(tags.data.starts_with(b"OpusTags"));
        let pre_skip = usize::from(u16::from_le_bytes([head.data[10], head.data[11]]));
        let mut decoder = Decoder::new(48_000, Channels::Mono).unwrap();
        let mut pcm = Vec::new();
        let mut last_granule = 0;
        while let Some(p) = reader.read_packet().unwrap() {
            let mut buf = [0i16; 5760];
            let n = decoder.decode(&p.data, &mut buf, false).unwrap();
            pcm.extend_from_slice(&buf[..n]);
            last_granule = p.absgp_page();
        }
        // Granule 0 is the end of the headers; the stream ends at `last_granule`.
        let playable = (last_granule as usize).saturating_sub(pre_skip);
        assert!(pcm.len() >= pre_skip + playable);
        (head.data, tags.data, playable, last_granule)
    }

    #[test]
    fn rejects_empty_audio() {
        assert!(matches!(encode_ogg_opus(&[]), Err(OpusError::Empty)));
    }

    #[test]
    fn a_tone_round_trips_with_the_right_length() {
        let pcm = tone(24_000 * 3 + 123);
        let encoded = encode_ogg_opus(&pcm).unwrap();
        assert_eq!(encoded.duration_ms, 3005);
        assert!(encoded.audio.starts_with(b"OggS"));
        let (head, _, playable_48k, _) = decode(&encoded.audio);
        assert_eq!(head[9], 1, "mono");
        assert_eq!(playable_48k, pcm.len() * 2);
        // 32 kbit/s is about 4 kB a second.
        let per_second = encoded.audio.len() as f32 / 3.005;
        assert!((2_500.0..6_000.0).contains(&per_second), "{per_second}");
    }

    #[test]
    fn a_very_short_clip_is_still_valid() {
        let encoded = encode_ogg_opus(&tone(100)).unwrap();
        assert_eq!(encoded.duration_ms, 4);
        let (_, _, playable_48k, _) = decode(&encoded.audio);
        assert_eq!(playable_48k, 200);
    }

    #[test]
    fn the_same_audio_gives_the_same_file() {
        let pcm = tone(24_000);
        assert_eq!(
            encode_ogg_opus(&pcm).unwrap().audio,
            encode_ogg_opus(&pcm).unwrap().audio
        );
    }
}
