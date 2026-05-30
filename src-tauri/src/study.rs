use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::history::Database;

#[derive(Serialize)]
pub struct StudySessionResponse {
    pub session_id: String,
}

#[derive(Serialize)]
pub struct LearnerSummary {
    pub study_sessions: i64,
    pub learning_events: i64,
    pub vocabulary_attempts: i64,
    pub quiz_attempts: i64,
}

#[derive(Deserialize)]
pub struct LearningEventPayload {
    pub session_id: Option<String>,
    pub kind: String,
    pub payload: serde_json::Value,
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_millis() as i64
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn create_study_session(
    conversation_id: Option<String>,
    subject: Option<String>,
    source: Option<String>,
    db: State<'_, Database>,
) -> Result<StudySessionResponse, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_millis();
    conn.execute(
        "INSERT INTO study_sessions \
         (id, conversation_id, subject, source, status, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?6)",
        params![id, conversation_id, subject, source, now, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(StudySessionResponse { session_id: id })
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn record_learning_event(
    event: LearningEventPayload,
    db: State<'_, Database>,
) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_millis();
    let payload = serde_json::to_string(&event.payload).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO learning_events (id, session_id, kind, payload, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, event.session_id, event.kind, payload, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn record_vocabulary_attempt(
    definition_id: Option<String>,
    session_id: Option<String>,
    sentence: String,
    correct: bool,
    feedback: Option<String>,
    db: State<'_, Database>,
) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO vocabulary_attempts \
         (id, definition_id, session_id, sentence, correct, feedback, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            definition_id,
            session_id,
            sentence,
            if correct { 1 } else { 0 },
            feedback,
            now_millis()
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn record_quiz_attempt(
    session_id: Option<String>,
    question: String,
    answer: String,
    correct: bool,
    feedback: Option<String>,
    db: State<'_, Database>,
) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO quiz_attempts \
         (id, session_id, question, answer, correct, feedback, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            session_id,
            question,
            answer,
            if correct { 1 } else { 0 },
            feedback,
            now_millis()
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn get_learner_summary(db: State<'_, Database>) -> Result<LearnerSummary, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let count = |table: &str| -> Result<i64, String> {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .map_err(|e| e.to_string())
    };
    Ok(LearnerSummary {
        study_sessions: count("study_sessions")?,
        learning_events: count("learning_events")?,
        vocabulary_attempts: count("vocabulary_attempts")?,
        quiz_attempts: count("quiz_attempts")?,
    })
}
