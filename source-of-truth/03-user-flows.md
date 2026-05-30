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

## Speaking

Study Buddy Pro auto-speaks in Study Mode only:

- lesson steps
- questions
- feedback
- short recaps

Normal chat shows a speaker button and does not auto-speak every response.
