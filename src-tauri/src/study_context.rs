use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

use crate::history::Database;

const ACTIVE_STUDY_PACK_KEY: &str = "active_study_pack_id";
const MAX_CHUNK_CHARS: usize = 900;
const CHUNK_OVERLAP_LINES: usize = 2;
const RETRIEVAL_LIMIT_DEFAULT: usize = 10;
const STUDY_CONTEXT_IMAGE_DIR: &str = "study-context-images";

#[derive(Clone, Serialize)]
pub struct StudyPackSummary {
    pub id: String,
    pub name: String,
    pub authority_source: Option<String>,
    pub description: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub item_count: i64,
    pub indexed_count: i64,
    pub needs_index_count: i64,
    pub active: bool,
}

#[derive(Clone, Serialize)]
pub struct StudyPackSummaryResponse {
    pub pack: StudyPackSummary,
}

#[derive(Clone, Serialize)]
pub struct StudyPackOverview {
    pub pack: Option<StudyPackSummary>,
    pub item_count: i64,
    pub chunk_count: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveContextRequest {
    pub pack_id: String,
    pub image_paths: Vec<String>,
    pub ocr_text: String,
    pub note: Option<String>,
    pub conversation_id: Option<String>,
    pub source_kind: Option<String>,
}

#[derive(Serialize)]
pub struct SaveContextResponse {
    pub item_id: String,
    pub chunks_saved: usize,
    pub title: String,
    pub image_paths: Vec<String>,
}

#[derive(Clone, Serialize)]
pub struct RetrievedContextChunk {
    pub id: String,
    pub item_id: String,
    pub source_id: String,
    pub source_label: String,
    pub chunk_text: String,
    pub score: f64,
}

#[derive(Clone, Serialize)]
pub struct RetrieveStudyContextResponse {
    pub pack: Option<StudyPackSummary>,
    pub chunks: Vec<RetrievedContextChunk>,
    pub context_block: String,
}

#[derive(Serialize)]
pub struct ContextPromptResponse {
    pub prompt: String,
    pub context: RetrieveStudyContextResponse,
    pub enough_context: bool,
}

#[derive(Serialize)]
pub struct StudyPackIndexResponse {
    pub pack_id: String,
    pub total_items: usize,
    pub indexed_items: usize,
    pub chunks_saved: usize,
}

#[derive(Default, Serialize)]
pub struct StudyPackImageBackfillResponse {
    pub items_scanned: usize,
    pub items_updated: usize,
    pub images_copied: usize,
    pub missing_images: usize,
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_millis() as i64
}

fn trim_to_chars(input: &str, max_chars: usize) -> String {
    let trimmed = input.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(max_chars).collect();
    out.push_str("...");
    out
}

fn title_from(note: Option<&str>, ocr_text: &str) -> String {
    if let Some(note) = note.map(str::trim).filter(|s| !s.is_empty()) {
        return trim_to_chars(note, 80);
    }
    ocr_text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && *line != "[No text detected]")
        .map(|line| trim_to_chars(line, 80))
        .unwrap_or_else(|| "Saved study page".to_string())
}

fn summary_from(ocr_text: &str) -> String {
    trim_to_chars(
        &ocr_text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .take(8)
            .collect::<Vec<_>>()
            .join(" "),
        320,
    )
}

fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            current.extend(ch.to_lowercase());
        } else if current.len() >= 2 {
            tokens.push(std::mem::take(&mut current));
        } else {
            current.clear();
        }
    }
    if current.len() >= 2 {
        tokens.push(current);
    }
    tokens
}

fn is_stopword(token: &str) -> bool {
    matches!(
        token,
        "the"
            | "and"
            | "or"
            | "for"
            | "with"
            | "that"
            | "this"
            | "what"
            | "when"
            | "where"
            | "which"
            | "who"
            | "why"
            | "how"
            | "does"
            | "is"
            | "are"
            | "was"
            | "were"
            | "can"
            | "should"
            | "would"
            | "answer"
            | "correct"
            | "wrong"
            | "check"
            | "il"
            | "lo"
            | "la"
            | "gli"
            | "le"
            | "un"
            | "una"
            | "di"
            | "del"
            | "dello"
            | "della"
            | "dei"
            | "degli"
            | "delle"
            | "a"
            | "ad"
            | "al"
            | "allo"
            | "alla"
            | "ai"
            | "agli"
            | "alle"
            | "da"
            | "dal"
            | "dallo"
            | "dalla"
            | "dai"
            | "dagli"
            | "dalle"
            | "in"
            | "nel"
            | "nello"
            | "nella"
            | "nei"
            | "negli"
            | "nelle"
            | "con"
            | "su"
            | "per"
            | "tra"
            | "fra"
            | "e"
            | "o"
            | "che"
            | "non"
            | "si"
            | "sono"
            | "sei"
            | "sia"
            | "deve"
            | "devono"
            | "vero"
            | "falso"
            | "risposta"
            | "corretta"
            | "errata"
    )
}

fn search_tokens(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for token in tokenize(text) {
        if token.chars().count() < 3 || is_stopword(&token) || seen.contains(&token) {
            continue;
        }
        seen.insert(token.clone());
        out.push(token);
        if out.len() >= 48 {
            break;
        }
    }
    out
}

fn tags_from(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut tags = Vec::new();
    for token in search_tokens(text) {
        if token.chars().count() < 4 || seen.contains(&token) {
            continue;
        }
        seen.insert(token.clone());
        tags.push(token);
        if tags.len() >= 12 {
            break;
        }
    }
    tags
}

fn chunk_text(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let lines = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();

    let mut idx = 0;
    while idx < lines.len() {
        let start = idx;
        let mut current = String::new();
        while idx < lines.len() {
            let line = lines[idx];
            let addition = if current.is_empty() {
                line.len()
            } else {
                line.len() + 1
            };
            if !current.is_empty() && current.len() + addition > MAX_CHUNK_CHARS {
                break;
            }
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
            idx += 1;
        }
        if !current.trim().is_empty() {
            chunks.push(current.trim().to_string());
        }
        if idx >= lines.len() {
            break;
        }
        let next = idx.saturating_sub(CHUNK_OVERLAP_LINES);
        idx = if next <= start { start + 1 } else { next };
    }
    if chunks.is_empty() && !text.trim().is_empty() {
        chunks.push(trim_to_chars(text, MAX_CHUNK_CHARS));
    }
    chunks
}

fn get_active_pack_id(conn: &rusqlite::Connection) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM app_config WHERE key = ?1",
        params![ACTIVE_STUDY_PACK_KEY],
        |row| row.get::<_, String>(0),
    )
    .optional()
}

fn set_active_pack_id(conn: &rusqlite::Connection, pack_id: Option<&str>) -> rusqlite::Result<()> {
    match pack_id {
        Some(pack_id) => {
            conn.execute(
                "INSERT INTO app_config (key, value) VALUES (?1, ?2) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![ACTIVE_STUDY_PACK_KEY, pack_id],
            )?;
        }
        None => {
            conn.execute(
                "DELETE FROM app_config WHERE key = ?1",
                params![ACTIVE_STUDY_PACK_KEY],
            )?;
        }
    }
    Ok(())
}

fn pack_exists(conn: &rusqlite::Connection, pack_id: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM study_packs WHERE id = ?1)",
        params![pack_id],
        |row| row.get::<_, i64>(0),
    )
    .map(|value| value == 1)
}

fn load_pack(
    conn: &rusqlite::Connection,
    pack_id: &str,
    active_id: Option<&str>,
) -> rusqlite::Result<Option<StudyPackSummary>> {
    conn.query_row(
        "SELECT p.id, p.name, p.authority_source, p.description, p.created_at, p.updated_at,
         (SELECT COUNT(*) FROM study_context_items i WHERE i.pack_id = p.id) AS item_count,
         (SELECT COUNT(*) FROM study_context_items i WHERE i.pack_id = p.id AND i.indexed_at IS NOT NULL) AS indexed_count \
         FROM study_packs p \
         WHERE p.id = ?1 \
         GROUP BY p.id",
        params![pack_id],
        |row| {
            let id: String = row.get(0)?;
            let item_count = row.get::<_, i64>(6)?;
            let indexed_count = row.get::<_, i64>(7)?;
            Ok(StudyPackSummary {
                active: active_id == Some(id.as_str()),
                id,
                name: row.get(1)?,
                authority_source: row.get(2)?,
                description: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
                item_count,
                indexed_count,
                needs_index_count: (item_count - indexed_count).max(0),
            })
        },
    )
    .optional()
}

fn active_or_requested_pack_id(
    conn: &rusqlite::Connection,
    pack_id: Option<&str>,
) -> rusqlite::Result<Option<String>> {
    match pack_id {
        Some(id) if !id.trim().is_empty() => Ok(Some(id.to_string())),
        _ => get_active_pack_id(conn),
    }
}

fn study_context_images_root(base_dir: &Path, pack_id: &str) -> PathBuf {
    base_dir.join(STUDY_CONTEXT_IMAGE_DIR).join(pack_id)
}

fn copy_into_study_context_images(base_dir: &Path, pack_id: &str, source: &Path) -> Option<String> {
    let root = study_context_images_root(base_dir, pack_id);
    if std::fs::create_dir_all(&root).is_err() {
        return None;
    }
    let extension = source
        .extension()
        .and_then(|ext| ext.to_str())
        .filter(|ext| !ext.trim().is_empty())
        .unwrap_or("jpg");
    let target = root.join(format!("{}.{}", uuid::Uuid::new_v4(), extension));
    std::fs::copy(source, &target).ok()?;
    target.to_str().map(ToString::to_string)
}

fn durable_image_paths_in_base(
    base_dir: &Path,
    pack_id: &str,
    image_paths: &[String],
) -> Vec<String> {
    let root = study_context_images_root(base_dir, pack_id);
    image_paths
        .iter()
        .filter_map(|path| {
            let source = Path::new(path);
            if source.starts_with(&root) {
                return Some(path.clone());
            }
            if !source.exists() {
                return None;
            }
            copy_into_study_context_images(base_dir, pack_id, source)
        })
        .collect()
}

fn durable_image_paths(
    app_handle: &tauri::AppHandle,
    pack_id: &str,
    image_paths: &[String],
) -> Vec<String> {
    let Ok(base_dir) = app_handle.path().app_data_dir() else {
        return image_paths.to_vec();
    };
    durable_image_paths_in_base(&base_dir, pack_id, image_paths)
}

fn backfill_study_context_image_paths_in_base(
    conn: &rusqlite::Connection,
    base_dir: &Path,
) -> Result<StudyPackImageBackfillResponse, String> {
    let mut response = StudyPackImageBackfillResponse::default();
    let mut stmt = conn
        .prepare(
            "SELECT id, pack_id, image_paths \
             FROM study_context_items \
             WHERE image_paths IS NOT NULL AND TRIM(image_paths) <> ''",
        )
        .map_err(|e| e.to_string())?;
    let items = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    drop(stmt);

    for (item_id, pack_id, image_paths_json) in items {
        let Ok(paths) = serde_json::from_str::<Vec<String>>(&image_paths_json) else {
            continue;
        };
        response.items_scanned += 1;
        let root = study_context_images_root(base_dir, &pack_id);
        let mut changed = false;
        let mut durable_paths = Vec::with_capacity(paths.len());

        for path in paths {
            let source = Path::new(&path);
            if source.starts_with(&root) {
                durable_paths.push(path);
                continue;
            }
            if !source.exists() {
                response.missing_images += 1;
                durable_paths.push(path);
                continue;
            }
            if let Some(copied_path) = copy_into_study_context_images(base_dir, &pack_id, source) {
                response.images_copied += 1;
                durable_paths.push(copied_path);
                changed = true;
            } else {
                durable_paths.push(path);
            }
        }

        if changed {
            let updated_json = serde_json::to_string(&durable_paths).map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE study_context_items SET image_paths = ?1 WHERE id = ?2",
                params![updated_json, item_id],
            )
            .map_err(|e| e.to_string())?;
            response.items_updated += 1;
        }
    }

    Ok(response)
}

fn fts_available(conn: &rusqlite::Connection) -> bool {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'study_context_fts')",
        [],
        |row| row.get::<_, i64>(0),
    )
    .map(|value| value == 1)
    .unwrap_or(false)
}

fn fts_query_for(text: &str) -> Option<String> {
    let tokens = search_tokens(text);
    if tokens.is_empty() {
        return None;
    }
    Some(
        tokens
            .into_iter()
            .take(32)
            .map(|token| format!("{token}*"))
            .collect::<Vec<_>>()
            .join(" OR "),
    )
}

fn delete_pack_fts(conn: &rusqlite::Connection, pack_id: &str) {
    if fts_available(conn) {
        let _ = conn.execute(
            "DELETE FROM study_context_fts WHERE pack_id = ?1",
            params![pack_id],
        );
    }
}

fn insert_chunk_fts(
    conn: &rusqlite::Connection,
    chunk_id: &str,
    item_id: &str,
    pack_id: &str,
    source_label: &str,
    chunk_text: &str,
    tags: &str,
) {
    if fts_available(conn) {
        let _ = conn.execute(
            "INSERT INTO study_context_fts \
             (chunk_id, item_id, pack_id, source_label, chunk_text, tags) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![chunk_id, item_id, pack_id, source_label, chunk_text, tags],
        );
    }
}

fn index_item_chunks(
    conn: &rusqlite::Connection,
    item_id: &str,
    pack_id: &str,
    title: &str,
    raw_ocr: &str,
    tags: &str,
    now: i64,
) -> rusqlite::Result<usize> {
    let chunks = chunk_text(raw_ocr);
    for (idx, chunk) in chunks.iter().enumerate() {
        let chunk_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO study_context_chunks \
             (id, item_id, pack_id, chunk_index, chunk_text, source_label, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![chunk_id, item_id, pack_id, idx as i64, chunk, title, now],
        )?;
        insert_chunk_fts(conn, &chunk_id, item_id, pack_id, title, chunk, tags);
    }
    conn.execute(
        "UPDATE study_context_items \
         SET index_status = 'ready', index_error = NULL, indexed_at = ?1 \
         WHERE id = ?2",
        params![now, item_id],
    )?;
    Ok(chunks.len())
}

fn score_chunk(query_tokens: &[String], query_lower: &str, text: &str, label: &str) -> f64 {
    let haystack = format!("{label}\n{text}").to_lowercase();
    let hay_tokens: HashSet<String> = tokenize(&haystack).into_iter().collect();
    let mut score = 0.0;
    let mut matches = 0.0;
    for token in query_tokens {
        if hay_tokens.contains(token) {
            matches += 1.0;
        }
    }
    if !query_tokens.is_empty() {
        score += matches;
        score += (matches / query_tokens.len() as f64) * 12.0;
    }
    if !query_lower.is_empty() && haystack.contains(query_lower) {
        score += 16.0;
    }
    score
}

pub fn retrieve_context_for_prompt(
    conn: &rusqlite::Connection,
    pack_id: Option<&str>,
    query: &str,
    limit: usize,
) -> rusqlite::Result<RetrieveStudyContextResponse> {
    let active_id = get_active_pack_id(conn)?;
    let Some(target_pack_id) = active_or_requested_pack_id(conn, pack_id)? else {
        return Ok(RetrieveStudyContextResponse {
            pack: None,
            chunks: Vec::new(),
            context_block: String::new(),
        });
    };
    let Some(pack) = load_pack(conn, &target_pack_id, active_id.as_deref())? else {
        return Ok(RetrieveStudyContextResponse {
            pack: None,
            chunks: Vec::new(),
            context_block: String::new(),
        });
    };

    let query_lower = query.trim().to_lowercase();
    let query_tokens = search_tokens(query);
    if query_tokens.is_empty() {
        return Ok(RetrieveStudyContextResponse {
            pack: Some(pack),
            chunks: Vec::new(),
            context_block: String::new(),
        });
    }

    let mut fts_boosts: HashMap<String, f64> = HashMap::new();
    if let Some(fts_query) = fts_query_for(query) {
        if fts_available(conn) {
            if let Ok(mut stmt) = conn.prepare(
                "SELECT chunk_id, bm25(study_context_fts) AS rank \
                 FROM study_context_fts \
                 WHERE pack_id = ?1 AND study_context_fts MATCH ?2 \
                 ORDER BY rank ASC \
                 LIMIT ?3",
            ) {
                if let Ok(rows) = stmt.query_map(
                    params![&target_pack_id, fts_query, (limit.max(1) * 4) as i64],
                    |row| {
                        let chunk_id: String = row.get(0)?;
                        let rank: f64 = row.get(1)?;
                        Ok((chunk_id, rank))
                    },
                ) {
                    for (idx, row) in rows.flatten().enumerate() {
                        let rank_bonus = 24.0 / (idx + 1) as f64;
                        let bm25_bonus = if row.1.is_finite() {
                            1.0 / (1.0 + row.1.abs())
                        } else {
                            0.0
                        };
                        fts_boosts.insert(row.0, rank_bonus + bm25_bonus);
                    }
                }
            }
        }
    }

    let mut stmt = conn.prepare(
        "SELECT c.id, c.item_id, c.source_label, c.chunk_text \
         FROM study_context_chunks c \
         WHERE c.pack_id = ?1 \
         ORDER BY c.created_at DESC",
    )?;
    let mut scored = stmt
        .query_map(params![&target_pack_id], |row| {
            let id: String = row.get(0)?;
            let item_id: String = row.get(1)?;
            let source_label: String = row.get(2)?;
            let chunk_text: String = row.get(3)?;
            let mut score = score_chunk(&query_tokens, &query_lower, &chunk_text, &source_label);
            if let Some(boost) = fts_boosts.get(&id) {
                score += boost;
            }
            Ok(RetrievedContextChunk {
                id,
                item_id,
                source_id: String::new(),
                source_label,
                chunk_text,
                score,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    scored.retain(|chunk| chunk.score > 0.0 || fts_boosts.contains_key(&chunk.id));
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(limit.max(1));
    for (idx, chunk) in scored.iter_mut().enumerate() {
        chunk.source_id = format!("SP{}", idx + 1);
    }
    let context_block = context_block(pack.clone(), &scored);
    Ok(RetrieveStudyContextResponse {
        pack: Some(pack),
        chunks: scored,
        context_block,
    })
}

pub fn context_block(pack: StudyPackSummary, chunks: &[RetrievedContextChunk]) -> String {
    if chunks.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str("[Saved Study Context]\n");
    out.push_str(&format!("Study Pack: {}\n", pack.name));
    if let Some(authority) = pack.authority_source.as_deref() {
        if !authority.trim().is_empty() {
            out.push_str(&format!("Authority source: {authority}\n"));
        }
    }
    out.push_str("Use these saved notes as evidence, not vague background. Cite source IDs when correcting the student. If no saved source directly supports the verdict, say the context is insufficient instead of guessing.\n\n");
    for chunk in chunks {
        out.push_str(&format!(
            "[{}] {}\n{}\n\n",
            chunk.source_id, chunk.source_label, chunk.chunk_text
        ));
    }
    out.trim_end().to_string()
}

pub fn inject_context_into_prompt(
    conn: &rusqlite::Connection,
    pack_id: Option<&str>,
    content: &str,
) -> rusqlite::Result<String> {
    if content.contains("[Saved Study Context]") {
        return Ok(content.to_string());
    }
    let retrieved = retrieve_context_for_prompt(conn, pack_id, content, RETRIEVAL_LIMIT_DEFAULT)?;
    if retrieved.context_block.trim().is_empty() {
        return Ok(content.to_string());
    }
    Ok(format!(
        "{}\n\n[Student Request]\n{}",
        retrieved.context_block, content
    ))
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn list_study_packs(db: State<'_, Database>) -> Result<Vec<StudyPackSummary>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let active_id = get_active_pack_id(&conn).map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.name, p.authority_source, p.description, p.created_at, p.updated_at,
             (SELECT COUNT(*) FROM study_context_items i WHERE i.pack_id = p.id) AS item_count,
             (SELECT COUNT(*) FROM study_context_items i WHERE i.pack_id = p.id AND i.indexed_at IS NOT NULL) AS indexed_count \
             FROM study_packs p \
             GROUP BY p.id \
             ORDER BY p.updated_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let packs = stmt
        .query_map([], |row| {
            let id: String = row.get(0)?;
            let item_count = row.get::<_, i64>(6)?;
            let indexed_count = row.get::<_, i64>(7)?;
            Ok(StudyPackSummary {
                active: active_id.as_deref() == Some(id.as_str()),
                id,
                name: row.get(1)?,
                authority_source: row.get(2)?,
                description: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
                item_count,
                indexed_count,
                needs_index_count: (item_count - indexed_count).max(0),
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    Ok(packs)
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn create_study_pack(
    name: String,
    authority_source: Option<String>,
    description: Option<String>,
    db: State<'_, Database>,
) -> Result<StudyPackSummaryResponse, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Study Pack name is required.".to_string());
    }
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_millis();
    let authority_source = authority_source
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let description = description
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    conn.execute(
        "INSERT INTO study_packs (id, name, authority_source, description, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, name, authority_source, description, now, now],
    )
    .map_err(|e| e.to_string())?;
    set_active_pack_id(&conn, Some(&id)).map_err(|e| e.to_string())?;
    let pack = load_pack(&conn, &id, Some(&id))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Created Study Pack could not be loaded.".to_string())?;
    Ok(StudyPackSummaryResponse { pack })
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn get_active_study_pack(db: State<'_, Database>) -> Result<Option<StudyPackSummary>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let Some(active_id) = get_active_pack_id(&conn).map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    load_pack(&conn, &active_id, Some(&active_id)).map_err(|e| e.to_string())
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn set_active_study_pack(
    pack_id: Option<String>,
    db: State<'_, Database>,
) -> Result<Option<StudyPackSummary>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let pack_id = pack_id
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(pack_id) = pack_id.as_deref() {
        if !pack_exists(&conn, pack_id).map_err(|e| e.to_string())? {
            return Err("Study Pack not found.".to_string());
        }
        set_active_pack_id(&conn, Some(pack_id)).map_err(|e| e.to_string())?;
        return load_pack(&conn, pack_id, Some(pack_id)).map_err(|e| e.to_string());
    }
    set_active_pack_id(&conn, None).map_err(|e| e.to_string())?;
    Ok(None)
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn save_context_from_images(
    request: SaveContextRequest,
    app_handle: tauri::AppHandle,
    db: State<'_, Database>,
) -> Result<SaveContextResponse, String> {
    let ocr_text = request.ocr_text.trim();
    if ocr_text.is_empty() || ocr_text == "[No text detected]" {
        return Err("No readable text was found to save.".to_string());
    }

    let conn = db.0.lock().map_err(|e| e.to_string())?;
    if !pack_exists(&conn, &request.pack_id).map_err(|e| e.to_string())? {
        return Err("Study Pack not found.".to_string());
    }

    let title = title_from(request.note.as_deref(), ocr_text);
    let summary = summary_from(ocr_text);
    let tags = serde_json::to_string(&tags_from(ocr_text)).map_err(|e| e.to_string())?;
    let durable_paths = durable_image_paths(&app_handle, &request.pack_id, &request.image_paths);
    let image_paths = if durable_paths.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&durable_paths).map_err(|e| e.to_string())?)
    };
    let item_id = uuid::Uuid::new_v4().to_string();
    let now = now_millis();
    let source_kind = request
        .source_kind
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("screenshot");

    conn.execute(
        "INSERT INTO study_context_items \
         (id, pack_id, conversation_id, title, source_kind, image_paths, raw_ocr, summary, tags, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            item_id,
            request.pack_id,
            request.conversation_id,
            title,
            source_kind,
            image_paths,
            ocr_text,
            summary,
            tags,
            now,
        ],
    )
    .map_err(|e| e.to_string())?;

    if let Some(conversation_id) = request
        .conversation_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        conn.execute(
            "INSERT OR IGNORE INTO study_pack_conversations (pack_id, conversation_id, created_at) \
             VALUES (?1, ?2, ?3)",
            params![request.pack_id, conversation_id, now],
        )
        .map_err(|e| e.to_string())?;
    }

    let chunks_saved = index_item_chunks(
        &conn,
        &item_id,
        &request.pack_id,
        &title,
        ocr_text,
        &tags,
        now,
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE study_packs SET updated_at = ?1 WHERE id = ?2",
        params![now, request.pack_id],
    )
    .map_err(|e| e.to_string())?;

    Ok(SaveContextResponse {
        item_id,
        chunks_saved,
        title,
        image_paths: durable_paths,
    })
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn backfill_study_pack_image_paths(
    app_handle: tauri::AppHandle,
    db: State<'_, Database>,
) -> Result<StudyPackImageBackfillResponse, String> {
    let base_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    backfill_study_context_image_paths_in_base(&conn, &base_dir)
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn retrieve_study_context(
    pack_id: Option<String>,
    query: String,
    limit: Option<usize>,
    db: State<'_, Database>,
) -> Result<RetrieveStudyContextResponse, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    retrieve_context_for_prompt(
        &conn,
        pack_id.as_deref(),
        &query,
        limit.unwrap_or(RETRIEVAL_LIMIT_DEFAULT),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn rebuild_study_pack_index(
    pack_id: Option<String>,
    db: State<'_, Database>,
) -> Result<StudyPackIndexResponse, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let target_id = active_or_requested_pack_id(&conn, pack_id.as_deref())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Create or select a Study Pack first.".to_string())?;
    if !pack_exists(&conn, &target_id).map_err(|e| e.to_string())? {
        return Err("Study Pack not found.".to_string());
    }

    conn.execute(
        "UPDATE study_context_items \
         SET index_status = 'indexing', index_error = NULL \
         WHERE pack_id = ?1",
        params![&target_id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM study_context_chunks WHERE pack_id = ?1",
        params![&target_id],
    )
    .map_err(|e| e.to_string())?;
    delete_pack_fts(&conn, &target_id);

    let mut stmt = conn
        .prepare(
            "SELECT id, title, raw_ocr, COALESCE(tags, '') \
             FROM study_context_items \
             WHERE pack_id = ?1 \
             ORDER BY created_at ASC",
        )
        .map_err(|e| e.to_string())?;
    let items = stmt
        .query_map(params![&target_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    drop(stmt);

    let now = now_millis();
    let mut chunks_saved = 0;
    let mut indexed_items = 0;
    for (item_id, title, raw_ocr, tags) in &items {
        match index_item_chunks(&conn, item_id, &target_id, title, raw_ocr, tags, now) {
            Ok(count) => {
                chunks_saved += count;
                indexed_items += 1;
            }
            Err(e) => {
                let _ = conn.execute(
                    "UPDATE study_context_items \
                     SET index_status = 'error', index_error = ?1 \
                     WHERE id = ?2",
                    params![e.to_string(), item_id],
                );
            }
        }
    }
    conn.execute(
        "UPDATE study_packs SET updated_at = ?1 WHERE id = ?2",
        params![now, &target_id],
    )
    .map_err(|e| e.to_string())?;

    Ok(StudyPackIndexResponse {
        pack_id: target_id,
        total_items: items.len(),
        indexed_items,
        chunks_saved,
    })
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn check_answer_from_context(
    pack_id: Option<String>,
    current_ocr: String,
    question: String,
    student_answer: Option<String>,
    db: State<'_, Database>,
) -> Result<ContextPromptResponse, String> {
    let query = format!(
        "{}\n{}\n{}",
        question,
        student_answer.as_deref().unwrap_or_default(),
        current_ocr
    );
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let context =
        retrieve_context_for_prompt(&conn, pack_id.as_deref(), &query, RETRIEVAL_LIMIT_DEFAULT)
            .map_err(|e| e.to_string())?;
    let enough_context = !context.chunks.is_empty();
    let prompt = format!(
        "{}\n\n[Current Quiz Screenshot OCR]\n{}\n\n[Student Question]\n{}\n\n[Student Answer If Known]\n{}\n\nYou are a strict answer verifier for a student. Follow this process:\n1. Extract the quiz question, answer options, and the student's selected answer from the current OCR when present.\n2. Compare only against the saved Study Pack evidence above. Treat the saved manual/explanation pages as the authority.\n3. If no saved source directly supports the rule needed to verify the answer, say: \"I can't verify this from the saved Study Pack yet\" and state exactly what page/rule should be saved. Do not use outside knowledge to guess.\n4. If evidence is sufficient, give a clear verdict: Correct, Incorrect, or Not enough context.\n5. Cite source IDs like [SP1] next to every rule you rely on.\n6. Keep the correction in small learning steps and end with one short check question.",
        if context.context_block.trim().is_empty() {
            "[Saved Study Context]\nNo relevant saved context was found for this question.".to_string()
        } else {
            context.context_block.clone()
        },
        current_ocr.trim(),
        question.trim(),
        student_answer.as_deref().unwrap_or("").trim(),
    );
    Ok(ContextPromptResponse {
        prompt,
        context,
        enough_context,
    })
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn get_study_pack_summary(
    pack_id: Option<String>,
    db: State<'_, Database>,
) -> Result<StudyPackOverview, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let active_id = get_active_pack_id(&conn).map_err(|e| e.to_string())?;
    let target_id =
        active_or_requested_pack_id(&conn, pack_id.as_deref()).map_err(|e| e.to_string())?;
    let Some(target_id) = target_id else {
        return Ok(StudyPackOverview {
            pack: None,
            item_count: 0,
            chunk_count: 0,
        });
    };
    let pack = load_pack(&conn, &target_id, active_id.as_deref()).map_err(|e| e.to_string())?;
    let item_count = conn
        .query_row(
            "SELECT COUNT(*) FROM study_context_items WHERE pack_id = ?1",
            params![target_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let chunk_count = conn
        .query_row(
            "SELECT COUNT(*) FROM study_context_chunks WHERE pack_id = ?1",
            params![target_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(StudyPackOverview {
        pack,
        item_count,
        chunk_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database;

    #[test]
    fn chunk_text_splits_long_context() {
        let text = (0..80)
            .map(|i| format!("line {i} says stop sign means stop completely"))
            .collect::<Vec<_>>()
            .join("\n");
        let chunks = chunk_text(&text);
        assert!(chunks.len() > 1);
        assert!(chunks
            .iter()
            .all(|chunk| chunk.len() <= MAX_CHUNK_CHARS + 80));
    }

    #[test]
    fn retrieve_context_scores_relevant_chunks() {
        let conn = database::open_in_memory().unwrap();
        let now = now_millis();
        let pack_id = "pack";
        conn.execute(
            "INSERT INTO study_packs (id, name, authority_source, description, created_at, updated_at) \
             VALUES (?1, 'Driver License', 'Italy', NULL, ?2, ?2)",
            params![pack_id, now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO study_context_items \
             (id, pack_id, title, source_kind, raw_ocr, created_at) \
             VALUES ('item1', ?1, 'Priority rules', 'screenshot', 'raw', ?2)",
            params![pack_id, now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO study_context_chunks \
             (id, item_id, pack_id, chunk_index, chunk_text, source_label, created_at) \
             VALUES ('c1', 'item1', ?1, 0, 'At a stop sign the driver must stop completely before entering.', 'Priority rules', ?2)",
            params![pack_id, now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO study_context_chunks \
             (id, item_id, pack_id, chunk_index, chunk_text, source_label, created_at) \
             VALUES ('c2', 'item1', ?1, 1, 'Headlights are required in poor visibility.', 'Lights', ?2)",
            params![pack_id, now],
        )
        .unwrap();

        let result =
            retrieve_context_for_prompt(&conn, Some(pack_id), "What does a stop sign mean?", 3)
                .unwrap();
        assert_eq!(result.chunks[0].id, "c1");
        assert!(result.context_block.contains("[SP1]"));
    }

    #[test]
    fn index_item_chunks_marks_item_ready_and_populates_fts() {
        let conn = database::open_in_memory().unwrap();
        let now = now_millis();
        let pack_id = "pack";
        conn.execute(
            "INSERT INTO study_packs (id, name, authority_source, description, created_at, updated_at) \
             VALUES (?1, 'Driver License', 'Official manual', NULL, ?2, ?2)",
            params![pack_id, now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO study_context_items \
             (id, pack_id, title, source_kind, raw_ocr, tags, created_at) \
             VALUES ('item1', ?1, 'Warning signs', 'screenshot', 'Slow down near warning signs. Tram lanes have priority rules.', '[\"warning\",\"tram\"]', ?2)",
            params![pack_id, now],
        )
        .unwrap();

        let chunks_saved = index_item_chunks(
            &conn,
            "item1",
            pack_id,
            "Warning signs",
            "Slow down near warning signs. Tram lanes have priority rules.",
            "[\"warning\",\"tram\"]",
            now,
        )
        .unwrap();
        assert_eq!(chunks_saved, 1);

        let indexed_at: Option<i64> = conn
            .query_row(
                "SELECT indexed_at FROM study_context_items WHERE id = 'item1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(indexed_at, Some(now));

        let fts_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM study_context_fts WHERE pack_id = ?1 AND study_context_fts MATCH 'tram*'",
                params![pack_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fts_rows, 1);

        let pack = load_pack(&conn, pack_id, Some(pack_id)).unwrap().unwrap();
        assert_eq!(pack.item_count, 1);
        assert_eq!(pack.indexed_count, 1);
        assert_eq!(pack.needs_index_count, 0);
    }

    #[test]
    fn backfill_image_paths_copies_existing_legacy_files() {
        let conn = database::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let legacy_dir = dir.path().join("legacy");
        std::fs::create_dir_all(&legacy_dir).unwrap();
        let legacy_image = legacy_dir.join("old.png");
        std::fs::write(&legacy_image, b"image").unwrap();
        let missing_image = legacy_dir.join("missing.png");
        let now = now_millis();
        let pack_id = "pack";
        let image_paths = serde_json::to_string(&vec![
            legacy_image.to_string_lossy().to_string(),
            missing_image.to_string_lossy().to_string(),
        ])
        .unwrap();

        conn.execute(
            "INSERT INTO study_packs (id, name, authority_source, description, created_at, updated_at) \
             VALUES (?1, 'Driver License', 'Official manual', NULL, ?2, ?2)",
            params![pack_id, now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO study_context_items \
             (id, pack_id, title, source_kind, image_paths, raw_ocr, created_at) \
             VALUES ('item1', ?1, 'Legacy page', 'screen', ?2, 'legacy ocr', ?3)",
            params![pack_id, image_paths, now],
        )
        .unwrap();

        let response = backfill_study_context_image_paths_in_base(&conn, dir.path()).unwrap();
        assert_eq!(response.items_scanned, 1);
        assert_eq!(response.items_updated, 1);
        assert_eq!(response.images_copied, 1);
        assert_eq!(response.missing_images, 1);

        let stored: String = conn
            .query_row(
                "SELECT image_paths FROM study_context_items WHERE id = 'item1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let paths = serde_json::from_str::<Vec<String>>(&stored).unwrap();
        assert_eq!(paths.len(), 2);
        assert!(Path::new(&paths[0]).starts_with(study_context_images_root(dir.path(), pack_id)));
        assert!(Path::new(&paths[0]).exists());
        assert_eq!(paths[1], missing_image.to_string_lossy());
    }
}
