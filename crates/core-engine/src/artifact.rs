use serde::{Deserialize, Serialize};

pub const ARTIFACT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LexiconArtifact {
    pub version: u32,
    pub stats: ArtifactStats,
    pub entries: Vec<LexiconEntry>,
    pub key_index: Vec<KeyIndexRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ArtifactStats {
    pub sabdakosh_rows: u32,
    pub content_rows: u32,
    pub unique_headwords: u32,
    pub indexed_keys: u32,
    pub dropped_empty_words: u32,
    pub dropped_unromanizable_words: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LexiconEntry {
    pub word: String,
    pub normalized_word: String,
    pub romanized: String,
    pub normalized_key: String,
    pub gloss: Option<String>,
    pub source_mask: u8,
    pub source_weight: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyIndexRecord {
    pub key: String,
    pub entry_indices: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LexiconSource {
    Sabdakosh = 0b0000_0001,
    Content = 0b0000_0010,
}

impl LexiconSource {
    pub const fn mask(self) -> u8 {
        self as u8
    }
}
