# Tech Architecture

Status: decided

The app is a Tauri v2 desktop app with a React/TypeScript frontend and Rust backend.

## Local Services

- Ollama runs the local LLM.
- Supertonic runs as a bundled local HTTP TTS sidecar on loopback. Packaged builds copy the bundled manager to the writable app-local data directory before creating the Python venv/runtime.
- SearXNG and the reader run through Docker for local web search. Search is optional on Windows beta and must not block core tutor readiness.
- SQLite stores conversations, learner profile, study sessions, attempts, mastery, and Study Pack context.

## Cross-Platform Target

macOS and Windows are first-class targets. Feature parity is required for:

- chat
- TTS
- guided study
- image context
- screenshots
- OCR
- selected text where the OS allows it
- local learner memory
- Study Pack save/retrieve/check flows

macOS implementations may use existing Accessibility, CoreGraphics, Vision, and NSPanel code. Windows implementations need equivalent platform modules rather than macOS stubs.

Windows beta defaults:

- installer: private unsigned NSIS
- activator: `Ctrl+Space`
- OCR: local Ollama vision model `gemma4:e2b`
- screen capture: Win32/GDI virtual desktop capture with the Study Buddy window hidden during capture

## Study Pack Storage And Retrieval

Study Packs use local SQLite tables for packs, saved OCR items, chunks, and an FTS5 search index. Saved screenshot images are copied into the app data directory under `study-context-images/<pack_id>/` so new captures survive normal app reinstalls alongside the SQLite database.

Retrieval uses deterministic lexical scoring plus SQLite FTS ranking over saved chunks, source labels, and tags. Retrieved chunks are injected into normal chat prompts only when relevant, and `/check` uses a dedicated prompt containing saved context, current screenshot OCR, and the student's question.

Answer checking is evidence-gated: the prompt instructs the model to compare only against saved Study Pack evidence, cite source IDs, and say the saved pack is insufficient when no source directly supports a verdict. Local embedding search is optional future work, not required for the current reliability path.
