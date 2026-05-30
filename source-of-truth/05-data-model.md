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

Learning events record what the tutor observed: confusion, correct answers, repeated mistakes, topic changes, and mastery progress. These events should make future sessions less generic.

