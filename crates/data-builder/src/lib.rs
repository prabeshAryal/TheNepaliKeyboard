use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result};
use core_engine::{
    ARTIFACT_VERSION, ArtifactStats, KeyIndexRecord, LexiconArtifact, LexiconEntry, LexiconSource,
    normalize_nepali_word, transliteration_key_for_word,
};
use html_escape::decode_html_entities;
use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::Connection;
use unicode_normalization::UnicodeNormalization;

static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]+>").expect("valid html tag regex"));
static WHITESPACE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\s+").expect("valid whitespace regex"));

#[derive(Debug, Clone)]
pub struct BuildConfig {
    pub sabdakosh_path: String,
    pub content_path: String,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            sabdakosh_path: "dictionaries/db.sqlite".into(),
            content_path: "dictionaries/content.db".into(),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct BuilderEntry {
    word: String,
    glosses: BTreeSet<String>,
    source_mask: u8,
    source_weight: u16,
}

pub fn build_artifact(config: &BuildConfig) -> Result<LexiconArtifact> {
    let mut merged: BTreeMap<String, BuilderEntry> = BTreeMap::new();
    let mut stats = ArtifactStats::default();

    let sabdakosh_conn = Connection::open(&config.sabdakosh_path)
        .with_context(|| format!("failed to open {}", &config.sabdakosh_path))?;
    ingest_sabdakosh(&sabdakosh_conn, &mut merged, &mut stats)?;

    let content_conn = Connection::open(&config.content_path)
        .with_context(|| format!("failed to open {}", &config.content_path))?;
    ingest_content(&content_conn, &mut merged, &mut stats)?;

    let mut entries = Vec::with_capacity(merged.len());
    let mut key_groups: BTreeMap<String, Vec<u32>> = BTreeMap::new();

    for entry in merged.into_values() {
        let normalized_word = normalize_nepali_word(&entry.word);
        if normalized_word.is_empty() {
            stats.dropped_empty_words += 1;
            continue;
        }

        let (romanized, normalized_key) = transliteration_key_for_word(&normalized_word);
        if normalized_key.is_empty() {
            stats.dropped_unromanizable_words += 1;
            continue;
        }

        let artifact_entry = LexiconEntry {
            word: entry.word,
            normalized_word,
            romanized,
            normalized_key: normalized_key.clone(),
            gloss: entry.glosses.into_iter().next(),
            source_mask: entry.source_mask,
            source_weight: entry.source_weight,
        };

        let index = entries.len() as u32;
        entries.push(artifact_entry);
        key_groups.entry(normalized_key).or_default().push(index);
    }

    stats.unique_headwords = entries.len() as u32;
    stats.indexed_keys = key_groups.len() as u32;

    let key_index = key_groups
        .into_iter()
        .map(|(key, mut entry_indices)| {
            entry_indices.sort_unstable();
            KeyIndexRecord { key, entry_indices }
        })
        .collect();

    Ok(LexiconArtifact {
        version: ARTIFACT_VERSION,
        stats,
        entries,
        key_index,
    })
}

pub fn write_artifact(path: impl AsRef<Path>, artifact: &LexiconArtifact) -> Result<()> {
    let bytes = bincode::serialize(artifact).context("failed to serialize artifact")?;
    if let Some(parent) = path.as_ref().parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create output directory {}", parent.display()))?;
    }
    std::fs::write(path.as_ref(), bytes)
        .with_context(|| format!("failed to write artifact to {}", path.as_ref().display()))?;
    Ok(())
}

fn ingest_sabdakosh(
    conn: &Connection,
    merged: &mut BTreeMap<String, BuilderEntry>,
    stats: &mut ArtifactStats,
) -> Result<()> {
    let mut statement = conn.prepare("SELECT word, meaning FROM sabdakosh")?;
    let rows = statement.query_map([], |row| {
        let word: String = row.get(0)?;
        let meaning: Option<String> = row.get(1)?;
        Ok((word, meaning))
    })?;

    for row in rows {
        let (word, meaning) = row?;
        stats.sabdakosh_rows += 1;
        push_entry(merged, LexiconSource::Sabdakosh, &word, meaning.as_deref());
    }

    Ok(())
}

fn ingest_content(
    conn: &Connection,
    merged: &mut BTreeMap<String, BuilderEntry>,
    stats: &mut ArtifactStats,
) -> Result<()> {
    let mut statement = conn.prepare("SELECT word, description FROM dictionary_content")?;
    let rows = statement.query_map([], |row| {
        let word: String = row.get(0)?;
        let description: Option<String> = row.get(1)?;
        Ok((word, description))
    })?;

    for row in rows {
        let (word, description) = row?;
        stats.content_rows += 1;
        push_entry(
            merged,
            LexiconSource::Content,
            &word,
            description.as_deref(),
        );
    }

    Ok(())
}

fn push_entry(
    merged: &mut BTreeMap<String, BuilderEntry>,
    source: LexiconSource,
    word: &str,
    gloss: Option<&str>,
) {
    let normalized = normalize_nepali_word(word);
    if normalized.is_empty() {
        return;
    }

    let cleaned_gloss = gloss.and_then(clean_gloss);

    let slot = merged
        .entry(normalized.clone())
        .or_insert_with(|| BuilderEntry {
            word: normalized,
            glosses: BTreeSet::new(),
            source_mask: 0,
            source_weight: 0,
        });

    slot.source_mask |= source.mask();
    slot.source_weight += 1;

    if let Some(gloss) = cleaned_gloss {
        slot.glosses.insert(gloss);
    }
}

fn clean_gloss(gloss: &str) -> Option<String> {
    let normalized = gloss.nfc().collect::<String>();
    let stripped = TAG_RE.replace_all(&normalized, " ");
    let decoded = decode_html_entities(&stripped);
    let compact = WHITESPACE_RE.replace_all(decoded.as_ref(), " ");
    let cleaned = compact.trim().to_string();

    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use core_engine::{ArtifactStats, Lexicon, latin_input_key};
    use rusqlite::Connection;
    use tempfile::NamedTempFile;

    use super::{
        BuildConfig, build_artifact, clean_gloss, ingest_content, ingest_sabdakosh, write_artifact,
    };

    #[test]
    fn strips_html_from_gloss() {
        let cleaned =
            clean_gloss("<p><strong>अर्थ</strong>&minus; नमस्ते</p>").expect("non-empty gloss");
        assert_eq!(cleaned, "अर्थ − नमस्ते");
    }

    #[test]
    fn produces_deterministic_artifact() -> Result<()> {
        let db_a = NamedTempFile::new()?;
        let db_b = NamedTempFile::new()?;

        seed_sabdakosh(db_a.path())?;
        seed_content(db_b.path())?;

        let config = BuildConfig {
            sabdakosh_path: db_a.path().display().to_string(),
            content_path: db_b.path().display().to_string(),
        };

        let first = build_artifact(&config)?;
        let second = build_artifact(&config)?;
        assert_eq!(first, second);

        let out = NamedTempFile::new()?;
        write_artifact(out.path(), &first)?;
        let lexicon = Lexicon::load_from_path(out.path())?;
        let candidates = lexicon.find_candidates("parbesh", 3);
        assert_eq!(
            candidates.first().map(|candidate| candidate.word.as_str()),
            Some("प्रवेश")
        );

        Ok(())
    }

    #[test]
    fn deduplicates_across_sources() -> Result<()> {
        let sabdakosh = Connection::open_in_memory()?;
        sabdakosh.execute(
            "CREATE TABLE sabdakosh (id INTEGER, word TEXT, meaning TEXT)",
            [],
        )?;
        sabdakosh.execute(
            "INSERT INTO sabdakosh (id, word, meaning) VALUES (1, 'प्रवेश', '<p>entry</p>')",
            [],
        )?;

        let content = Connection::open_in_memory()?;
        content.execute(
            "CREATE TABLE dictionary_content (_id INTEGER, word TEXT, description TEXT, home TEXT, image TEXT, version INTEGER, origin_id TEXT)",
            [],
        )?;
        content.execute(
            "INSERT INTO dictionary_content (_id, word, description, home, image, version, origin_id) VALUES (1, 'प्रवेश', '<p>arrival</p>', '', '', 1, '1')",
            [],
        )?;

        let mut merged = std::collections::BTreeMap::new();
        let mut stats = ArtifactStats::default();
        ingest_sabdakosh(&sabdakosh, &mut merged, &mut stats)?;
        ingest_content(&content, &mut merged, &mut stats)?;
        assert_eq!(merged.len(), 1);
        let only = merged.values().next().expect("deduped entry");
        assert_eq!(only.source_weight, 2);
        assert_eq!(only.source_mask, 0b11);
        assert!(only.glosses.contains("entry"));
        assert!(only.glosses.contains("arrival"));
        assert_eq!(stats.sabdakosh_rows, 1);
        assert_eq!(stats.content_rows, 1);
        assert_eq!(latin_input_key("prabesh"), latin_input_key("parbesh"));

        Ok(())
    }

    fn seed_sabdakosh(path: &Path) -> Result<()> {
        let conn = Connection::open(path)?;
        conn.execute(
            "CREATE TABLE sabdakosh (id INTEGER, word TEXT, meaning TEXT)",
            [],
        )?;
        conn.execute(
            "INSERT INTO sabdakosh (id, word, meaning) VALUES (1, 'प्रवेश', '<p>entry</p>'), (2, 'परीक्षा', '<p>exam</p>')",
            [],
        )?;
        Ok(())
    }

    fn seed_content(path: &Path) -> Result<()> {
        let conn = Connection::open(path)?;
        conn.execute(
            "CREATE TABLE dictionary_content (_id INTEGER, word TEXT, description TEXT, home TEXT, image TEXT, version INTEGER, origin_id TEXT)",
            [],
        )?;
        conn.execute(
            "INSERT INTO dictionary_content (_id, word, description, home, image, version, origin_id) VALUES (1, 'प्रवेश', '<p>arrival</p>', '', '', 1, '1')",
            [],
        )?;
        Ok(())
    }

    use std::path::Path;
}
