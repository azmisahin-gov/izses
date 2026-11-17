# AGENTS.md — İZSES repository notes

Persistent context for future agent sessions. Read this before making changes.

## What this project is

İZSES is a **from-scratch, offline, pure-Rust Windows desktop application** for
forensic audio transcription (Turkish expertise field **56.12 – Ses Kayıtlarının
Metin Hâline Dönüştürülmesi**). The user is a court-appointed expert (bilirkişi).

The deliverable is a working Windows x64 app, not a library. Success is measured
by whether an expert can complete a full investigation end to end, offline.

## Hard constraints (do not violate)

- **Pure Rust.** No Python, JavaScript, Node.js, Electron, Tauri, browser engine,
  or C++ Whisper engine. The only accepted C-containing dependencies are
  `rusqlite` (bundled SQLite, public domain) and, on the Windows target only,
  `rodio` (audio playback). See `NOTICE.md`.
- **Fully offline.** No network calls at runtime. Model files ship with the
  installer. Never add a download-at-runtime path.
- **Never modify source evidence.** Acquisition opens sources read-only and
  writes only to the investigation workspace.
- **Never show a failed/partial operation as successful.** Hash mismatch means
  the acquisition is not verified. Half-copied files (`.kismi`) are never valid.
- **Turkish UI and Turkish domain terms.** Keep identifiers in English, prose
  and user-facing strings in Turkish.
- **Do not fabricate model confidence or speaker identity.** If the model gives
  no score, leave it empty. Speaker labels are set only by the expert.

## Layout

```
crates/
  izses-core     domain model, SQLite persistence, SHA-256, device discovery, audit
  izses-audio    symphonia decoding, resampling, waveform, spectrogram, technical probe
  izses-asr      candle Whisper inference, mel spectrogram, model store/integrity
  izses-report   report assembly, consistency checks, DOCX/PDF/HTML export
  izses-app      services (acquisition, transcription queue, session, backup) + egui UI
installer/       Inno Setup script
scripts/         PowerShell helper scripts
```

## Commands

```bash
cargo build                 # dev build (Linux works for everything except audio playback)
cargo test                  # all tests, including crates/izses-app/tests/acceptance.rs
cargo build --release --target x86_64-pc-windows-msvc   # delivery target
```

### Windows cross-compilation in the dev container

There is no MSVC on Linux. To type-check or link the Windows target, use a
Debian mingw-w64 toolchain extracted without root:

1. Extract these into one root (see `crates` note below):
   `binutils-mingw-w64-x86-64`, `gcc-mingw-w64-x86-64-posix`,
   `gcc-mingw-w64-x86-64-win32-runtime`, `mingw-w64-x86-64-dev`,
   `mingw-w64-common` (all from deb.debian.org).
2. `.cargo/config.toml` maps `x86_64-pc-windows-gnu` to the mingw gcc/ar.
3. `rustup target add x86_64-pc-windows-gnu`.
4. `cargo build --release --target x86_64-pc-windows-gnu -p izses-app`

`x86_64-pc-windows-gnu` is used only as a **proxy** to prove the Windows code
paths compile and link. The shipped target is `x86_64-pc-windows-msvc`.

`rust-toolchain.toml` pins the MSVC target; if you add GNU you must also add it
to `rust-toolchain.toml` or the pinned toolchain will refuse to build it.

## Design decisions worth knowing

- **Storage key is persisted.** `Investigation::storage_key` is assigned once at
  creation (`assign_storage_key`) and stored in SQLite. It is NOT derived from
  `case_number` on every call. This keeps data in place if the case number is
  edited, and makes `-2`, `-3` collision suffixes stable. `storage_key()` falls
  back to sanitizing the case number only when no key was assigned.
- **Transcript chunks are the resume unit.** Audio is split into 30 s chunks.
  After each chunk the segments are persisted and `completed_segments` is
  updated. On restart, `resume_from` is derived from the highest saved `end_ms`.
- **Auto text and expert text are separate columns** (`auto_text` vs
  `edited_text`). `effective_text()` returns the expert version when present.
  Never overwrite `auto_text`.
- **Device status honesty.** The app never claims a source device is
  hardware write-protected. It only states that it does not write to the source.
- **PDF is written by our own minimal writer** (`izses-report/src/pdf.rs`), with
  an embedded TrueType font (Identity-H / CIDFontType2 + ToUnicode). This keeps
  Turkish characters correct without an external PDF engine. Validate with
  `pypdf` text extraction after changing it.
- **`tokenizers` uses the `fancy-regex` feature, not `onig`.** `onig` pulls in
  C (oniguruma) and breaks the pure-Rust rule and cross-compilation.

## Testing

- Unit tests live beside the code in each crate.
- `crates/izses-app/tests/acceptance.rs` is the mandatory end-to-end scenario
  from the spec (section 12). Keep it passing; it is the definition of done.
- Use **synthetic data only** in tests. Never real evidence.
- PDF/DOCX tests skip gracefully when no font is available.

## Gotchas

- `eframe` needs `x11`/`wayland` features to build on Linux; audio playback is
  `cfg(windows)`-gated so the app still builds without ALSA headers.
- Borrow conflicts in egui closures are avoided by cloning state out of
  `app.session` / `app.report` before the closure, and by using
  `app.with_store(...)` or `app.store_handle()` instead of `app.store()` when
  `self` is also mutated.
- `Store` uses WAL and `foreign_keys=ON`; deletes cascade.
