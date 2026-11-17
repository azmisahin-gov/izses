//! Notların kalıcılığı.

use chrono::{DateTime, Utc};
use rusqlite::{params, Row};

use crate::error::{CoreError, CoreResult};
use crate::model::Id;
use crate::store::Store;
use crate::timefmt;
use crate::transcript::{Note, NoteKind};

fn parse_dt(s: Option<String>) -> Option<DateTime<Utc>> {
    s.and_then(|s| timefmt::parse_iso(&s))
}

fn note_from_row(row: &Row<'_>) -> rusqlite::Result<Note> {
    let kind: String = row.get("kind")?;
    let file_id: Option<String> = row.get("evidence_file_id")?;
    let seg_id: Option<String> = row.get("segment_id")?;
    Ok(Note {
        id: Id(row.get("id")?),
        investigation_id: Id(row.get("investigation_id")?),
        evidence_file_id: file_id.map(Id),
        segment_id: seg_id.map(Id),
        kind: NoteKind::from_str_opt(&kind).unwrap_or(NoteKind::General),
        title: row.get("title")?,
        body: row.get("body")?,
        created_at: parse_dt(row.get("created_at")?).unwrap_or_else(Utc::now),
        updated_at: parse_dt(row.get("updated_at")?).unwrap_or_else(Utc::now),
    })
}

impl Store {
    pub fn insert_note(&self, note: &Note) -> CoreResult<()> {
        self.conn().execute(
            r#"
            INSERT INTO notes (
                id, investigation_id, evidence_file_id, segment_id, kind,
                title, body, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                note.id.0,
                note.investigation_id.0,
                note.evidence_file_id.as_ref().map(|i| i.0.clone()),
                note.segment_id.as_ref().map(|i| i.0.clone()),
                note.kind.as_str(),
                note.title,
                note.body,
                timefmt::to_iso(&note.created_at),
                timefmt::to_iso(&note.updated_at),
            ],
        )?;
        Ok(())
    }

    pub fn update_note(&self, note: &mut Note) -> CoreResult<()> {
        note.updated_at = Utc::now();
        let n = self.conn().execute(
            "UPDATE notes SET kind = ?2, title = ?3, body = ?4, updated_at = ?5 WHERE id = ?1",
            params![
                note.id.0,
                note.kind.as_str(),
                note.title,
                note.body,
                timefmt::to_iso(&note.updated_at),
            ],
        )?;
        if n == 0 {
            return Err(CoreError::not_found("not"));
        }
        Ok(())
    }

    pub fn list_notes(&self, investigation_id: &Id) -> CoreResult<Vec<Note>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT * FROM notes WHERE investigation_id = ?1 ORDER BY created_at DESC")?;
        let rows = stmt.query_map(params![investigation_id.0], note_from_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn get_note(&self, id: &Id) -> CoreResult<Note> {
        self.conn()
            .query_row(
                "SELECT * FROM notes WHERE id = ?1",
                params![id.0],
                note_from_row,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => CoreError::not_found("not"),
                other => CoreError::Database(other),
            })
    }

    pub fn delete_note(&self, id: &Id) -> CoreResult<()> {
        let n = self
            .conn()
            .execute("DELETE FROM notes WHERE id = ?1", params![id.0])?;
        if n == 0 {
            return Err(CoreError::not_found("not"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Investigation;

    #[test]
    fn notes_are_scoped_to_investigation() {
        let store = Store::open_in_memory().unwrap();
        let mut a = Investigation::new();
        a.case_number = "A".into();
        store.insert_investigation(&a).unwrap();
        let mut b = Investigation::new();
        b.case_number = "B".into();
        store.insert_investigation(&b).unwrap();

        let note = Note::new(&a.id, NoteKind::Technical, "Başlık", "Gövde");
        store.insert_note(&note).unwrap();

        assert_eq!(store.list_notes(&a.id).unwrap().len(), 1);
        assert_eq!(store.list_notes(&b.id).unwrap().len(), 0);
    }

    #[test]
    fn note_update_and_delete() {
        let store = Store::open_in_memory().unwrap();
        let inv = Investigation::new();
        store.insert_investigation(&inv).unwrap();
        let mut note = Note::new(&inv.id, NoteKind::General, "T", "B");
        store.insert_note(&note).unwrap();

        note.body = "Yeni gövde".into();
        store.update_note(&mut note).unwrap();
        assert_eq!(store.get_note(&note.id).unwrap().body, "Yeni gövde");

        store.delete_note(&note.id).unwrap();
        assert!(store.get_note(&note.id).is_err());
    }
}
