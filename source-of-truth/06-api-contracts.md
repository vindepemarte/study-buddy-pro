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

## MLX Vision

Backend commands:

- `mlx_vlm_status`
- `mlx_vlm_install`
- `mlx_vlm_describe_images`

MLX Vision commands are Apple Silicon-only and optional. `mlx_vlm_install` creates an app-local Python venv, installs `mlx-vlm` and `huggingface_hub` without pinning an exact MLX version, downloads the selected model, and returns probed runtime status. `mlx_vlm_describe_images` must return structured page notes for Study Pack indexing and must fail visibly without blocking OCR-only saves.

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

Study commands should remain local and additive to existing conversation history. Study Pack commands never upload screenshots or OCR outside the local app. `save_context_from_images` must persist OCR text, optional MLX Vision structured notes, and copy available source images into app data before indexing. `backfill_study_pack_image_paths` must be safe to run repeatedly and copy still-existing legacy image paths into durable app data without losing missing-path OCR records. `rebuild_study_pack_index` must be safe to run repeatedly and rebuild chunks/search rows from stored OCR plus structured notes. `check_answer_from_context` must return a model prompt that includes retrieved saved sources and tells the model to cite source IDs or admit missing context.

## Setup

Backend setup status should check:

- OS
- Ollama reachability
- active/installed model
- Windows OCR model `gemma4:e2b` when running on Windows
- MLX Vision runtime status when running on Apple Silicon macOS
- Python availability for the Supertonic manager
- writable Supertonic runtime availability
- Supertonic health
- Docker availability
- search services health

`ready` means core tutor readiness. Docker/search readiness is reported separately and remains optional for Windows beta.
