//! Keyword matching algorithm for skill recommendations

use super::types::SkillMatch;
use std::collections::HashMap;

/// Punctuation stripped from the start of a word; interior punctuation and a
/// leading `.` stay, so `c++`, `.net` and `node.js` still match themselves.
const LEADING_EDGE: &[char] = &[
    ',', ';', ':', '!', '?', '(', ')', '[', ']', '{', '}', '"', '\'', '`',
];

/// Punctuation stripped from the end of a word: the leading set and a full stop.
const TRAILING_EDGE: &[char] = &[
    ',', ';', ':', '!', '?', '(', ')', '[', ']', '{', '}', '"', '\'', '`', '.',
];

/// Normalize text for matching: lowercase, replace separators with spaces and
/// strip sentence punctuation from the edge of each word.
pub fn normalize_text(text: &str) -> String {
    text.to_lowercase()
        .replace(['_', '-'], " ")
        .split_whitespace()
        .map(|word| {
            word.trim_start_matches(LEADING_EDGE)
                .trim_end_matches(TRAILING_EDGE)
        })
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Split text into individual words for matching
pub fn split_into_words(text: &str) -> Vec<String> {
    normalize_text(text)
        .split_whitespace()
        .map(|s| s.to_string())
        .collect()
}

/// Match triggers, scoring phrase hits at 2 and word hits at 1. Return scores
/// at or above `score_threshold`, ordered by descending score then name, up to `max_results`.
pub fn match_skills(
    text: &str,
    trigger_map: &HashMap<String, Vec<String>>,
    skill_descriptions: &HashMap<String, String>,
    max_results: usize,
    score_threshold: f32,
) -> Vec<SkillMatch> {
    let words = split_into_words(text);

    let mut skill_scores: HashMap<String, f32> = HashMap::new();
    let mut skill_matched_triggers: HashMap<String, Vec<String>> = HashMap::new();

    for (trigger, skill_names) in trigger_map {
        let trigger_normalized = normalize_text(trigger);
        let trigger_words: Vec<&str> = trigger_normalized.split_whitespace().collect();
        let Some(score_increment) = trigger_score(&words, &trigger_words) else {
            continue;
        };

        for skill_name in skill_names {
            *skill_scores.entry(skill_name.clone()).or_insert(0.0) += score_increment;
            skill_matched_triggers
                .entry(skill_name.clone())
                .or_default()
                .push(trigger.clone());
        }
    }

    let mut matches: Vec<SkillMatch> = skill_scores
        .into_iter()
        .filter(|(_, score)| *score >= score_threshold)
        .map(|(name, score)| {
            let description = skill_descriptions.get(&name).cloned().unwrap_or_default();
            let matched_triggers = skill_matched_triggers.remove(&name).unwrap_or_default();
            SkillMatch::new(name, description, score, matched_triggers)
        })
        .collect();

    matches.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
    });

    matches.truncate(max_results);
    matches
}

/// Score of one trigger against the prompt's words: 2 for a whole-word phrase,
/// 1 for a single word, `None` for no match.
fn trigger_score(words: &[String], trigger_words: &[&str]) -> Option<f32> {
    match trigger_words {
        [] => None,
        [word] => words.iter().any(|w| w == word).then_some(1.0),
        phrase => words
            .windows(phrase.len())
            .any(|window| window.iter().map(String::as_str).eq(phrase.iter().copied()))
            .then_some(2.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_text() {
        assert_eq!(normalize_text("Hello_World"), "hello world");
        assert_eq!(normalize_text("foo-bar-baz"), "foo bar baz");
        assert_eq!(normalize_text("  multiple   spaces  "), "multiple spaces");
        assert_eq!(normalize_text("MixedCase"), "mixedcase");
    }

    #[test]
    fn test_split_into_words() {
        let words = split_into_words("implement login_flow with OAuth");
        assert_eq!(words, vec!["implement", "login", "flow", "with", "oauth"]);
    }

    #[test]
    fn test_match_skills_single_word() {
        let mut trigger_map = HashMap::new();
        trigger_map.insert("login".to_string(), vec!["auth".to_string()]);
        trigger_map.insert("test".to_string(), vec!["testing".to_string()]);

        let mut descriptions = HashMap::new();
        descriptions.insert("auth".to_string(), "Authentication patterns".to_string());
        descriptions.insert("testing".to_string(), "Testing patterns".to_string());

        let matches = match_skills(
            "implement login functionality",
            &trigger_map,
            &descriptions,
            5,
            1.0,
        );

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "auth");
        assert_eq!(matches[0].score, 1.0);
    }

    #[test]
    fn test_match_skills_phrase() {
        let mut trigger_map = HashMap::new();
        trigger_map.insert("refresh token".to_string(), vec!["auth".to_string()]);

        let mut descriptions = HashMap::new();
        descriptions.insert("auth".to_string(), "Authentication patterns".to_string());

        let matches = match_skills(
            "implement refresh token rotation",
            &trigger_map,
            &descriptions,
            5,
            1.0,
        );

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "auth");
        assert_eq!(matches[0].score, 2.0); // phrase match = 2 points
    }

    #[test]
    fn test_match_skills_multiple_hits() {
        let mut trigger_map = HashMap::new();
        trigger_map.insert("login".to_string(), vec!["auth".to_string()]);
        trigger_map.insert("password".to_string(), vec!["auth".to_string()]);
        trigger_map.insert("token".to_string(), vec!["auth".to_string()]);

        let mut descriptions = HashMap::new();
        descriptions.insert("auth".to_string(), "Authentication patterns".to_string());

        let matches = match_skills(
            "implement login with password and token",
            &trigger_map,
            &descriptions,
            5,
            1.0,
        );

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "auth");
        assert_eq!(matches[0].score, 3.0); // 3 word matches = 3 points
    }

    #[test]
    fn test_match_skills_threshold() {
        let mut trigger_map = HashMap::new();
        trigger_map.insert("login".to_string(), vec!["auth".to_string()]);

        let mut descriptions = HashMap::new();
        descriptions.insert("auth".to_string(), "Authentication patterns".to_string());

        // With threshold 2.0, a single word match (score 1.0) should not pass
        let matches = match_skills(
            "implement login",
            &trigger_map,
            &descriptions,
            5,
            2.0, // threshold = 2.0
        );

        assert!(matches.is_empty());
    }

    #[test]
    fn test_match_skills_max_results() {
        let mut trigger_map = HashMap::new();
        trigger_map.insert("a".to_string(), vec!["skill1".to_string()]);
        trigger_map.insert("b".to_string(), vec!["skill2".to_string()]);
        trigger_map.insert("c".to_string(), vec!["skill3".to_string()]);

        let mut descriptions = HashMap::new();
        descriptions.insert("skill1".to_string(), "Skill 1".to_string());
        descriptions.insert("skill2".to_string(), "Skill 2".to_string());
        descriptions.insert("skill3".to_string(), "Skill 3".to_string());

        let matches = match_skills("a b c", &trigger_map, &descriptions, 2, 1.0);

        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_match_skills_sorting() {
        let mut trigger_map = HashMap::new();
        trigger_map.insert("a".to_string(), vec!["skill_low".to_string()]);
        trigger_map.insert("b".to_string(), vec!["skill_high".to_string()]);
        trigger_map.insert("c".to_string(), vec!["skill_high".to_string()]);

        let mut descriptions = HashMap::new();
        descriptions.insert("skill_low".to_string(), "Low score skill".to_string());
        descriptions.insert("skill_high".to_string(), "High score skill".to_string());

        let matches = match_skills("a b c", &trigger_map, &descriptions, 5, 1.0);

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].name, "skill_high"); // Higher score first
        assert_eq!(matches[0].score, 2.0);
        assert_eq!(matches[1].name, "skill_low");
        assert_eq!(matches[1].score, 1.0);
    }

    fn matched(text: &str, trigger: &str) -> bool {
        let mut trigger_map = HashMap::new();
        trigger_map.insert(trigger.to_string(), vec!["skill".to_string()]);
        !match_skills(text, &trigger_map, &HashMap::new(), 5, 1.0).is_empty()
    }

    #[test]
    fn a_word_followed_by_punctuation_matches_its_trigger() {
        assert!(matched("fix auth, now", "auth"));
        assert!(matched("(refresh token)", "refresh token"));
        assert!(matched("port the c++ and node.js, code", "c++"));
        assert!(matched("port the c++ and node.js, code", "node.js"));
    }

    #[test]
    fn a_phrase_matches_whole_words_only() {
        assert!(!matched("django model", "go mod"));
        assert!(!matched("refresh tokens", "refresh token"));
        assert!(matched("a refresh token here", "refresh token"));
    }

    #[test]
    fn normalizing_is_idempotent_and_keeps_interior_punctuation() {
        for text in [
            "c++",
            ".net",
            "std::span",
            "ci/cd",
            "openapi 3.1",
            "asp.net core",
        ] {
            assert_eq!(normalize_text(text), text);
            assert_eq!(normalize_text(&normalize_text(text)), text);
        }
        assert_eq!(normalize_text("\"quoted\"; (x)."), "quoted x");
    }
}
