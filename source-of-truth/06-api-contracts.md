# API Contracts

Status: decided

## Voice

Backend commands:

- `voice_health`
- `voice_start`
- `voice_stop`
- `voice_styles`
- `speak_text`
- `stop_speech`

Voice responses must report whether Supertonic is reachable, which URL is used, available voices when known, and a clear failure message when offline.

## Study

Backend commands:

- create or resume a study session
- record quiz and vocabulary attempts
- update mastery state
- return local learner summary

Study commands should remain local and additive to existing conversation history.

## Setup

Backend setup status should check:

- OS
- Ollama reachability
- active/installed model
- Windows OCR model `gemma4:e2b` when running on Windows
- Python availability for the Supertonic manager
- writable Supertonic runtime availability
- Supertonic health
- Docker availability
- search services health

`ready` means core tutor readiness. Docker/search readiness is reported separately and remains optional for Windows beta.
