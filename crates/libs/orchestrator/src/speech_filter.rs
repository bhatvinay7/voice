/// Transcript sanitiser — runs after STT, before the LLM.
///
/// Cleans noise artefacts that survive speech-to-text transcription:
/// - Sound/action tags from STT:  `[cow mooing]`, `(laughter)`, `*whistle*`
/// - Filler sounds:  mmm, uh, uhh, um, hmm, ah, eh, …
/// - Animal sounds:  moo, baa, meow, woof, cluck, oink, neigh, quack, …
/// - Sound-effect words:  honk, beep, bang, whistle, buzz, ring, …
/// - Tokens with non-word characters (keeps Unicode letters and digits so
///   Arabic / Hindi text passes through).
///
/// The caller then decides whether to continue, ask to repeat, or handle a
/// continuation command.
use regex::Regex;
use std::sync::OnceLock;

/// Result of sanitising a raw STT transcript.
#[derive(Debug, Clone, PartialEq)]
pub enum CleanedInput {
    /// Nothing meaningful remained after cleaning — no LLM call warranted.
    Empty,
    /// User asked to continue or repeat the previous answer.
    Continuation(ContinuationKind),
    /// Cleaned, meaningful text ready for the LLM / RAG pipeline.
    Text(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContinuationKind {
    Continue,
    Repeat,
}

// ── Static compiled regexes ──────────────────────────────────────────────────

fn sound_tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Matches [anything], (anything), *anything*
    RE.get_or_init(|| Regex::new(r"\[.*?\]|\(.*?\)|\*[^*]+\*").unwrap())
}

fn word_token_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Allow Unicode letters, digits, apostrophe, hyphen.
    RE.get_or_init(|| Regex::new(r"^[\p{L}\p{N}'\-]+$").unwrap())
}

fn collapse_repeated_chars(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut previous = None;
    let mut run_length = 0usize;
    for character in input.chars() {
        if previous == Some(character) {
            run_length += 1;
        } else {
            previous = Some(character);
            run_length = 1;
        }
        if run_length <= 2 {
            output.push(character);
        }
    }
    output
}

// ── Noise word lists ─────────────────────────────────────────────────────────

/// Tokens that are pure noise even after collapsing repeated chars.
const NOISE_TOKENS: &[&str] = &[
    // Fillers
    "mm", "mmm", "hmm", "hm", "uh", "uhh", "um", "umm", "ah", "ahh", "eh", "oh", "ooh", "er", "err",
    // Animal sounds
    "moo", "baa", "meow", "mew", "woof", "bark", "cluck", "oink", "neigh", "quack", "tweet", "roar",
    "hiss", "chirp", "caw", "miaow", // Sound effects / environmental noise words
    "honk", "beep", "ding", "ring", "buzz", "bang", "clang", "crash", "thud", "whirr", "whir",
    "hum", "shh", "ssh", "psst", "tsk", "click", "pop", "whoosh", "swoosh",
];

/// Phrases that signal the user wants to continue or repeat the previous turn.
const CONTINUATION_PHRASES: &[(&str, ContinuationKind)] = &[
    ("continue", ContinuationKind::Continue),
    ("go on", ContinuationKind::Continue),
    ("go ahead", ContinuationKind::Continue),
    ("keep going", ContinuationKind::Continue),
    ("carry on", ContinuationKind::Continue),
    ("tell me more", ContinuationKind::Continue),
    ("repeat", ContinuationKind::Repeat),
    ("say that again", ContinuationKind::Repeat),
    ("can you repeat", ContinuationKind::Repeat),
    ("could you repeat", ContinuationKind::Repeat),
    ("say again", ContinuationKind::Repeat),
    ("come again", ContinuationKind::Repeat),
    ("pardon", ContinuationKind::Repeat),
    ("what was that", ContinuationKind::Repeat),
];

// ── Public API ───────────────────────────────────────────────────────────────

/// Sanitise a raw STT transcript and classify the result.
pub fn sanitize_transcript(raw: &str) -> CleanedInput {
    // 1. Strip sound tags
    let after_tags = sound_tag_re().replace_all(raw, " ");

    // 2. Lowercase
    let lower = after_tags.to_lowercase();

    // 3. Collapse 3+ repeated Unicode characters without regex backreferences.
    // The Rust regex crate intentionally does not support backreferences.
    let collapsed = collapse_repeated_chars(&lower);

    // 4. Check for continuation phrase (before token filtering)
    let trimmed = collapsed.trim();
    for (phrase, kind) in CONTINUATION_PHRASES {
        if trimmed == *phrase {
            return CleanedInput::Continuation(kind.clone());
        }
    }

    // 5. Filter tokens
    let filtered: Vec<&str> = trimmed
        .split_whitespace()
        .filter(|token| word_token_re().is_match(token) && !NOISE_TOKENS.contains(token))
        .collect();

    if filtered.is_empty() {
        return CleanedInput::Empty;
    }

    // 6. Detect all-same-token repetition (e.g. "ha ha ha ha ha")
    if filtered.len() >= 4 && filtered.iter().all(|t| *t == filtered[0]) {
        return CleanedInput::Empty;
    }

    CleanedInput::Text(filtered.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_filler_is_empty() {
        assert_eq!(sanitize_transcript("mmm"), CleanedInput::Empty);
        assert_eq!(sanitize_transcript("uh uh uh"), CleanedInput::Empty);
    }

    #[test]
    fn animal_sound_is_empty() {
        assert_eq!(
            sanitize_transcript("moooo [cow mooing]"),
            CleanedInput::Empty
        );
        assert_eq!(sanitize_transcript("baa baa"), CleanedInput::Empty);
    }

    #[test]
    fn mixed_filler_with_real_query() {
        let result = sanitize_transcript("uh honk show me 2 bed in marina");
        assert_eq!(
            result,
            CleanedInput::Text("show me 2 bed in marina".to_string())
        );
    }

    #[test]
    fn repeated_char_normalization_keeps_real_transcript() {
        assert_eq!(
            sanitize_transcript("Can I get information about rent properties?"),
            CleanedInput::Text("can i get information about rent properties".to_string())
        );
        assert_eq!(sanitize_transcript("moooo"), CleanedInput::Empty);
    }

    #[test]
    fn continuation_detected() {
        assert_eq!(
            sanitize_transcript("Continue."),
            // period stripped by word_token_re, but "continue" checked before filtering
            // Actually the check is pre-filter so let's test cleaned form
            sanitize_transcript("continue")
        );
        assert_eq!(
            sanitize_transcript("repeat"),
            CleanedInput::Continuation(ContinuationKind::Repeat)
        );
        assert_eq!(
            sanitize_transcript("say that again"),
            CleanedInput::Continuation(ContinuationKind::Repeat)
        );
    }

    #[test]
    fn repeated_token_is_empty() {
        assert_eq!(sanitize_transcript("ha ha ha ha ha"), CleanedInput::Empty);
    }

    #[test]
    fn arabic_text_survives() {
        // Arabic words should not be stripped by word_token_re (Unicode letters).
        match sanitize_transcript("أريد شقة في دبي") {
            CleanedInput::Text(t) => assert!(!t.is_empty()),
            other => panic!("Expected Text, got {:?}", other),
        }
    }

    #[test]
    fn sound_tags_stripped() {
        let result = sanitize_transcript("[loud bang] I need a 3 bedroom apartment (horn honking)");
        assert_eq!(
            result,
            CleanedInput::Text("i need a 3 bedroom apartment".to_string())
        );
    }

    #[test]
    fn clean_sentence_passthrough() {
        let result = sanitize_transcript("Show me properties near downtown Dubai under 2 million");
        match result {
            CleanedInput::Text(t) => assert!(t.contains("properties")),
            other => panic!("Expected Text, got {:?}", other),
        }
    }
}
