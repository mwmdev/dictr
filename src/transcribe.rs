use anyhow::{Context, Result};
use std::collections::HashMap;
use std::io::Cursor;

pub trait TranscribeBackend {
    fn transcribe(
        &mut self,
        audio_f32_16khz: &[f32],
        language: Option<&str>,
        initial_prompt: Option<&str>,
    ) -> Result<String>;
}

// --- Local whisper-rs backend ---

pub struct LocalWhisper {
    ctx: whisper_rs::WhisperContext,
}

impl LocalWhisper {
    pub fn new(model_path: &str) -> Result<Self> {
        let ctx = whisper_rs::WhisperContext::new_with_params(
            model_path,
            whisper_rs::WhisperContextParameters::default(),
        )
        .context("failed to load whisper model")?;
        Ok(Self { ctx })
    }
}

impl TranscribeBackend for LocalWhisper {
    fn transcribe(
        &mut self,
        audio: &[f32],
        language: Option<&str>,
        initial_prompt: Option<&str>,
    ) -> Result<String> {
        let mut state = self.ctx.create_state().context("failed to create state")?;
        let mut params =
            whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });
        if let Some(lang) = language {
            params.set_language(Some(lang));
        }
        if let Some(prompt) = initial_prompt {
            params.set_initial_prompt(prompt);
        }

        state
            .full(params, audio)
            .context("whisper inference failed")?;

        let n = state.full_n_segments().context("failed to get segments")?;
        let mut text = String::new();
        for i in 0..n {
            if let Ok(seg) = state.full_get_segment_text(i) {
                text.push_str(seg.trim());
                text.push(' ');
            }
        }
        Ok(text.trim().to_string())
    }
}

// --- OpenAI API backend ---

pub struct ApiWhisper {
    api_key: String,
    api_url: String,
    client: reqwest::Client,
    rt: tokio::runtime::Runtime,
}

impl ApiWhisper {
    pub fn new(api_key: String, api_url: String) -> Result<Self> {
        let rt = tokio::runtime::Runtime::new()?;
        let client = reqwest::Client::new();
        Ok(Self {
            api_key,
            api_url,
            client,
            rt,
        })
    }
}

impl TranscribeBackend for ApiWhisper {
    fn transcribe(
        &mut self,
        audio: &[f32],
        language: Option<&str>,
        initial_prompt: Option<&str>,
    ) -> Result<String> {
        let wav_bytes = encode_wav(audio)?;
        let api_key = self.api_key.clone();
        let api_url = self.api_url.clone();
        let client = self.client.clone();
        let language = language.map(String::from);
        let initial_prompt = initial_prompt.map(String::from);
        self.rt.block_on(async move {
            let part = reqwest::multipart::Part::bytes(wav_bytes)
                .file_name("audio.wav")
                .mime_str("audio/wav")?;
            let mut form = reqwest::multipart::Form::new()
                .text("model", "whisper-1")
                .part("file", part);
            if let Some(lang) = language {
                form = form.text("language", lang);
            }
            if let Some(prompt) = initial_prompt {
                form = form.text("prompt", prompt);
            }

            let resp = client
                .post(&api_url)
                .bearer_auth(&api_key)
                .multipart(form)
                .send()
                .await?
                .error_for_status()?;

            let json: HashMap<String, String> = resp.json().await?;
            let text = json.get("text").cloned().unwrap_or_default();
            Ok(text)
        })
    }
}

fn encode_wav(audio: &[f32]) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::new(&mut buf, spec)?;
    for &sample in audio {
        let s = (sample * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        writer.write_sample(s)?;
    }
    writer.finalize()?;
    Ok(buf.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_wav_produces_valid_wav() {
        let audio = vec![0.0f32; 16000]; // 1 second of silence
        let wav = encode_wav(&audio).unwrap();

        // WAV header: "RIFF"
        assert_eq!(&wav[..4], b"RIFF");
        // Format: "WAVE"
        assert_eq!(&wav[8..12], b"WAVE");

        // Verify hound can read it back
        let reader = hound::WavReader::new(Cursor::new(&wav)).unwrap();
        let spec = reader.spec();
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.sample_rate, 16000);
        assert_eq!(spec.bits_per_sample, 16);
        assert_eq!(spec.sample_format, hound::SampleFormat::Int);
    }

    #[test]
    fn encode_wav_sample_count_matches() {
        let audio = vec![0.5f32; 8000];
        let wav = encode_wav(&audio).unwrap();
        let reader = hound::WavReader::new(Cursor::new(&wav)).unwrap();
        assert_eq!(reader.len(), 8000);
    }

    #[test]
    fn encode_wav_clamps_extremes() {
        let audio = vec![1.5, -1.5, 1.0, -1.0, 0.0];
        let wav = encode_wav(&audio).unwrap();
        let mut reader = hound::WavReader::new(Cursor::new(&wav)).unwrap();
        let samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
        // Values beyond [-1, 1] should be clamped to i16 range
        assert_eq!(samples[0], i16::MAX);
        assert_eq!(samples[1], i16::MIN);
        assert_eq!(samples[4], 0);
    }

    #[test]
    fn encode_wav_empty_audio() {
        let wav = encode_wav(&[]).unwrap();
        let reader = hound::WavReader::new(Cursor::new(&wav)).unwrap();
        assert_eq!(reader.len(), 0);
    }

    #[test]
    fn api_whisper_new_stores_fields() {
        let api = ApiWhisper::new(
            "sk-test".into(),
            "https://example.com/v1/transcriptions".into(),
        )
        .unwrap();
        assert_eq!(api.api_key, "sk-test");
        assert_eq!(api.api_url, "https://example.com/v1/transcriptions");
    }

    #[test]
    fn api_whisper_connection_refused() {
        // Hitting a port with nothing listening should produce an error, not panic
        let mut api = ApiWhisper::new(
            "sk-test".into(),
            "http://127.0.0.1:1/v1/audio/transcriptions".into(),
        )
        .unwrap();
        let audio = vec![0.0f32; 16000];
        let result = api.transcribe(&audio, None, None);
        assert!(result.is_err());
    }

    #[test]
    fn api_whisper_transcribe_with_language_and_prompt() {
        // Verify the method doesn't panic when language and prompt are provided
        // (connection will fail, but form construction should succeed)
        let mut api = ApiWhisper::new("sk-test".into(), "http://127.0.0.1:1/nope".into()).unwrap();
        let audio = vec![0.0f32; 16000];
        let result = api.transcribe(&audio, Some("en"), Some("test prompt"));
        assert!(result.is_err()); // connection error, not a panic
    }
}
