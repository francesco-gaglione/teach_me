//! Qwen3-TTS adapter against a fake engine on 127.0.0.1: no real TTS needed.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use domain::{ContentLanguage, SpeechError, SpeechSynthesizer};
use infrastructure::{Qwen3TtsConfig, Qwen3TtsSynthesizer, parse_voices};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Debug, Clone)]
struct Captured {
    request_line: String,
    content_type: Option<String>,
    body: Value,
}

struct Fake {
    base_url: String,
    seen: Arc<Mutex<Vec<Captured>>>,
}

/// Answers every request with `status` and `body`.
async fn fake_engine(status: u16, content_type: &str, body: Vec<u8>) -> Fake {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let content_type = content_type.to_owned();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let log = log.clone();
            let content_type = content_type.clone();
            let body = body.clone();
            tokio::spawn(async move {
                let mut raw = Vec::new();
                let mut buf = [0u8; 4096];
                let (head_end, length) = loop {
                    let n = socket.read(&mut buf).await.unwrap();
                    raw.extend_from_slice(&buf[..n]);
                    if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&raw[..pos]).to_lowercase();
                        let length = head
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length:"))
                            .map_or(0, |v| v.trim().parse::<usize>().unwrap());
                        break (pos + 4, length);
                    }
                };
                while raw.len() < head_end + length {
                    let n = socket.read(&mut buf).await.unwrap();
                    raw.extend_from_slice(&buf[..n]);
                }
                let head = String::from_utf8_lossy(&raw[..head_end]).to_string();
                let header = |name: &str| {
                    head.lines().find_map(|l| {
                        let (k, v) = l.split_once(':')?;
                        k.eq_ignore_ascii_case(name).then(|| v.trim().to_owned())
                    })
                };
                log.lock().unwrap().push(Captured {
                    request_line: head.lines().next().unwrap().to_owned(),
                    content_type: header("content-type"),
                    body: serde_json::from_slice(&raw[head_end..head_end + length]).unwrap(),
                });
                let reply = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                socket.write_all(reply.as_bytes()).await.unwrap();
                socket.write_all(&body).await.unwrap();
                let _ = socket.shutdown().await;
            });
        }
    });
    Fake { base_url, seen }
}

fn wav(samples: &[i16], rate: u32) -> Vec<u8> {
    let data = u32::try_from(samples.len() * 2).unwrap();
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

fn synthesizer(base_url: &str) -> Qwen3TtsSynthesizer {
    let mut config = Qwen3TtsConfig::new(
        base_url,
        parse_voices("voice_a:Alfa,voice_b:Beta"),
        "voice_a",
    );
    config.request_timeout = Duration::from_secs(10);
    Qwen3TtsSynthesizer::new(config).unwrap()
}

fn second_of_tone() -> Vec<i16> {
    (0..24_000i16).map(|i| (i % 100) * 200 - 10_000).collect()
}

#[tokio::test]
async fn sends_the_documented_request_and_returns_ogg_opus() {
    let fake = fake_engine(200, "audio/wav", wav(&second_of_tone(), 24_000)).await;
    let tts = synthesizer(&fake.base_url);

    let clip = tts
        .synthesize("Ciao a tutti.", "voice_b", ContentLanguage::It)
        .await
        .unwrap();

    assert!(clip.audio.starts_with(b"OggS"));
    assert!(clip.audio.windows(8).any(|w| w == b"OpusHead"));
    assert_eq!(clip.duration_ms, 1000);
    let seen = fake.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].request_line, "POST /v1/tts HTTP/1.1");
    assert_eq!(seen[0].content_type.as_deref(), Some("application/json"));
    // Exactly the fields the engine accepts; it refuses unknown ones with 400.
    assert_eq!(
        seen[0].body,
        json!({"text": "Ciao a tutti.", "speaker": "voice_b", "language": "Italian", "seed": 42})
    );
}

#[tokio::test]
async fn the_language_of_the_topic_is_the_one_sent_to_the_engine() {
    let fake = fake_engine(200, "audio/wav", wav(&second_of_tone(), 24_000)).await;
    let tts = synthesizer(&fake.base_url);

    tts.synthesize("Hello.", "voice_a", ContentLanguage::En)
        .await
        .unwrap();

    let seen = fake.seen.lock().unwrap();
    assert_eq!(
        seen[0].body,
        json!({"text": "Hello.", "speaker": "voice_a", "language": "English", "seed": 42})
    );
}

#[tokio::test]
async fn the_same_request_is_repeatable() {
    let fake = fake_engine(200, "audio/wav", wav(&second_of_tone(), 24_000)).await;
    let tts = synthesizer(&fake.base_url);
    let a = tts
        .synthesize("t", "voice_a", ContentLanguage::It)
        .await
        .unwrap();
    let b = tts
        .synthesize("t", "voice_a", ContentLanguage::It)
        .await
        .unwrap();
    assert_eq!(a, b);
}

#[tokio::test]
async fn an_unlisted_voice_never_reaches_the_engine() {
    let fake = fake_engine(200, "audio/wav", wav(&second_of_tone(), 24_000)).await;
    let tts = synthesizer(&fake.base_url);
    let err = tts
        .synthesize("t", "ghost", ContentLanguage::It)
        .await
        .unwrap_err();
    assert!(matches!(err, SpeechError::UnknownVoice));
    assert!(fake.seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_rejected_request_is_a_failure_with_the_engine_message() {
    let body = br#"{"error":{"message":"unknown field 'voise'"}}"#.to_vec();
    let fake = fake_engine(400, "application/json", body).await;
    let err = synthesizer(&fake.base_url)
        .synthesize("t", "voice_a", ContentLanguage::It)
        .await
        .unwrap_err();
    match err {
        SpeechError::Failed(m) => assert!(m.contains("400") && m.contains("unknown field"), "{m}"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_busy_engine_is_unavailable() {
    let fake = fake_engine(503, "application/json", b"{}".to_vec()).await;
    let err = synthesizer(&fake.base_url)
        .synthesize("t", "voice_a", ContentLanguage::It)
        .await
        .unwrap_err();
    assert!(matches!(err, SpeechError::Unavailable(_)), "{err:?}");
}

#[tokio::test]
async fn an_engine_error_is_a_failure_that_uses_an_attempt() {
    for status in [500u16, 501, 507] {
        let fake = fake_engine(status, "application/json", b"{}".to_vec()).await;
        let err = synthesizer(&fake.base_url)
            .synthesize("t", "voice_a", ContentLanguage::It)
            .await
            .unwrap_err();
        assert!(matches!(err, SpeechError::Failed(_)), "{status}: {err:?}");
    }
    for status in [502u16, 504] {
        let fake = fake_engine(status, "application/json", b"{}".to_vec()).await;
        let err = synthesizer(&fake.base_url)
            .synthesize("t", "voice_a", ContentLanguage::It)
            .await
            .unwrap_err();
        assert!(
            matches!(err, SpeechError::Unavailable(_)),
            "{status}: {err:?}"
        );
    }
}

#[tokio::test]
async fn a_request_timeout_is_a_failure() {
    // Accepts the connection and never answers.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((socket, _)) = listener.accept().await {
            held.push(socket);
        }
    });
    let mut config = Qwen3TtsConfig::new(base_url, parse_voices("voice_a:Alfa"), "voice_a");
    config.request_timeout = Duration::from_millis(300);
    let err = Qwen3TtsSynthesizer::new(config)
        .unwrap()
        .synthesize("t", "voice_a", ContentLanguage::It)
        .await
        .unwrap_err();
    assert!(matches!(err, SpeechError::Failed(_)), "{err:?}");
}

#[tokio::test]
async fn nobody_listening_is_unavailable() {
    let err = synthesizer("http://127.0.0.1:1")
        .synthesize("t", "voice_a", ContentLanguage::It)
        .await
        .unwrap_err();
    assert!(matches!(err, SpeechError::Unavailable(_)), "{err:?}");
}

#[tokio::test]
async fn audio_in_the_wrong_format_is_a_failure() {
    let fake = fake_engine(200, "audio/wav", wav(&second_of_tone(), 44_100)).await;
    let err = synthesizer(&fake.base_url)
        .synthesize("t", "voice_a", ContentLanguage::It)
        .await
        .unwrap_err();
    assert!(matches!(err, SpeechError::Failed(_)), "{err:?}");

    let fake = fake_engine(200, "audio/wav", b"not audio".to_vec()).await;
    let err = synthesizer(&fake.base_url)
        .synthesize("t", "voice_a", ContentLanguage::It)
        .await
        .unwrap_err();
    assert!(matches!(err, SpeechError::Failed(_)), "{err:?}");

    let fake = fake_engine(200, "audio/wav", wav(&[], 24_000)).await;
    let err = synthesizer(&fake.base_url)
        .synthesize("t", "voice_a", ContentLanguage::It)
        .await
        .unwrap_err();
    assert!(matches!(err, SpeechError::Failed(_)), "{err:?}");
}

#[tokio::test]
async fn the_engine_identity_covers_model_voices_and_parameters() {
    let tts = synthesizer("http://127.0.0.1:1");
    let info = tts.engine();
    assert_eq!(info.engine, "qwen3-tts");
    assert!(info.params.contains("seed=42") && info.params.contains("Italian"));
    assert_eq!(tts.default_voice(), "voice_a");
    let ids: Vec<_> = tts.voices().iter().map(|v| v.id.as_str()).collect();
    assert_eq!(ids, ["voice_a", "voice_b"]);
}
