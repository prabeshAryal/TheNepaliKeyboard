mod artifact;
mod transliteration;

use std::cmp::Reverse;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

pub use artifact::{
    ARTIFACT_VERSION, ArtifactStats, KeyIndexRecord, LexiconArtifact, LexiconEntry, LexiconSource,
};
pub use transliteration::{
    edit_distance_units, latin_input_key, normalize_nepali_word, romanize_nepali_word,
    transliterate_latin_fallback, transliteration_key_for_word,
};

use anyhow::{Context, Result};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct Candidate {
    pub word: String,
    pub romanized: String,
    pub score: i32,
    pub gloss: Option<String>,
    pub source_mask: u8,
}

#[derive(Debug, Clone)]
pub struct Preedit {
    pub latin_buffer: String,
    pub normalized_key: String,
    pub auto_selected: Option<Candidate>,
    pub selected_index: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
pub struct SessionConfig {
    pub shortlist_size: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self { shortlist_size: 5 }
    }
}

#[derive(Debug, Clone)]
pub struct CommitOutcome {
    pub committed: Option<String>,
    pub cleared_buffer: String,
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("candidate index {0} is out of bounds")]
    InvalidCandidateIndex(usize),
}

#[derive(Debug, Clone)]
pub struct Lexicon {
    artifact: LexiconArtifact,
}

impl Lexicon {
    pub fn from_artifact(artifact: LexiconArtifact) -> Self {
        Self {
            artifact: normalize_artifact(artifact),
        }
    }

    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self> {
        let bytes = fs::read(path.as_ref()).with_context(|| {
            format!(
                "failed to read lexicon artifact from {}",
                path.as_ref().display()
            )
        })?;
        Self::from_bytes(&bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let artifact: LexiconArtifact =
            bincode::deserialize(bytes).context("failed to deserialize lexicon artifact")?;

        if artifact.version != ARTIFACT_VERSION {
            anyhow::bail!(
                "unsupported lexicon version {}, expected {}",
                artifact.version,
                ARTIFACT_VERSION
            );
        }

        Ok(Self {
            artifact: normalize_artifact(artifact),
        })
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        bincode::serialize(&self.artifact).context("failed to serialize lexicon artifact")
    }

    pub fn entries_len(&self) -> usize {
        self.artifact.entries.len()
    }

    pub fn stats(&self) -> &ArtifactStats {
        &self.artifact.stats
    }

    pub fn find_candidates(&self, input: &str, limit: usize) -> Vec<Candidate> {
        let key = latin_input_key(input);
        if key.is_empty() {
            return Vec::new();
        }

        let start = self
            .artifact
            .key_index
            .partition_point(|record| record.key.as_str() < key.as_str());

        let mut seen = HashSet::new();
        let mut scored = Vec::new();

        for record in self.artifact.key_index.iter().skip(start) {
            if !record.key.starts_with(&key) {
                break;
            }

            for &entry_index in &record.entry_indices {
                let idx = entry_index as usize;
                if !seen.insert(idx) {
                    continue;
                }

                let entry = &self.artifact.entries[idx];
                scored.push((self.score_entry(&key, entry), idx));
            }
        }

        scored.sort_by_key(|(score, idx)| (Reverse(*score), *idx));

        let mut candidates: Vec<Candidate> = scored
            .into_iter()
            .take(limit)
            .map(|(score, idx)| {
                let entry = &self.artifact.entries[idx];
                Candidate {
                    word: entry.word.clone(),
                    romanized: entry.romanized.clone(),
                    score,
                    gloss: entry.gloss.clone(),
                    source_mask: entry.source_mask,
                }
            })
            .collect();

        self.append_fallback_candidate(input, &mut candidates, limit);
        candidates
    }

    fn score_entry(&self, query_key: &str, entry: &LexiconEntry) -> i32 {
        let exact_bonus = if entry.normalized_key == query_key {
            300
        } else {
            0
        };
        let prefix_bonus = if entry.normalized_key.starts_with(query_key) {
            120 - ((entry.normalized_key.len() as i32 - query_key.len() as i32).max(0) * 4)
        } else {
            0
        };
        let edit_penalty = (edit_distance_units(query_key, &entry.normalized_key) as i32) * 25;
        let length_penalty = ((entry.romanized.len() as i32 - query_key.len() as i32).abs()) * 2;
        let source_bonus = i32::from(entry.source_weight) * 20;

        exact_bonus + prefix_bonus + source_bonus - edit_penalty - length_penalty
    }

    fn append_fallback_candidate(
        &self,
        input: &str,
        candidates: &mut Vec<Candidate>,
        limit: usize,
    ) {
        let fallback = transliterate_latin_fallback(input);
        if fallback.is_empty()
            || candidates
                .iter()
                .any(|candidate| candidate.word == fallback)
        {
            return;
        }

        let base_score = candidates
            .last()
            .map_or(120, |candidate| candidate.score.saturating_sub(40));
        let fallback_candidate = Candidate {
            word: fallback,
            romanized: input.to_lowercase(),
            score: base_score,
            gloss: Some("phonetic fallback".into()),
            source_mask: 0,
        };

        if candidates.len() < limit {
            candidates.push(fallback_candidate);
        } else if let Some(last) = candidates.last_mut() {
            *last = fallback_candidate;
        }
    }
}

fn normalize_artifact(mut artifact: LexiconArtifact) -> LexiconArtifact {
    artifact
        .key_index
        .sort_by(|left, right| left.key.cmp(&right.key));
    for record in &mut artifact.key_index {
        record.entry_indices.sort_unstable();
        record.entry_indices.dedup();
    }
    artifact
}

#[derive(Debug, Clone)]
pub struct Session {
    lexicon: Lexicon,
    config: SessionConfig,
    buffer: String,
    cached_candidates: Vec<Candidate>,
    selected_index: usize,
}

impl Session {
    pub fn new(lexicon: Lexicon) -> Self {
        Self::with_config(lexicon, SessionConfig::default())
    }

    pub fn with_config(lexicon: Lexicon, config: SessionConfig) -> Self {
        Self {
            lexicon,
            config,
            buffer: String::new(),
            cached_candidates: Vec::new(),
            selected_index: 0,
        }
    }

    pub fn apply_input(&mut self, input: &str) {
        self.buffer.clear();
        self.buffer.push_str(input);
        self.refresh_candidates();
    }

    pub fn append_keystroke(&mut self, ch: char) {
        self.buffer.push(ch);
        self.refresh_candidates();
    }

    pub fn backspace(&mut self) {
        self.buffer.pop();
        self.refresh_candidates();
    }

    pub fn reset_session(&mut self) {
        self.buffer.clear();
        self.cached_candidates.clear();
        self.selected_index = 0;
    }

    pub fn get_preedit(&self) -> Preedit {
        Preedit {
            latin_buffer: self.buffer.clone(),
            normalized_key: latin_input_key(&self.buffer),
            auto_selected: self.selected_candidate().cloned(),
            selected_index: (!self.cached_candidates.is_empty()).then_some(self.selected_index),
        }
    }

    pub fn get_candidates(&mut self, limit: usize) -> Vec<Candidate> {
        self.cached_candidates = self.lexicon.find_candidates(&self.buffer, limit);
        self.reconcile_selection();
        self.cached_candidates.clone()
    }

    pub fn selected_index(&self) -> Option<usize> {
        (!self.cached_candidates.is_empty()).then_some(self.selected_index)
    }

    pub fn selected_candidate(&self) -> Option<&Candidate> {
        self.cached_candidates.get(self.selected_index)
    }

    pub fn select_next(&mut self) -> Option<usize> {
        if self.cached_candidates.is_empty() {
            return None;
        }

        self.selected_index = (self.selected_index + 1) % self.cached_candidates.len();
        Some(self.selected_index)
    }

    pub fn select_previous(&mut self) -> Option<usize> {
        if self.cached_candidates.is_empty() {
            return None;
        }

        self.selected_index = if self.selected_index == 0 {
            self.cached_candidates.len() - 1
        } else {
            self.selected_index - 1
        };
        Some(self.selected_index)
    }

    pub fn commit_selected(
        &mut self,
        index: usize,
    ) -> std::result::Result<CommitOutcome, EngineError> {
        let committed = self
            .cached_candidates
            .get(index)
            .cloned()
            .ok_or(EngineError::InvalidCandidateIndex(index))?
            .word;

        let cleared_buffer = std::mem::take(&mut self.buffer);
        self.cached_candidates.clear();
        self.selected_index = 0;

        Ok(CommitOutcome {
            committed: Some(committed),
            cleared_buffer,
        })
    }

    pub fn commit_current(&mut self) -> std::result::Result<CommitOutcome, EngineError> {
        self.commit_selected(self.selected_index)
    }

    fn refresh_candidates(&mut self) {
        if self.buffer.is_empty() {
            self.cached_candidates.clear();
            self.selected_index = 0;
        } else {
            self.cached_candidates = self
                .lexicon
                .find_candidates(&self.buffer, self.config.shortlist_size);
            self.reconcile_selection();
        }
    }

    fn reconcile_selection(&mut self) {
        if self.cached_candidates.is_empty() || self.selected_index >= self.cached_candidates.len()
        {
            self.selected_index = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Lexicon, LexiconArtifact, LexiconEntry, Session};
    use crate::{ARTIFACT_VERSION, ArtifactStats, KeyIndexRecord, latin_input_key};

    fn fixture_lexicon() -> Lexicon {
        Lexicon::from_artifact(LexiconArtifact {
            version: ARTIFACT_VERSION,
            stats: ArtifactStats::default(),
            entries: vec![
                LexiconEntry {
                    word: "प्रवेश".into(),
                    normalized_word: "प्रवेश".into(),
                    romanized: "pravesha".into(),
                    normalized_key: latin_input_key("pravesh"),
                    gloss: Some("entry".into()),
                    source_mask: 0b11,
                    source_weight: 2,
                },
                LexiconEntry {
                    word: "परीक्षा".into(),
                    normalized_word: "परीक्षा".into(),
                    romanized: "pariikshaa".into(),
                    normalized_key: latin_input_key("pariksha"),
                    gloss: Some("exam".into()),
                    source_mask: 0b01,
                    source_weight: 1,
                },
            ],
            key_index: vec![
                KeyIndexRecord {
                    key: latin_input_key("pariksha"),
                    entry_indices: vec![1],
                },
                KeyIndexRecord {
                    key: latin_input_key("pravesh"),
                    entry_indices: vec![0],
                },
            ],
        })
    }

    #[test]
    fn finds_candidates_for_fuzzy_input() {
        let lexicon = fixture_lexicon();
        let candidates = lexicon.find_candidates("parbesh", 5);
        assert_eq!(
            candidates.first().map(|candidate| candidate.word.as_str()),
            Some("प्रवेश")
        );
    }

    #[test]
    fn session_handles_backspace_and_commit() {
        let lexicon = fixture_lexicon();
        let mut session = Session::new(lexicon);

        for ch in "prabesh".chars() {
            session.append_keystroke(ch);
        }

        assert_eq!(
            session
                .get_preedit()
                .auto_selected
                .as_ref()
                .map(|candidate| candidate.word.as_str()),
            Some("प्रवेश")
        );
        assert_eq!(session.get_preedit().selected_index, Some(0));

        session.backspace();
        assert!(!session.get_candidates(5).is_empty());

        session.append_keystroke('h');
        let commit = session.commit_selected(0).expect("candidate should exist");
        assert_eq!(commit.committed.as_deref(), Some("प्रवेश"));
        assert!(session.get_preedit().latin_buffer.is_empty());
    }

    #[test]
    fn session_can_replace_input_in_one_step() {
        let lexicon = fixture_lexicon();
        let mut session = Session::new(lexicon);
        session.apply_input("pariksha");

        let candidates = session.get_candidates(5);
        assert_eq!(
            candidates.first().map(|candidate| candidate.word.as_str()),
            Some("परीक्षा")
        );
    }

    #[test]
    fn returns_fallback_when_dictionary_has_no_match() {
        let lexicon = fixture_lexicon();
        let candidates = lexicon.find_candidates("moiz", 5);
        assert_eq!(
            candidates.first().map(|candidate| candidate.word.as_str()),
            Some("मोइज")
        );
    }

    #[test]
    fn keeps_phonetic_fallback_visible_when_dictionary_has_nearby_matches() {
        let lexicon = fixture_lexicon();
        let candidates = lexicon.find_candidates("prabesh", 1);
        assert_eq!(
            candidates.first().map(|candidate| candidate.word.as_str()),
            Some("परबेश")
        );
    }

    #[test]
    fn session_can_navigate_candidates_before_commit() {
        let lexicon = Lexicon::from_artifact(LexiconArtifact {
            version: ARTIFACT_VERSION,
            stats: ArtifactStats::default(),
            entries: vec![
                LexiconEntry {
                    word: "प्रवेश".into(),
                    normalized_word: "प्रवेश".into(),
                    romanized: "pravesha".into(),
                    normalized_key: latin_input_key("pravesh"),
                    gloss: Some("entry".into()),
                    source_mask: 0b11,
                    source_weight: 2,
                },
                LexiconEntry {
                    word: "परबेश".into(),
                    normalized_word: "परबेश".into(),
                    romanized: "prabesh".into(),
                    normalized_key: latin_input_key("prabesh"),
                    gloss: Some("fallback spelling".into()),
                    source_mask: 0b01,
                    source_weight: 1,
                },
            ],
            key_index: vec![KeyIndexRecord {
                key: latin_input_key("prabesh"),
                entry_indices: vec![0, 1],
            }],
        });

        let mut session = Session::new(lexicon);
        session.apply_input("prabesh");
        assert_eq!(session.selected_index(), Some(0));

        session.select_next();
        assert_eq!(session.selected_index(), Some(1));
        assert_eq!(
            session
                .get_preedit()
                .auto_selected
                .as_ref()
                .map(|candidate| candidate.word.as_str()),
            Some("परबेश")
        );

        session.select_previous();
        assert_eq!(session.selected_index(), Some(0));

        session.select_next();
        let commit = session
            .commit_current()
            .expect("selected candidate should commit");
        assert_eq!(commit.committed.as_deref(), Some("परबेश"));
    }
}
