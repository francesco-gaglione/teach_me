//! Text-to-speech adapters (ADR 0013): today one HTTP engine, Qwen3-TTS.

mod opus;
mod qwen3;
mod wav;

pub use opus::{BITRATE, Encoded, OpusError, SAMPLE_RATE, encode_ogg_opus};
pub use qwen3::{Qwen3TtsBuildError, Qwen3TtsConfig, Qwen3TtsSynthesizer, parse_voices};
pub use wav::{WavError, decode_pcm16_mono};
