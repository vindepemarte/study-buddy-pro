# User Flows

Status: decided

## First Launch

Study Buddy Pro starts with guided setup:

1. Check OS and app permissions.
2. Check Ollama.
3. Install or guide setup for a vision-capable model; Windows beta requires `gemma4:e2b` for OCR.
4. Check/start Supertonic TTS from the writable app-local runtime folder.
5. Check Docker search services as optional.
6. Verify a short spoken phrase and a local AI response.

Setup must explain what it is about to install or download before doing it.

## Study Entry

Students can enter guided learning through:

- natural language, such as "I can't understand this subject"
- screenshots or pasted images
- highlighted text
- `/study`, `/quiz`, and `/vocab`

Natural-language study routing is primary; commands are explicit shortcuts.

## Context Capture

Students can keep one conversation open and switch the active Study Pack from the ask surface. They can save the current page with the Remember control or type `/screen /remember`, and can save attached images with `/remember`.

Saved pages are OCRed locally, optionally enriched with MLX Vision structured notes on Apple Silicon, copied into the app data directory when an image path is available, chunked into retrievable context, indexed, and stored in the selected Study Pack. Optional notes in the `/remember` message become the saved page title when present.

When MLX Vision is available but not installed, the Study Pack surface offers an explicit enable action. OCR-only saving remains available if the MLX runtime is absent, installing, or fails.

Existing saved pages with older chunking are automatically re-indexed in the background when their Study Pack becomes active. The ask surface shows indexed-page progress so the student can tell when the pack is ready for grounded checks.

Students may attach multiple screenshots to one `/remember` message to save several manual pages at once. PDF/manual import is planned as a later bulk-ingestion surface; until then, photographed or screenshot pages are the supported local source format.

## Answer Checking

For quizzes and practice tests, students use `/check` with text, an attached screenshot, or `/screen /check`. Study Buddy Pro retrieves relevant indexed chunks from the active Study Pack, combines them with the current screenshot OCR, and explains whether the student's answer is correct in small learning steps.

If the saved Study Pack does not contain enough evidence, the tutor must tell the student what page or rule needs to be saved next instead of guessing from outside knowledge.

## Speaking

Study Buddy Pro auto-speaks in Study Mode only:

- lesson steps
- questions
- feedback
- short recaps

Normal chat shows a speaker button and does not auto-speak every response.
