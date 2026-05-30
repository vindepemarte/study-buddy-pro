# Tech Architecture

Status: decided

The app is a Tauri v2 desktop app with a React/TypeScript frontend and Rust backend.

## Local Services

- Ollama runs the local LLM.
- Supertonic runs as a bundled local HTTP TTS sidecar on loopback. Packaged builds copy the bundled manager to the writable app-local data directory before creating the Python venv/runtime.
- SearXNG and the reader run through Docker for local web search. Search is optional on Windows beta and must not block core tutor readiness.
- SQLite stores conversations, learner profile, study sessions, attempts, and mastery.

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

macOS implementations may use existing Accessibility, CoreGraphics, Vision, and NSPanel code. Windows implementations need equivalent platform modules rather than macOS stubs.

Windows beta defaults:

- installer: private unsigned NSIS
- activator: `Ctrl+Space`
- OCR: local Ollama vision model `gemma4:e2b`
- screen capture: Win32/GDI virtual desktop capture with the Study Buddy window hidden during capture
