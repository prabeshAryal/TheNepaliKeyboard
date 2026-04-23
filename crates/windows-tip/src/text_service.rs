//! Full TSF Text Input Processor implementation.
//!
//! This module implements the COM interfaces required for a Windows Text
//! Services Framework (TSF) text input method:
//!
//!  - `ITfTextInputProcessor` — lifecycle (Activate / Deactivate)
//!  - `ITfKeyEventSink`       — keystroke interception
//!  - `ITfCompositionSink`    — composition termination notification

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use windows::core::{implement, Error, Interface, GUID, Result};
use windows::Win32::Foundation::{BOOL, E_FAIL, LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_BACK, VK_DOWN, VK_ESCAPE, VK_RETURN, VK_SPACE, VK_UP,
};
use windows::Win32::UI::TextServices::{
    ITfComposition, ITfCompositionSink, ITfCompositionSink_Impl, ITfContext,
    ITfContextComposition, ITfEditSession, ITfEditSession_Impl, ITfInsertAtSelection,
    ITfKeyEventSink, ITfKeyEventSink_Impl, ITfKeystrokeMgr, ITfRange,
    ITfTextInputProcessor, ITfTextInputProcessor_Impl, ITfThreadMgr,
    TF_IAS_QUERYONLY, TF_ST_CORRECTION, TF_ES_READWRITE, TF_ES_SYNC,
};

use core_engine::Lexicon;
use host_api::{HostKeyEvent, HostSession, SessionConfig};



// ─── Global lexicon state ────────────────────────────────────────────
// The lexicon is expensive to load; it is lazily initialized once per DLL
// lifetime and then shared across all text-service instances created by
// DllGetClassObject.

static LEXICON_INIT: once_cell::sync::OnceCell<Lexicon> = once_cell::sync::OnceCell::new();

/// Resolve the lexicon artifact path next to the DLL.
fn default_lexicon_path() -> PathBuf {
    // Try to find the lexicon file next to the DLL
    let mut path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    path.pop();
    path.push("nepali.lexicon.bin");
    if !path.exists() {
        // Fall back to the artifacts directory
        path = PathBuf::from("artifacts/nepali.lexicon.bin");
    }
    path
}

pub fn get_or_init_lexicon() -> Option<&'static Lexicon> {
    LEXICON_INIT
        .get_or_try_init(|| {
            let path = default_lexicon_path();
            Lexicon::load_from_path(&path).map_err(|_| ())
        })
        .ok()
}

// ─── NepaliTextService ───────────────────────────────────────────────
// Interior mutability via RefCell – TSF calls us on a single-threaded
// apartment, so this is safe.

#[implement(ITfTextInputProcessor, ITfKeyEventSink, ITfCompositionSink)]
pub struct NepaliTextService {
    /// The TSF thread-manager cached during Activate.
    thread_mgr: RefCell<Option<ITfThreadMgr>>,
    /// Our TfClientId assigned by the framework.
    client_id: AtomicU32,
    /// The transliteration session wrapping the core engine.
    session: RefCell<Option<HostSession>>,
    /// Active TSF composition object, if any.
    composition: RefCell<Option<ITfComposition>>,
}

impl NepaliTextService {
    pub fn new() -> Self {
        Self {
            thread_mgr: RefCell::new(None),
            client_id: AtomicU32::new(0),
            session: RefCell::new(None),
            composition: RefCell::new(None),
        }
    }

    fn tid(&self) -> u32 {
        self.client_id.load(Ordering::SeqCst)
    }

    /// Returns true if we are currently composing (buffer is non-empty).
    fn is_composing(&self) -> bool {
        self.composition.borrow().is_some()
    }

    /// Determine whether we should intercept this virtual-key code.
    fn should_eat_key(&self, vk: u16) -> bool {
        let composing = self.is_composing();

        // Always eat printable ASCII when we could start/extend a composition.
        if (b'A'..=b'Z').contains(&(vk as u8)) || (b'0'..=b'9').contains(&(vk as u8)) {
            return true;
        }

        // While composing, also eat navigation/commit keys.
        if composing {
            matches!(
                vk,
                _ if vk == VK_BACK.0
                    || vk == VK_RETURN.0
                    || vk == VK_ESCAPE.0
                    || vk == VK_UP.0
                    || vk == VK_DOWN.0
                    || vk == VK_SPACE.0
            )
        } else {
            false
        }
    }

    /// Core handler: process a key and inject text into the TSF context.
    fn handle_key(&self, context: &ITfContext, vk: u16) -> Result<()> {
        let vk_u8 = vk as u8;

        // Determine engine event from the virtual key
        let event = if (b'A'..=b'Z').contains(&vk_u8) {
            let ch = (vk_u8 as char).to_ascii_lowercase();
            HostKeyEvent::Character(ch)
        } else if vk == VK_BACK.0 {
            HostKeyEvent::Backspace
        } else if vk == VK_RETURN.0 || vk == VK_SPACE.0 {
            HostKeyEvent::CommitCurrent
        } else if vk == VK_ESCAPE.0 {
            HostKeyEvent::Reset
        } else if vk == VK_UP.0 {
            HostKeyEvent::PrevCandidate
        } else if vk == VK_DOWN.0 {
            HostKeyEvent::NextCandidate
        } else {
            // Unknown key – ignore
            return Ok(());
        };

        // Process through host-api session
        let mut session_guard = self.session.borrow_mut();
        let session = match session_guard.as_mut() {
            Some(s) => s,
            None => return Ok(()),
        };

        match &event {
            HostKeyEvent::Character(ch) => {
                session.append_keystroke(*ch);
                let preedit = session.get_preedit();

                // Start composition if needed
                if !self.is_composing() {
                    self.start_composition(context)?;
                }

                // Update the composition text to show the top candidate
                if let Some(candidate) = preedit.auto_selected.as_ref() {
                    self.set_composition_text(context, &candidate.word)?;
                } else {
                    self.set_composition_text(context, &preedit.latin_buffer)?;
                }
            }
            HostKeyEvent::Backspace => {
                session.backspace();
                let preedit = session.get_preedit();

                if preedit.latin_buffer.is_empty() {
                    // End composition, inserting nothing
                    self.end_composition(context, "")?;
                } else if let Some(candidate) = preedit.auto_selected.as_ref() {
                    self.set_composition_text(context, &candidate.word)?;
                } else {
                    self.set_composition_text(context, &preedit.latin_buffer)?;
                }
            }
            HostKeyEvent::CommitCurrent => {
                let result = session.commit_current();
                match result {
                    Ok(outcome) => {
                        let text = outcome.committed.unwrap_or_default();
                        self.end_composition(context, &text)?;
                    }
                    Err(_) => {
                        // If no candidates, commit latin buffer verbatim
                        let preedit = session.get_preedit();
                        let text = preedit.latin_buffer.clone();
                        self.end_composition(context, &text)?;
                    }
                }
            }
            HostKeyEvent::Reset => {
                session.reset_session();
                self.end_composition(context, "")?;
            }
            HostKeyEvent::PrevCandidate => {
                session.select_previous();
                let preedit = session.get_preedit();
                if let Some(candidate) = preedit.auto_selected.as_ref() {
                    self.set_composition_text(context, &candidate.word)?;
                }
            }
            HostKeyEvent::NextCandidate => {
                session.select_next();
                let preedit = session.get_preedit();
                if let Some(candidate) = preedit.auto_selected.as_ref() {
                    self.set_composition_text(context, &candidate.word)?;
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Start a new TSF composition in the context.
    fn start_composition(&self, context: &ITfContext) -> Result<()> {
        let tid = self.tid();
        let this_sink: ITfCompositionSink = unsafe { self.cast()? };

        // We need an edit session to start a composition.
        let session = StartCompositionEditSession {
            context: context.clone(),
            sink: this_sink,
            composition: self.composition.clone(),
        };
        let session_iface: ITfEditSession = session.into();

        unsafe {
            let _hr = context.RequestEditSession(tid, &session_iface, TF_ES_READWRITE | TF_ES_SYNC)?;
        }

        Ok(())
    }

    /// Update the text inside the active composition range.
    fn set_composition_text(&self, context: &ITfContext, text: &str) -> Result<()> {
        let comp = self.composition.borrow();
        let composition = match comp.as_ref() {
            Some(c) => c.clone(),
            None => return Ok(()),
        };
        drop(comp);

        let tid = self.tid();
        let text_owned = text.to_string();

        let session = SetTextEditSession {
            composition,
            text: text_owned,
        };
        let session_iface: ITfEditSession = session.into();

        unsafe {
            let _hr = context.RequestEditSession(tid, &session_iface, TF_ES_READWRITE | TF_ES_SYNC)?;
        }

        Ok(())
    }

    /// End the active composition, optionally committing final text.
    fn end_composition(&self, context: &ITfContext, final_text: &str) -> Result<()> {
        let comp = self.composition.borrow().clone();
        let composition = match comp {
            Some(c) => c,
            None => return Ok(()),
        };

        let tid = self.tid();
        let text = final_text.to_string();

        let session = EndCompositionEditSession {
            composition: composition.clone(),
            text,
        };
        let session_iface: ITfEditSession = session.into();

        unsafe {
            let _hr = context.RequestEditSession(tid, &session_iface, TF_ES_READWRITE | TF_ES_SYNC)?;
        }

        // Clear our reference
        *self.composition.borrow_mut() = None;

        Ok(())
    }
}

// ─── ITfTextInputProcessor ──────────────────────────────────────────

impl ITfTextInputProcessor_Impl for NepaliTextService_Impl {
    fn Activate(&self, ptim: Option<&ITfThreadMgr>, tid: u32) -> Result<()> {
        let thread_mgr = ptim.ok_or(Error::from(E_FAIL))?;

        // Store the thread-manager and client ID.
        *self.thread_mgr.borrow_mut() = Some(thread_mgr.clone());
        self.client_id.store(tid, Ordering::SeqCst);

        // Initialize the transliteration session with the global lexicon.
        if let Some(lexicon) = get_or_init_lexicon() {
            let config = SessionConfig { shortlist_size: 9 };
            *self.session.borrow_mut() = Some(HostSession::with_config(lexicon.clone(), config));
        }

        // Register ourselves as the key-event sink.
        unsafe {
            let keystroke_mgr: ITfKeystrokeMgr = thread_mgr.cast()?;
            let sink: ITfKeyEventSink = self.cast()?;
            keystroke_mgr.AdviseKeyEventSink(tid, &sink, BOOL::from(true))?;
        }

        Ok(())
    }

    fn Deactivate(&self) -> Result<()> {
        let tid = self.tid();
        let thread_mgr = self.thread_mgr.borrow().clone();

        if let Some(ref tm) = thread_mgr {
            unsafe {
                let keystroke_mgr: ITfKeystrokeMgr = tm.cast()?;
                let _ = keystroke_mgr.UnadviseKeyEventSink(tid);
            }
        }

        *self.thread_mgr.borrow_mut() = None;
        *self.session.borrow_mut() = None;
        *self.composition.borrow_mut() = None;
        self.client_id.store(0, Ordering::SeqCst);

        Ok(())
    }
}

// ─── ITfKeyEventSink ─────────────────────────────────────────────────

impl ITfKeyEventSink_Impl for NepaliTextService_Impl {
    fn OnSetFocus(&self, _fforeground: BOOL) -> Result<()> {
        Ok(())
    }

    fn OnTestKeyDown(
        &self,
        _pic: Option<&ITfContext>,
        wparam: WPARAM,
        _lparam: LPARAM,
    ) -> Result<BOOL> {
        let vk = wparam.0 as u16;
        Ok(self.should_eat_key(vk).into())
    }

    fn OnTestKeyUp(
        &self,
        _pic: Option<&ITfContext>,
        _wparam: WPARAM,
        _lparam: LPARAM,
    ) -> Result<BOOL> {
        Ok(false.into())
    }

    fn OnKeyDown(
        &self,
        pic: Option<&ITfContext>,
        wparam: WPARAM,
        _lparam: LPARAM,
    ) -> Result<BOOL> {
        let context = match pic {
            Some(c) => c,
            None => return Ok(false.into()),
        };

        let vk = wparam.0 as u16;
        if !self.should_eat_key(vk) {
            return Ok(false.into());
        }

        self.handle_key(context, vk)?;
        Ok(true.into())
    }

    fn OnKeyUp(
        &self,
        _pic: Option<&ITfContext>,
        _wparam: WPARAM,
        _lparam: LPARAM,
    ) -> Result<BOOL> {
        Ok(false.into())
    }

    fn OnPreservedKey(
        &self,
        _pic: Option<&ITfContext>,
        _rguid: *const GUID,
    ) -> Result<BOOL> {
        Ok(false.into())
    }
}

// ─── ITfCompositionSink ──────────────────────────────────────────────

impl ITfCompositionSink_Impl for NepaliTextService_Impl {
    fn OnCompositionTerminated(
        &self,
        _ecwrite: u32,
        _pcomposition: Option<&ITfComposition>,
    ) -> Result<()> {
        // The framework terminated our composition externally (e.g. focus change).
        *self.composition.borrow_mut() = None;
        if let Some(session) = self.session.borrow_mut().as_mut() {
            session.reset_session();
        }
        Ok(())
    }
}

// ─── Edit session helpers (COM objects called back by the framework) ─

/// Edit session: start a new composition.
#[implement(ITfEditSession)]
struct StartCompositionEditSession {
    context: ITfContext,
    sink: ITfCompositionSink,
    composition: RefCell<Option<ITfComposition>>,
}

impl ITfEditSession_Impl for StartCompositionEditSession_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        unsafe {
            let context_composition: ITfContextComposition = self.context.cast()?;
            let insert: ITfInsertAtSelection = self.context.cast()?;

            // Get a range at the current insertion point
            let range: ITfRange =
                insert.InsertTextAtSelection(ec, TF_IAS_QUERYONLY, &[])?;

            // Start the composition on that range
            let composition =
                context_composition.StartComposition(ec, &range, &self.sink)?;

            *self.composition.borrow_mut() = Some(composition);
        }
        Ok(())
    }
}

/// Edit session: update text in the composition range.
#[implement(ITfEditSession)]
struct SetTextEditSession {
    composition: ITfComposition,
    text: String,
}

impl ITfEditSession_Impl for SetTextEditSession_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        unsafe {
            let range: ITfRange = self.composition.GetRange()?;
            let wide: Vec<u16> = self.text.encode_utf16().collect();
            range.SetText(ec, TF_ST_CORRECTION, &wide)?;
        }
        Ok(())
    }
}

/// Edit session: end composition and optionally set final text.
#[implement(ITfEditSession)]
struct EndCompositionEditSession {
    composition: ITfComposition,
    text: String,
}

impl ITfEditSession_Impl for EndCompositionEditSession_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        unsafe {
            if !self.text.is_empty() {
                let range: ITfRange = self.composition.GetRange()?;
                let wide: Vec<u16> = self.text.encode_utf16().collect();
                range.SetText(ec, TF_ST_CORRECTION, &wide)?;
            }
            self.composition.EndComposition(ec)?;
        }
        Ok(())
    }
}
