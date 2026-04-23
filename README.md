# TheNepaliKeyboard

An offline Nepali transliteration workspace in Rust.

The current workspace already supports fast incremental transliteration, candidate ranking,
highlighted selection, and IME-style candidate navigation in the demo host. It is not yet a
packaged Windows input method that you can install from the language picker, but the engine and
host contract are now shaped around that workflow.

## Workspace

- `crates/core-engine`: transliteration core, fuzzy matching, lexicon loading, and session state
- `crates/data-builder`: offline dictionary ingestion and artifact generation from SQLite sources
- `crates/host-api`: stable host-facing API plus Windows TSF and Linux IME adapter scaffolding
- `crates/cli`: demo REPL, adapter simulation, and lightweight benchmark harness
- `crates/windows-tip`: Windows TIP registration/export crate plus `tipctl` install/status helper

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

## Windows TIP Tooling

The repo now includes a Windows-only registration crate that prepares the project to be surfaced as a
Text Services Framework (TSF) text service.

Check status:

```powershell
cargo run -p windows-tip --bin tipctl -- status
```

Register the built DLL with the current user profile:

```powershell
cargo run -p windows-tip --bin tipctl -- register --dll target\debug\windows_tip.dll
```

Unregister it:

```powershell
cargo run -p windows-tip --bin tipctl -- unregister
```

Current scope note: the registration and packaging side is in place, but the full in-proc TSF text
service activation and composition UI layer still needs to be implemented before this behaves like a
fully selectable Google Input Tools style Windows IME.
