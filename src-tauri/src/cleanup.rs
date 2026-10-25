use regex::{Regex, RegexBuilder};
use std::collections::HashMap;

pub struct CleanupEngine {
    fillers_regex: Regex,
    stutters_regex: Regex,
    self_corrections: Vec<(Regex, &'static str)>,
    spoken_punct: Vec<(Regex, &'static str)>,
    dictionary: HashMap<String, String>,
    snippets: HashMap<String, String>,
}

impl CleanupEngine {
    pub fn new() -> Self {
        // 1. Fillers: 'um', 'uh', 'er', 'ah', 'hmm', 'you know', 'i mean', 'sort of', 'kind of'
        // 'like' as filler: e.g. ", like," or "like, "
        let fillers_regex = RegexBuilder::new(r"\b(um|uh|er|ah|hmm|you know|i mean|sort of|kind of)\b,?\s*")
            .case_insensitive(true)
            .build()
            .unwrap();

        // 2. Stutters / repeated adjacent words: "the the" -> "the", "I I" -> "I"
        let stutters_regex = RegexBuilder::new(r"\b([a-zA-Z]+)\s+\1\b")
            .case_insensitive(true)
            .build()
            .unwrap();

        // 3. Spoken Self-corrections:
        // "meet at 3, no, 4 pm" -> "meet at 4 pm"
        // "invoice to Sarah, actually send it to John" -> "invoice to John"
        // "budget is 10k, wait, 12k" -> "budget is 12k"
        let self_corrections = vec![
            (Regex::new(r"(?i)\b.+?,\s*(?:no|wait|scratch that),?\s*(.+)").unwrap(), "$1"),
            (Regex::new(r"(?i)\b.+?,\s*actually,?\s*(.+)").unwrap(), "$1"),
        ];

        // 4. Spoken punctuation commands:
        let spoken_punct = vec![
            (Regex::new(r"(?i)\s*\b(new paragraph)\b\s*").unwrap(), "\n\n"),
            (Regex::new(r"(?i)\s*\b(new line)\b\s*").unwrap(), "\n"),
            (Regex::new(r"(?i)\s*\b(bullet point|bullet list)\b\s*").unwrap(), "\n- "),
            (Regex::new(r"(?i)\s*\b(period|full stop)\b").unwrap(), "."),
            (Regex::new(r"(?i)\s*\b(comma)\b").unwrap(), ","),
            (Regex::new(r"(?i)\s*\b(question mark)\b").unwrap(), "?"),
            (Regex::new(r"(?i)\s*\b(exclamation point|exclamation mark)\b").unwrap(), "!"),
            (Regex::new(r"(?i)\s*\b(colon)\b").unwrap(), ":"),
            (Regex::new(r"(?i)\s*\b(semicolon)\b").unwrap(), ";"),
        ];

        Self {
            fillers_regex,
            stutters_regex,
            self_corrections,
            spoken_punct,
            dictionary: HashMap::new(),
            snippets: HashMap::new(),
        }
    }

    pub fn set_dictionary(&mut self, dict: HashMap<String, String>) {
        self.dictionary = dict;
    }

    pub fn set_snippets(&mut self, snippets: HashMap<String, String>) {
        self.snippets = snippets;
    }

    pub fn clean(&self, input: &str) -> String {
        let mut text = input.trim().to_string();
        if text.is_empty() {
            return text;
        }

        // Check if input exactly matches or contains a snippet trigger
        let text_lower = text.to_lowercase();
        for (trigger, expansion) in &self.snippets {
            let trig_lower = trigger.to_lowercase();
            if text_lower == trig_lower || text_lower == format!("{}.", trig_lower) {
                return expansion.clone();
            }
        }

        // Apply Spoken Self-Corrections
        for (re, replacement) in &self.self_corrections {
            if re.is_match(&text) {
                text = re.replace_all(&text, *replacement).to_string();
            }
        }

        // Apply Spoken Punctuation & Formatting commands
        for (re, replacement) in &self.spoken_punct {
            text = re.replace_all(&text, *replacement).to_string();
        }

        // Remove Fillers
        text = self.fillers_regex.replace_all(&text, "").to_string();

        // Collapse Stutters (loop twice to catch triplicates e.g. "the the the")
        text = self.stutters_regex.replace_all(&text, "$1").to_string();
        text = self.stutters_regex.replace_all(&text, "$1").to_string();

        // Clean up punctuation spacing: e.g. " ," -> "," or " ." -> "."
        let fix_space_punct = Regex::new(r"\s+([,\.!\?:;])").unwrap();
        text = fix_space_punct.replace_all(&text, "$1").to_string();

        // Clean up multiple spaces
        let multi_space = Regex::new(r"[ \t]{2,}").unwrap();
        text = multi_space.replace_all(&text, " ").to_string();

        // Apply Personal Dictionary replacements
        for (key, replacement) in &self.dictionary {
            let pattern = format!(r"(?i)\b{}\b", regex::escape(key));
            if let Ok(re) = Regex::new(&pattern) {
                text = re.replace_all(&text, replacement.as_str()).to_string();
            }
        }

        // Sentence Capitalization: Capitalize start of text and letters following . ! ? \n
        text = Self::capitalize_sentences(&text);

        text.trim().to_string()
    }

    fn capitalize_sentences(input: &str) -> String {
        let mut chars: Vec<char> = input.chars().collect();
        let mut capitalize_next = true;

        for i in 0..chars.len() {
            let c = chars[i];
            if capitalize_next && c.is_alphabetic() {
                chars[i] = c.to_uppercase().next().unwrap_or(c);
                capitalize_next = false;
            } else if c == '.' || c == '!' || c == '?' || c == '\n' {
                capitalize_next = true;
            }
        }

        chars.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filler_removal() {
        let engine = CleanupEngine::new();
        let res = engine.clean("Um, I think, uh, we should go.");
        assert_eq!(res, "I think we should go.");
    }

    #[test]
    fn test_stutter_removal() {
        let engine = CleanupEngine::new();
        let res = engine.clean("We will will see the the results.");
        assert_eq!(res, "We will see the results.");
    }

    #[test]
    fn test_spoken_punctuation() {
        let engine = CleanupEngine::new();
        let res = engine.clean("Hello world period how are you question mark");
        assert_eq!(res, "Hello world. How are you?");
    }

    #[test]
    fn test_self_correction() {
        let engine = CleanupEngine::new();
        let res = engine.clean("Let's meet at 3, no, 4 pm");
        assert_eq!(res, "4 pm");
    }

    #[test]
    fn test_dictionary_and_snippets() {
        let mut engine = CleanupEngine::new();
        let mut dict = HashMap::new();
        dict.insert("flow local".to_string(), "FlowLocal".to_string());
        engine.set_dictionary(dict);

        let mut snippets = HashMap::new();
        snippets.insert("insert cal link".to_string(), "https://cal.com/me".to_string());
        engine.set_snippets(snippets);

        let res1 = engine.clean("Welcome to flow local period");
        assert_eq!(res1, "Welcome to FlowLocal.");

        let res2 = engine.clean("insert cal link");
        assert_eq!(res2, "https://cal.com/me");
    }
}
