# Tech Architecture

Status: decided

The app is a Tauri v2 desktop app with a React/TypeScript frontend and Rust backend.

## Local Services

- Ollama runs the optional local LLM route.
- OpenRouter is the API-first inference provider when configured in Settings. It can handle chat, `/search` planner/judge/synthesis calls, direct screenshot/image turns through a vision-capable model, embeddings, OpenRouter STT model selection, and OpenRouter TTS playback while local storage remains the source of memory truth.
- MLX-VLM is the optional Apple Silicon vision-understanding runtime for Study Pack indexing and for enriching direct screenshot/image turns when the active local chat model is text-only.
- Supertonic runs as a bundled local HTTP TTS sidecar on loopback. Packaged builds copy the bundled manager to the writable app-local data directory before creating the Python venv/runtime.
- SearXNG and the reader run through Docker for local web retrieval. `/search` uses the selected inference provider for LLM planning/judging/synthesis, while SearXNG and the reader remain local optional services and must not block core tutor readiness.
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

Study Packs use local SQLite tables for packs, saved OCR items, structured MLX Vision notes, chunks, semantic embeddings, and an FTS5 search index. Saved screenshot images are copied into the app data directory under `study-context-images/<pack_id>/` so new captures survive normal app reinstalls alongside the SQLite database.

On macOS Apple Silicon, `/remember` keeps Apple Vision OCR as the exact-text path and can add MLX-VLM structured page notes when the app-local MLX runtime is installed. Plain screenshot/image submits to a text-only Ollama model use the same local image-to-text fallback: OCR first, optional MLX-VLM notes second, then a text-only prompt to the chat model with the original thumbnail kept only for the UI. The default model target is `mlx-community/Qwen3-VL-8B-Instruct-4bit`; setup must let pip resolve compatible MLX package versions for the current Python environment, then persist/probe runtime status instead of hardcoding a machine-specific MLX version.

Retrieval uses hybrid scoring: deterministic lexical scoring, SQLite FTS ranking over saved OCR chunks/structured notes/source labels/tags, and local cosine search over Study Pack embeddings when an OpenRouter embedding model has indexed the pack. Retrieved chunks are injected into normal chat prompts only when relevant, and `/check` uses a dedicated prompt containing saved context, current screenshot OCR, and the student's question.

Answer checking is evidence-gated: the prompt instructs the model to compare only against saved Study Pack evidence, cite source IDs, and say the saved pack is insufficient when no source directly supports a verdict.
