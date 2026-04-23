# TheNepaliKeyboard

A production-ready offline Nepali transliteration keyboard for Windows, written in Rust.

Works like **Google Input Tools** — type Latin text and get Nepali candidates inline. The
transliteration engine runs entirely offline using a pre-built lexicon artifact derived from
two Nepali dictionary databases (~640 MB of source data compressed to ~85 MB binary).

## How It Works

1. You type Latin characters (e.g. `namaste`, `prabesh`, `nepal`)
2. The engine fuzzy-matches against 100k+ Nepali headwords in real time
3. The top candidate appears inline in the text field as a TSF composition
4. Press **Enter** or **Space** to commit, **Up/Down** to cycle candidates, **Escape** to cancel

## Workspace

- `crates/core-engine`: transliteration core, fuzzy matching, lexicon loading, and session state
- `crates/data-builder`: offline dictionary ingestion and artifact generation from SQLite sources
- `crates/host-api`: stable host-facing API plus Windows TSF and Linux IME adapter scaffolding
- `crates/cli`: demo REPL, adapter simulation, and lightweight benchmark harness
- `crates/windows-tip`: **Complete Windows TIP** — COM DLL implementing `ITfTextInputProcessor`,
  `ITfKeyEventSink`, and `ITfCompositionSink` with `IClassFactory` for in-process activation

## Quick Start

1. Build a lexicon artifact from the bundled dictionaries:

```powershell
cargo run -p data-builder -- build --output artifacts/nepali.lexicon.bin
```

2. Run the interactive demo:

```powershell
cargo run -p cli -- demo --lexicon artifacts/nepali.lexicon.bin

The demo supports IME-style commands:

- `:up` and `:down` to move through candidates
- `:enter` to commit the currently highlighted candidate
- `:back`, `:reset`, `:commit N`, and `:quit`
```

3. Look up related Nepali words for a Latin input:

```powershell
cargo run -p cli -- lookup --lexicon artifacts/nepali.lexicon.bin --input prabesh
```

4. Run the benchmark harness:

```powershell
cargo run -p cli -- bench --lexicon artifacts/nepali.lexicon.bin
```

5. Simulate a platform adapter flow without a full OS integration layer:

```powershell
cargo run -p cli -- simulate --lexicon artifacts/nepali.lexicon.bin --platform windows --input prabesh
```

## Installing the Windows Input Method

### Build the DLL

```powershell
cargo build -p windows-tip --release
```

This produces `target/release/windows_tip.dll`.

### Deploy the lexicon

Copy the lexicon artifact next to the DLL:

```powershell
Copy-Item artifacts/nepali.lexicon.bin target/release/nepali.lexicon.bin
```

### Register with Windows

```powershell
cargo run -p windows-tip --bin tipctl -- register --dll target\release\windows_tip.dll
```

### Add the keyboard

1. Open **Settings → Time & Language → Language & Region**
2. Add **Nepali (Nepal)** if not already present
3. Under Nepali, click **Language options → Add a keyboard**
4. Select **Nepali Transliteration (The Nepali Keyboard)**

### Unregister

```powershell
cargo run -p windows-tip --bin tipctl -- unregister
```

### Check status

```powershell
cargo run -p windows-tip --bin tipctl -- status
```

## Architecture

```
┌──────────────────┐
│   Windows TSF    │  ← ITfTextInputProcessor, ITfKeyEventSink
│   (windows-tip)  │     ITfCompositionSink, IClassFactory
└────────┬─────────┘
         │  HostKeyEvent → HostAction
┌────────▼─────────┐
│    host-api       │  ← Platform adapter layer
└────────┬─────────┘
         │
┌────────▼─────────┐
│   core-engine     │  ← Transliteration, fuzzy matching, candidate ranking
└────────┬─────────┘
         │
┌────────▼─────────┐
│ nepali.lexicon.bin│  ← Pre-built binary artifact (bincode)
└──────────────────┘
         ▲
┌────────┴─────────┐
│   data-builder    │  ← Offline ingestion from db.sqlite + content.db
└──────────────────┘
```

## Technical Details

- **Lexicon**: ~100k unique Nepali headwords with romanization keys, glosses, and source weights
- **Matching**: Prefix search on a sorted key index with edit-distance scoring and phonetic fallback
- **Transliteration**: Rule-based Latin→Devanagari with multi-char digraph support (kh→ख, sh→श, etc.)
- **TSF Integration**: Full COM DLL with `DllGetClassObject`/`DllRegisterServer` entry points
- **Session**: Incremental keystroke processing with candidate navigation (up/down/commit)
