// Whisper large-v3-turbo through whisper.cpp, on the GPU via Metal. Unlike
// Parakeet and Bark it covers about 100 languages, so it's the model for
// anything they don't speak (Japanese, for one).
//
// The weights are a GGML file the registry downloads (a quantized and a
// half-precision variant of the same model). Whisper decodes a whole
// utterance at once, so the endpointer's utterances (at most 15 s) go in
// as they are and sentences come back with their own timestamps.

use super::{TranscriptionSegment, SAMPLE_RATE, WHISPER_VARIANT_PREFIX};
use crate::model_registry::ModelVariant;
use anyhow::anyhow;
use std::borrow::Cow;
use std::path::Path;

const SR: usize = SAMPLE_RATE as usize;
/// whisper.cpp refuses input under a second. The margin is for the rounding
/// of its own length check.
const MIN_SAMPLES: usize = SR + SR / 10;
/// Whisper reports time in 10 ms steps.
const STEP_MS: u64 = 10;
/// A segment it thinks isn't speech, and isn't sure of what it heard.
const NO_SPEECH_ABOVE: f32 = 0.6;
const CONFIDENCE_BELOW: f32 = 0.4;
/// Runs of one character or of a longer unit that mean the decoder is stuck.
const LOOP_UNIT_MAX: usize = 16;
const LOOP_REPEATS: usize = 6;
const LOOP_REPEATS_SINGLE_CHAR: usize = 12;

/// Whether this Mac can run Whisper: whisper.cpp is only built for macOS.
pub fn is_supported() -> bool {
    cfg!(target_os = "macos")
}

pub(super) fn is_variant(variant: &ModelVariant) -> bool {
    variant.id.starts_with(WHISPER_VARIANT_PREFIX)
}

/// The languages Whisper was trained on, by ISO 639-1 code (a few have only
/// a three-letter one). Fixing one makes it skip guessing the language, which
/// is where short utterances go wrong.
pub const LANGUAGES: &[&str] = &[
    "en", "zh", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl", "ca", "nl", "ar", "sv", "it",
    "id", "hi", "fi", "vi", "he", "uk", "el", "ms", "cs", "ro", "da", "hu", "ta", "no", "th", "ur",
    "hr", "bg", "lt", "la", "mi", "ml", "cy", "sk", "te", "fa", "lv", "bn", "sr", "az", "sl", "kn",
    "et", "mk", "br", "eu", "is", "hy", "ne", "mn", "bs", "kk", "sq", "sw", "gl", "mr", "pa", "si",
    "km", "sn", "yo", "so", "af", "oc", "ka", "be", "tg", "sd", "gu", "am", "yi", "lo", "uz", "fo",
    "ht", "ps", "tk", "nn", "mt", "sa", "lb", "my", "bo", "tl", "mg", "as", "tt", "haw", "ln",
    "ha", "ba", "jw", "su", "yue",
];

/// `code` as one of `LANGUAGES`, which is what lets the engine hold it for
/// as long as it lives.
pub fn language_code(code: &str) -> Option<&'static str> {
    LANGUAGES.iter().copied().find(|&known| known == code)
}

/// One segment as whisper.cpp reports it.
struct Decoded {
    text: String,
    /// In `STEP_MS` units from the start of the audio.
    start: i64,
    end: i64,
    no_speech: f32,
    /// The probability it gave each token of the text, 0–1.
    token_probs: Vec<f32>,
}

pub(super) struct Whisper {
    engine: engine::Engine,
}

impl Whisper {
    /// `variant`'s first file is its weights; the speech detector follows.
    /// `language` is a code from `LANGUAGES`; `None` has it work the language
    /// out for each utterance.
    pub(super) fn load(
        dir: &Path,
        variant: &ModelVariant,
        language: Option<&'static str>,
    ) -> anyhow::Result<Self> {
        let weights = variant
            .files
            .first()
            .ok_or_else(|| anyhow!("Whisper variant {} lists no weights", variant.id))?;
        Ok(Self {
            engine: engine::Engine::load(&dir.join(weights.local_name), language)?,
        })
    }

    /// Transcribe a chunk of audio (16kHz mono f32 samples).
    pub(super) fn transcribe(
        &mut self,
        audio: &[f32],
        offset_ms: u64,
    ) -> anyhow::Result<Vec<TranscriptionSegment>> {
        let audio_ms = (audio.len() * 1000 / SR) as u64;
        let decoded = self.engine.run(&padded(audio))?;
        Ok(decoded
            .into_iter()
            .map(|d| (mean(&d.token_probs), d))
            .filter(|(confidence, d)| !is_spurious(&d.text, d.no_speech, *confidence))
            // Whatever it hears in the silence added to reach its minimum.
            .filter(|(_, d)| step_ms(d.start) < audio_ms)
            .map(|(confidence, d)| {
                let start = step_ms(d.start);
                TranscriptionSegment {
                    text: d.text,
                    start_ms: offset_ms + start,
                    end_ms: offset_ms + step_ms(d.end).clamp(start, audio_ms),
                    confidence,
                }
            })
            .collect())
    }
}

/// `audio`, with silence after it up to the shortest input whisper.cpp takes.
fn padded(audio: &[f32]) -> Cow<'_, [f32]> {
    if audio.len() >= MIN_SAMPLES {
        return Cow::Borrowed(audio);
    }
    let mut audio = audio.to_vec();
    audio.resize(MIN_SAMPLES, 0.0);
    Cow::Owned(audio)
}

fn step_ms(steps: i64) -> u64 {
    steps.max(0) as u64 * STEP_MS
}

fn mean(probs: &[f32]) -> f32 {
    if probs.is_empty() {
        0.0
    } else {
        probs.iter().sum::<f32>() / probs.len() as f32
    }
}

/// Whether a segment is something whisper makes up from silence or noise
/// rather than something said.
fn is_spurious(text: &str, no_speech: f32, confidence: f32) -> bool {
    let text = text.trim();
    // Not language-specific on purpose: the stock phrases it invents differ
    // per language, but how they look (tags, punctuation, loops) doesn't.
    text.is_empty()
        || !text.chars().any(char::is_alphanumeric)
        || is_annotation(text)
        || (no_speech > NO_SPEECH_ABOVE && confidence < CONFIDENCE_BELOW)
        || is_stuck_in_a_loop(text)
}

/// "[BLANK_AUDIO]", "(music)", "♪ ... ♪": tags for what isn't speech.
fn is_annotation(text: &str) -> bool {
    const PAIRS: [(char, char); 6] = [
        ('[', ']'),
        ('(', ')'),
        ('（', '）'),
        ('【', '】'),
        ('*', '*'),
        ('♪', '♪'),
    ];
    let (Some(first), Some(last)) = (text.chars().next(), text.chars().next_back()) else {
        return false;
    };
    PAIRS.contains(&(first, last))
}

/// Whether some short run of characters repeats over and over, as the
/// decoder does when it loses track of the audio.
fn is_stuck_in_a_loop(text: &str) -> bool {
    let chars: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    (1..=LOOP_UNIT_MAX).any(|unit| {
        let repeats = if unit == 1 {
            LOOP_REPEATS_SINGLE_CHAR
        } else {
            LOOP_REPEATS
        };
        // A stretch of `run + unit` characters repeating every `unit` of them.
        let mut run = 0;
        (unit..chars.len()).any(|i| {
            run = if chars[i] == chars[i - unit] {
                run + 1
            } else {
                0
            };
            run + unit >= repeats * unit
        })
    })
}

#[cfg(target_os = "macos")]
mod engine {
    use super::Decoded;
    use anyhow::anyhow;
    use std::path::Path;
    use whisper_rs::{
        FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
        WhisperTokenId,
    };

    /// Few, so that the threads left are enough for capture and the UI; the
    /// work is on the GPU.
    const THREADS: i32 = 4;

    pub(super) struct Engine {
        state: WhisperState,
        params: FullParams<'static, 'static>,
        /// Token ids from this up are markers (timestamps, language), not text.
        end_of_text: WhisperTokenId,
    }

    impl Engine {
        pub(super) fn load(weights: &Path, language: Option<&'static str>) -> anyhow::Result<Self> {
            // Without this, whisper.cpp writes its log to stderr.
            whisper_rs::install_logging_hooks();
            let context =
                WhisperContext::new_with_params(weights, WhisperContextParameters::default())
                    .map_err(|e| anyhow!("Failed to load the Whisper model: {e}"))?;
            let state = context
                .create_state()
                .map_err(|e| anyhow!("Failed to start Whisper: {e}"))?;

            let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
            params.set_n_threads(THREADS);
            // Each utterance stands alone, and carrying text over would let a
            // hallucination in one spread to the next.
            params.set_no_context(true);
            params.set_translate(false);
            // Left unset it detects the language. `set_detect_language` is not
            // that: it stops after detecting and returns no text.
            params.set_language(language);
            params.set_suppress_blank(true);
            params.set_suppress_nst(true);
            // Retrying a doubtful utterance at higher temperatures can multiply
            // its latency; the caller drops what looks made up instead.
            params.set_temperature_inc(0.0);
            params.set_print_special(false);
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_timestamps(false);

            tracing::info!("Transcription engine loaded: Whisper on Metal");
            Ok(Self {
                end_of_text: context.token_eot(),
                state,
                params,
            })
        }

        pub(super) fn run(&mut self, audio: &[f32]) -> anyhow::Result<Vec<Decoded>> {
            self.state
                .full(self.params.clone(), audio)
                .map_err(|e| anyhow!("Whisper failed: {e}"))?;
            let end_of_text = self.end_of_text;
            let mut decoded = Vec::new();
            for segment in self.state.as_iter() {
                let text = segment
                    .to_str_lossy()
                    .map_err(|e| anyhow!("Whisper returned unreadable text: {e}"))?
                    .trim()
                    .to_string();
                decoded.push(Decoded {
                    text,
                    start: segment.start_timestamp(),
                    end: segment.end_timestamp(),
                    no_speech: segment.no_speech_probability(),
                    token_probs: (0..segment.n_tokens())
                        .filter_map(|i| segment.get_token(i))
                        .filter(|t| t.token_id() < end_of_text)
                        .map(|t| t.token_probability())
                        .collect(),
                });
            }
            Ok(decoded)
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod engine {
    use super::Decoded;
    use anyhow::anyhow;
    use std::path::Path;

    /// Never built: the registry doesn't offer Whisper off macOS.
    pub(super) enum Engine {}

    impl Engine {
        pub(super) fn load(
            _weights: &Path,
            _language: Option<&'static str>,
        ) -> anyhow::Result<Self> {
            Err(anyhow!("Whisper transcription needs macOS"))
        }

        pub(super) fn run(&mut self, _audio: &[f32]) -> anyhow::Result<Vec<Decoded>> {
            match *self {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_audio_is_padded_with_silence_and_long_audio_is_left_alone() {
        let short = padded(&[0.5; 100]);
        assert_eq!(short.len(), MIN_SAMPLES);
        assert!(short[..100].iter().all(|&s| s == 0.5));
        assert!(short[100..].iter().all(|&s| s == 0.0));

        let long = vec![0.1; SR * 5];
        assert!(matches!(padded(&long), Cow::Borrowed(_)));
    }

    #[test]
    fn steps_become_milliseconds_and_never_go_negative() {
        assert_eq!(step_ms(150), 1500);
        assert_eq!(step_ms(-3), 0);
    }

    #[test]
    fn confidence_is_the_mean_and_zero_without_tokens() {
        assert_eq!(mean(&[]), 0.0);
        assert!((mean(&[0.5, 1.0]) - 0.75).abs() < 1e-6);
    }

    #[test]
    fn empty_symbol_only_and_tagged_text_is_dropped() {
        for text in [
            "",
            "  ",
            "...",
            "♪♪",
            "。",
            "[BLANK_AUDIO]",
            "(音楽)",
            "♪ la la ♪",
        ] {
            assert!(is_spurious(text, 0.0, 1.0), "{text:?} should be dropped");
        }
    }

    #[test]
    fn real_speech_in_any_language_is_kept() {
        for text in [
            "Hello, how are you?",
            "今日は会議を始めましょう。",
            "Let's use the 新しい API.",
            "Voilà, ça marche (enfin).",
        ] {
            assert!(!is_spurious(text, 0.1, 0.9), "{text:?} should be kept");
        }
    }

    #[test]
    fn a_segment_whisper_doubts_is_dropped_only_when_it_is_unsure_too() {
        assert!(is_spurious("Thank you.", 0.9, 0.2));
        assert!(!is_spurious("Thank you.", 0.9, 0.9));
        assert!(!is_spurious("Thank you.", 0.1, 0.2));
    }

    #[test]
    fn a_decoder_stuck_in_a_loop_is_dropped() {
        assert!(is_stuck_in_a_loop(&"ありがとうございます".repeat(6)));
        assert!(is_stuck_in_a_loop(&"thank you ".repeat(6)));
        assert!(is_stuck_in_a_loop(&"あ".repeat(12)));
        assert!(!is_stuck_in_a_loop("はいはい、そうですね。"));
        assert!(!is_stuck_in_a_loop("It was a very, very good day."));
        assert!(!is_stuck_in_a_loop("うーーーん"));
    }

    #[test]
    fn only_languages_whisper_knows_are_accepted() {
        assert_eq!(language_code("ja"), Some("ja"));
        assert_eq!(language_code("haw"), Some("haw"));
        assert_eq!(language_code("auto"), None);
        assert_eq!(language_code(""), None);
        assert_eq!(language_code("xx"), None);
    }

    #[test]
    fn language_codes_are_listed_once() {
        let unique: std::collections::HashSet<_> = LANGUAGES.iter().collect();
        assert_eq!(unique.len(), LANGUAGES.len());
        assert_eq!(LANGUAGES.len(), 100);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn whisper_cpp_knows_every_listed_language() {
        for code in LANGUAGES {
            assert!(
                whisper_rs::get_lang_id(code).is_some(),
                "whisper.cpp doesn't know {code}"
            );
        }
    }

    #[test]
    fn every_whisper_variant_is_found_by_its_prefix() {
        let model = crate::model_registry::get_model(super::super::MODEL_ID).unwrap();
        let whisper: Vec<_> = model.variants.iter().filter(|v| is_variant(v)).collect();
        assert_eq!(whisper.len(), 2);
        for variant in whisper {
            let weights = variant.files.first().unwrap().local_name;
            assert!(
                weights.ends_with(".bin"),
                "{} should lead with its weights",
                variant.id
            );
        }
    }
}
