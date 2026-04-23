use core_engine::Session;

pub use core_engine::{Candidate, CommitOutcome, EngineError, Lexicon, Preedit, SessionConfig};

#[derive(Debug, Clone)]
pub enum HostKeyEvent {
    Character(char),
    Backspace,
    Reset,
    CommitSelection(usize),
    CommitCurrent,
    NextCandidate,
    PrevCandidate,
}

#[derive(Debug, Clone)]
pub enum HostAction {
    UpdatePreedit(Preedit),
    ShowCandidates(Vec<Candidate>),
    CommitText(String),
    ClearComposition,
    Noop,
}

pub trait PlatformAdapter {
    fn platform_id(&self) -> &'static str;
    fn handle_key_event(&mut self, event: HostKeyEvent) -> Result<Vec<HostAction>, EngineError>;
    fn session(&self) -> &HostSession;
}

#[derive(Debug, Clone)]
pub struct HostSession {
    inner: Session,
}

impl HostSession {
    pub fn new(lexicon: Lexicon) -> Self {
        Self::with_config(lexicon, SessionConfig::default())
    }

    pub fn with_config(lexicon: Lexicon, config: SessionConfig) -> Self {
        Self {
            inner: Session::with_config(lexicon, config),
        }
    }

    pub fn apply_input(&mut self, input: &str) {
        self.inner.apply_input(input);
    }

    pub fn append_keystroke(&mut self, ch: char) {
        self.inner.append_keystroke(ch);
    }

    pub fn backspace(&mut self) {
        self.inner.backspace();
    }

    pub fn reset_session(&mut self) {
        self.inner.reset_session();
    }

    pub fn get_preedit(&self) -> Preedit {
        self.inner.get_preedit()
    }

    pub fn get_candidates(&mut self, limit: usize) -> Vec<Candidate> {
        self.inner.get_candidates(limit)
    }

    pub fn commit_selected(&mut self, index: usize) -> Result<CommitOutcome, EngineError> {
        self.inner.commit_selected(index)
    }

    pub fn commit_current(&mut self) -> Result<CommitOutcome, EngineError> {
        self.inner.commit_current()
    }

    pub fn select_next(&mut self) -> Option<usize> {
        self.inner.select_next()
    }

    pub fn select_previous(&mut self) -> Option<usize> {
        self.inner.select_previous()
    }
}

#[derive(Debug, Clone)]
pub struct WindowsTsfAdapter {
    session: HostSession,
    composition_active: bool,
}

impl WindowsTsfAdapter {
    pub fn new(lexicon: Lexicon) -> Self {
        Self::with_config(lexicon, SessionConfig::default())
    }

    pub fn with_config(lexicon: Lexicon, config: SessionConfig) -> Self {
        Self {
            session: HostSession::with_config(lexicon, config),
            composition_active: false,
        }
    }
}

impl PlatformAdapter for WindowsTsfAdapter {
    fn platform_id(&self) -> &'static str {
        "windows-tsf"
    }

    fn handle_key_event(&mut self, event: HostKeyEvent) -> Result<Vec<HostAction>, EngineError> {
        match event {
            HostKeyEvent::Character(ch) => {
                self.session.append_keystroke(ch);
                self.composition_active = true;
                Ok(vec![
                    HostAction::UpdatePreedit(self.session.get_preedit()),
                    HostAction::ShowCandidates(self.session.get_candidates(5)),
                ])
            }
            HostKeyEvent::Backspace => {
                self.session.backspace();
                if self.session.get_preedit().latin_buffer.is_empty() {
                    self.composition_active = false;
                    Ok(vec![HostAction::ClearComposition])
                } else {
                    Ok(vec![
                        HostAction::UpdatePreedit(self.session.get_preedit()),
                        HostAction::ShowCandidates(self.session.get_candidates(5)),
                    ])
                }
            }
            HostKeyEvent::Reset => {
                self.session.reset_session();
                self.composition_active = false;
                Ok(vec![HostAction::ClearComposition])
            }
            HostKeyEvent::CommitSelection(index) => {
                let outcome = self.session.commit_selected(index)?;
                self.composition_active = false;
                Ok(vec![
                    HostAction::CommitText(outcome.committed.unwrap_or_default()),
                    HostAction::ClearComposition,
                ])
            }
            HostKeyEvent::CommitCurrent => {
                let outcome = self.session.commit_current()?;
                self.composition_active = false;
                Ok(vec![
                    HostAction::CommitText(outcome.committed.unwrap_or_default()),
                    HostAction::ClearComposition,
                ])
            }
            HostKeyEvent::NextCandidate => {
                self.session.select_next();
                Ok(vec![
                    HostAction::UpdatePreedit(self.session.get_preedit()),
                    HostAction::ShowCandidates(self.session.get_candidates(5)),
                ])
            }
            HostKeyEvent::PrevCandidate => {
                self.session.select_previous();
                Ok(vec![
                    HostAction::UpdatePreedit(self.session.get_preedit()),
                    HostAction::ShowCandidates(self.session.get_candidates(5)),
                ])
            }
        }
    }

    fn session(&self) -> &HostSession {
        &self.session
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxImeFramework {
    IBus,
    Fcitx5,
}

#[derive(Debug, Clone)]
pub struct LinuxImeAdapter {
    session: HostSession,
    framework: LinuxImeFramework,
}

impl LinuxImeAdapter {
    pub fn new(lexicon: Lexicon, framework: LinuxImeFramework) -> Self {
        Self::with_config(lexicon, framework, SessionConfig::default())
    }

    pub fn with_config(
        lexicon: Lexicon,
        framework: LinuxImeFramework,
        config: SessionConfig,
    ) -> Self {
        Self {
            session: HostSession::with_config(lexicon, config),
            framework,
        }
    }

    pub fn framework(&self) -> LinuxImeFramework {
        self.framework
    }
}

impl PlatformAdapter for LinuxImeAdapter {
    fn platform_id(&self) -> &'static str {
        match self.framework {
            LinuxImeFramework::IBus => "linux-ibus",
            LinuxImeFramework::Fcitx5 => "linux-fcitx5",
        }
    }

    fn handle_key_event(&mut self, event: HostKeyEvent) -> Result<Vec<HostAction>, EngineError> {
        match event {
            HostKeyEvent::Character(ch) => {
                self.session.append_keystroke(ch);
                Ok(vec![
                    HostAction::UpdatePreedit(self.session.get_preedit()),
                    HostAction::ShowCandidates(self.session.get_candidates(5)),
                ])
            }
            HostKeyEvent::Backspace => {
                self.session.backspace();
                Ok(vec![
                    HostAction::UpdatePreedit(self.session.get_preedit()),
                    HostAction::ShowCandidates(self.session.get_candidates(5)),
                ])
            }
            HostKeyEvent::Reset => {
                self.session.reset_session();
                Ok(vec![HostAction::ClearComposition])
            }
            HostKeyEvent::CommitSelection(index) => {
                let outcome = self.session.commit_selected(index)?;
                Ok(vec![
                    HostAction::CommitText(outcome.committed.unwrap_or_default()),
                    HostAction::ClearComposition,
                ])
            }
            HostKeyEvent::CommitCurrent => {
                let outcome = self.session.commit_current()?;
                Ok(vec![
                    HostAction::CommitText(outcome.committed.unwrap_or_default()),
                    HostAction::ClearComposition,
                ])
            }
            HostKeyEvent::NextCandidate => Ok(vec![
                {
                    self.session.select_next();
                    HostAction::UpdatePreedit(self.session.get_preedit())
                },
                HostAction::ShowCandidates(self.session.get_candidates(5)),
            ]),
            HostKeyEvent::PrevCandidate => Ok(vec![
                {
                    self.session.select_previous();
                    HostAction::UpdatePreedit(self.session.get_preedit())
                },
                HostAction::ShowCandidates(self.session.get_candidates(5)),
            ]),
        }
    }

    fn session(&self) -> &HostSession {
        &self.session
    }
}

#[cfg(test)]
mod tests {
    use core_engine::{
        ARTIFACT_VERSION, ArtifactStats, KeyIndexRecord, LexiconArtifact, LexiconEntry,
        latin_input_key,
    };

    use super::{
        HostAction, HostKeyEvent, Lexicon, LinuxImeAdapter, LinuxImeFramework, PlatformAdapter,
        WindowsTsfAdapter,
    };

    fn fixture_lexicon() -> Lexicon {
        Lexicon::from_artifact(LexiconArtifact {
            version: ARTIFACT_VERSION,
            stats: ArtifactStats::default(),
            entries: vec![LexiconEntry {
                word: "प्रवेश".into(),
                normalized_word: "प्रवेश".into(),
                romanized: "pravesha".into(),
                normalized_key: latin_input_key("pravesh"),
                gloss: Some("entry".into()),
                source_mask: 0b11,
                source_weight: 2,
            }],
            key_index: vec![KeyIndexRecord {
                key: latin_input_key("pravesh"),
                entry_indices: vec![0],
            }],
        })
    }

    #[test]
    fn windows_adapter_commits_top_candidate() {
        let lexicon = fixture_lexicon();
        let mut adapter = WindowsTsfAdapter::new(lexicon);

        for ch in "prabesh".chars() {
            let actions = adapter
                .handle_key_event(HostKeyEvent::Character(ch))
                .expect("valid event");
            assert!(
                actions
                    .iter()
                    .any(|action| matches!(action, HostAction::ShowCandidates(_)))
            );
        }

        let actions = adapter
            .handle_key_event(HostKeyEvent::CommitSelection(0))
            .expect("candidate should commit");

        assert!(
            actions
                .iter()
                .any(|action| matches!(action, HostAction::CommitText(text) if text == "प्रवेश"))
        );
    }

    #[test]
    fn windows_adapter_can_cycle_candidates() {
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
        let mut adapter = WindowsTsfAdapter::new(lexicon);

        for ch in "prabesh".chars() {
            adapter
                .handle_key_event(HostKeyEvent::Character(ch))
                .expect("valid event");
        }

        let actions = adapter
            .handle_key_event(HostKeyEvent::NextCandidate)
            .expect("candidate list should cycle");

        assert!(actions.iter().any(|action| {
            matches!(
                action,
                HostAction::UpdatePreedit(preedit)
                    if preedit.selected_index == Some(1)
                        && preedit.auto_selected.as_ref().map(|candidate| candidate.word.as_str()) == Some("परबेश")
            )
        }));
    }

    #[test]
    fn linux_adapters_share_the_same_session_contract() {
        for framework in [LinuxImeFramework::IBus, LinuxImeFramework::Fcitx5] {
            let lexicon = fixture_lexicon();
            let mut adapter = LinuxImeAdapter::new(lexicon, framework);
            for ch in "parbesh".chars() {
                adapter
                    .handle_key_event(HostKeyEvent::Character(ch))
                    .expect("valid event");
            }

            let actions = adapter
                .handle_key_event(HostKeyEvent::CommitSelection(0))
                .expect("candidate should commit");

            assert!(
                actions
                    .iter()
                    .any(|action| matches!(action, HostAction::CommitText(text) if text == "प्रवेश"))
            );
        }
    }
}
