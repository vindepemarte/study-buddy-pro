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

- `list_study_packs`
- `create_study_pack`
- `get_active_study_pack`
- `set_active_study_pack`
- `save_context_from_images`
- `backfill_study_pack_image_paths`
- `retrieve_study_context`
- `rebuild_study_pack_index`
- `check_answer_from_context`
- `get_study_pack_summary`
- create or resume a study session
- record quiz and vocabulary attempts
- update mastery state
- return local learner summary

Study commands should remain local and additive to existing conversation history. Study Pack commands never upload screenshots or OCR outside the local app. `save_context_from_images` must persist OCR text and copy available source images into app data before indexing. `backfill_study_pack_image_paths` must be safe to run repeatedly and copy still-existing legacy image paths into durable app data without losing missing-path OCR records. `rebuild_study_pack_index` must be safe to run repeatedly and rebuild chunks/search rows from stored OCR. `check_answer_from_context` must return a model prompt that includes retrieved saved sources and tells the model to cite source IDs or admit missing context.

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
