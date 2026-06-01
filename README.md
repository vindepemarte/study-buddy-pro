# Study Buddy Pro

Study Buddy Pro is a local-first desktop tutor forked from Thuki. It keeps the floating overlay, screenshot context, optional local Ollama chat, and local search pipeline, then adds a study-buddy direction: guided explanations, adaptive quizzes, vocabulary mastery, learner memory, OpenRouter API routing, semantic Study Pack search, and selectable local/API text-to-speech.

The goal is not to complete homework for a student. The app should help the student understand the material, identify gaps, explain difficult words, ask for original examples, quiz one step at a time, and adapt from mistakes.

## What Is Implemented

- OpenRouter chat provider with Settings-based API key, model catalog, capability filters, and rough input/output pricing summaries.
- Optional local Ollama chat with Thuki's existing floating overlay workflow.
- Screenshot and image context through the existing capture/OCR paths.
- Study commands: `/study`, `/quiz`, and `/vocab`.
- Natural study routing for phrases like "I can't understand this".
- Local learner tables in SQLite for study sessions, learning events, vocabulary attempts, quiz attempts, and mastery state.
- Supertonic voice settings and Tauri commands for health, style listing, speech playback, stop, start, and stop.
- OpenRouter TTS playback through the selected speech model and voice, plus an OpenRouter STT command that uses the selected transcription model.
- Startup voice launch attempt when voice is enabled. Packaged builds copy the bundled Supertonic manager into the app-local data directory before starting it.
- Windows beta support for `Ctrl+Space`, screen capture, Supertonic startup, and OCR through local Ollama vision model `gemma4:e2b`.
- Optional Apple Silicon MLX Vision enrichment for Study Packs. `/remember` can combine Apple Vision OCR with local MLX-VLM structured page notes before indexing saved screenshots.
- Local Study Pack semantic embeddings. Saved screenshot chunks stay in SQLite; when OpenRouter is configured, embeddings are generated through the selected embedding model and stored locally for hybrid FTS + vector retrieval.
- Setup readiness command for Ollama, Windows OCR model, Python, optional MLX Vision, voice, Docker, SearXNG, and reader status. Docker search is optional.

## Local Runtime

Required for API-first chat:

1. Open Settings.
2. Set `Inference provider` to `openrouter`.
3. Add your OpenRouter API key.
4. Click `Refresh models` and choose chat, vision, embedding, speech-to-text, and text-to-speech models.

For API speech, set `Voice provider` to `openrouter` in Settings and choose an `OpenRouter voice` supported by the selected TTS model. Local Supertonic remains the default/offline voice provider.

Optional for fully local chat:

```bash
ollama serve
ollama pull gemma4:e4b
```

Required for Windows OCR:

```bash
ollama pull gemma4:e2b
```

Required for voice in development:

```bash
export STUDY_BUDDY_SUPERTONIC_DIR=/Users/vdpm/Documents/codex-projects/supertonic
```

Packaged builds include `src-tauri/resources/supertonic/native-server`. The app copies that runtime into `$APPLOCALDATA/supertonic` on first launch and calls `native-server/manage.py start --no-wait`. First run may create `.native-venv`, install Supertonic dependencies, and download the model. Python 3.11 or 3.12 must be available.

Required for local search:

```bash
bun run search-box:start
```

Packaged builds also include the search-box resources. Docker Desktop remains optional; `/search` becomes available after the Docker services are started.

Optional for richer Study Pack screenshot indexing on Apple Silicon:

```bash
python3.12 --version
# or
python3.11 --version
```

In the app, select a Study Pack and click `Enable MLX`. Study Buddy Pro creates an app-local MLX-VLM venv, installs compatible packages without pinning exact MLX versions, downloads `mlx-community/Qwen3-VL-8B-Instruct-4bit`, and then uses it automatically on future `/remember` saves. OCR-only saving still works if MLX Vision is not installed.

## Development

```bash
bun install
bun run dev
```

Useful checks:

```bash
bun run typecheck
bun run test
cd src-tauri && cargo test
```

## Source Of Truth

Project decisions live in `source-of-truth/`. Update those docs when product scope, runtime assumptions, or user-facing behavior changes.
