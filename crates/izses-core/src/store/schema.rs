//! Veritabanı şeması ve göçler (migrations).

use rusqlite::Connection;

use crate::error::CoreResult;

/// Şema sürümü.
pub const SCHEMA_VERSION: i32 = 1;

/// Şemayı oluşturur ve sürümü günceller.
pub fn migrate(conn: &Connection) -> CoreResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS schema_info (
            version INTEGER NOT NULL
        );
        "#,
    )?;

    let current: Option<i32> = conn
        .query_row("SELECT version FROM schema_info LIMIT 1", [], |row| {
            row.get(0)
        })
        .ok();

    let current = current.unwrap_or(0);
    if current < 1 {
        apply_v1(conn)?;
    }

    conn.execute("DELETE FROM schema_info", [])?;
    conn.execute(
        "INSERT INTO schema_info (version) VALUES (?1)",
        [SCHEMA_VERSION],
    )?;
    Ok(())
}

fn apply_v1(conn: &Connection) -> CoreResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS investigations (
            id                  TEXT PRIMARY KEY,
            court               TEXT NOT NULL DEFAULT '',
            case_number         TEXT NOT NULL DEFAULT '',
            assignment_date     TEXT,
            due_date            TEXT,
            allowed_days        INTEGER,
            parties             TEXT NOT NULL DEFAULT '[]',
            subject             TEXT NOT NULL DEFAULT '',
            expert_name         TEXT NOT NULL DEFAULT '',
            expert_registry_no  TEXT NOT NULL DEFAULT '',
            expertise_field     TEXT NOT NULL DEFAULT '',
            questions           TEXT NOT NULL DEFAULT '[]',
            scope_notes         TEXT NOT NULL DEFAULT '',
            status              TEXT NOT NULL DEFAULT 'draft',
            storage_key         TEXT NOT NULL UNIQUE,
            created_at          TEXT NOT NULL,
            updated_at          TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS evidence_sources (
            id                  TEXT PRIMARY KEY,
            investigation_id    TEXT NOT NULL REFERENCES investigations(id) ON DELETE CASCADE,
            label               TEXT NOT NULL,
            root_path           TEXT NOT NULL,
            kind                TEXT NOT NULL DEFAULT 'unknown',
            filesystem          TEXT,
            total_bytes         INTEGER,
            ordinal             INTEGER NOT NULL,
            added_at            TEXT NOT NULL,
            last_scan_at        TEXT,
            last_scan_count     INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_sources_inv ON evidence_sources(investigation_id);

        CREATE TABLE IF NOT EXISTS evidence_files (
            id                  TEXT PRIMARY KEY,
            investigation_id    TEXT NOT NULL REFERENCES investigations(id) ON DELETE CASCADE,
            source_id           TEXT NOT NULL REFERENCES evidence_sources(id) ON DELETE CASCADE,
            relative_path       TEXT NOT NULL,
            local_relative_path TEXT NOT NULL,
            size_bytes          INTEGER NOT NULL DEFAULT 0,
            extension           TEXT NOT NULL DEFAULT '',
            category            TEXT NOT NULL DEFAULT 'other',
            source_modified     TEXT,
            source_created      TEXT,
            acquired_at         TEXT,
            source_sha256       TEXT,
            copy_sha256         TEXT,
            verification        TEXT NOT NULL DEFAULT 'not_acquired',
            audio               TEXT,
            issue               TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_files_inv ON evidence_files(investigation_id);
        CREATE INDEX IF NOT EXISTS idx_files_source ON evidence_files(source_id);

        CREATE TABLE IF NOT EXISTS transcripts (
            id                  TEXT PRIMARY KEY,
            investigation_id    TEXT NOT NULL REFERENCES investigations(id) ON DELETE CASCADE,
            evidence_file_id    TEXT NOT NULL REFERENCES evidence_files(id) ON DELETE CASCADE,
            model_name          TEXT NOT NULL DEFAULT '',
            model_revision      TEXT NOT NULL DEFAULT '',
            model_sha256        TEXT,
            language            TEXT NOT NULL DEFAULT 'tr',
            task                TEXT NOT NULL DEFAULT 'transcribe',
            settings            TEXT NOT NULL DEFAULT '{}',
            status              TEXT NOT NULL DEFAULT 'queued',
            completed_segments  INTEGER NOT NULL DEFAULT 0,
            total_segments      INTEGER,
            error               TEXT,
            created_at          TEXT NOT NULL,
            updated_at          TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_transcripts_inv ON transcripts(investigation_id);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_transcripts_file ON transcripts(evidence_file_id);

        CREATE TABLE IF NOT EXISTS transcript_segments (
            id                  TEXT PRIMARY KEY,
            transcript_id       TEXT NOT NULL REFERENCES transcripts(id) ON DELETE CASCADE,
            idx                 INTEGER NOT NULL,
            start_ms            INTEGER NOT NULL,
            end_ms              INTEGER NOT NULL,
            auto_text           TEXT NOT NULL DEFAULT '',
            edited_text         TEXT,
            speaker             TEXT,
            flags               TEXT NOT NULL DEFAULT '{}',
            observation         TEXT,
            avg_logprob         REAL,
            no_speech_prob      REAL,
            updated_at          TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_segments_transcript ON transcript_segments(transcript_id, idx);

        CREATE TABLE IF NOT EXISTS segment_revisions (
            id                  TEXT PRIMARY KEY,
            segment_id          TEXT NOT NULL,
            transcript_id       TEXT NOT NULL REFERENCES transcripts(id) ON DELETE CASCADE,
            at                  TEXT NOT NULL,
            before_text         TEXT NOT NULL DEFAULT '',
            after_text          TEXT NOT NULL DEFAULT '',
            author              TEXT NOT NULL DEFAULT '',
            note                TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_revisions_transcript ON segment_revisions(transcript_id);

        CREATE TABLE IF NOT EXISTS notes (
            id                  TEXT PRIMARY KEY,
            investigation_id    TEXT NOT NULL REFERENCES investigations(id) ON DELETE CASCADE,
            evidence_file_id    TEXT,
            segment_id          TEXT,
            kind                TEXT NOT NULL DEFAULT 'general',
            title               TEXT NOT NULL DEFAULT '',
            body                TEXT NOT NULL DEFAULT '',
            created_at          TEXT NOT NULL,
            updated_at          TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_notes_inv ON notes(investigation_id);

        CREATE TABLE IF NOT EXISTS audit_log (
            id                  TEXT PRIMARY KEY,
            investigation_id    TEXT NOT NULL REFERENCES investigations(id) ON DELETE CASCADE,
            at                  TEXT NOT NULL,
            kind                TEXT NOT NULL,
            summary             TEXT NOT NULL DEFAULT '',
            details             TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_audit_inv ON audit_log(investigation_id, at);

        CREATE TABLE IF NOT EXISTS reports (
            id                  TEXT PRIMARY KEY,
            investigation_id    TEXT NOT NULL REFERENCES investigations(id) ON DELETE CASCADE,
            title               TEXT NOT NULL DEFAULT '',
            generated_at        TEXT NOT NULL,
            content             TEXT NOT NULL DEFAULT '{}',
            exported_paths      TEXT NOT NULL DEFAULT '[]'
        );
        CREATE INDEX IF NOT EXISTS idx_reports_inv ON reports(investigation_id);
        "#,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_creates_tables_idempotently() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='investigations'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}
