# Data Model

Status: decided

Study Buddy Pro keeps learner data local in SQLite.

Required entities:

- learner profile
- conversations
- messages
- study sessions
- learning events
- vocabulary terms
- vocabulary definitions
- vocabulary attempts
- quiz attempts
- mastery state
- study packs
- study context items
- study context chunks
- study pack conversation links

Learning events record what the tutor observed: confusion, correct answers, repeated mistakes, topic changes, and mastery progress. These events should make future sessions less generic.

Study Packs store named local context for modules or subjects. Study context items store the original OCR, optional MLX Vision structured notes, source kind, source role, durable image paths when available, summary, tags, indexing status/error, indexed timestamp, and optional conversation id.

Study context chunks store retrievable text spans with stable source labels for citation in grounded corrections. The `study_context_fts` virtual table stores chunk/source/tag text for local full-text retrieval and can be rebuilt from persisted items at any time.

Normal app reinstall should preserve Study Pack SQLite rows and durable study-context images because they live in app data. On launch, Study Buddy Pro backfills older remembered image paths by copying any still-existing legacy screenshot files into `study-context-images/<pack_id>/` and updating the stored paths. If an older temp screenshot file is already gone before backfill runs, its OCR text remains in SQLite and can still be re-indexed.
