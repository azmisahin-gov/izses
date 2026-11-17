//! İnceleme kayıtlarının kalıcılığı.

use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{params, Row};

use crate::error::{CoreError, CoreResult};
use crate::model::{Id, Investigation, InvestigationStatus};
use crate::store::Store;
use crate::timefmt;

fn parse_date(s: Option<String>) -> Option<NaiveDate> {
    s.and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok())
}

fn parse_dt(s: Option<String>) -> Option<DateTime<Utc>> {
    s.and_then(|s| timefmt::parse_iso(&s))
}

fn row_to_investigation(row: &Row<'_>) -> rusqlite::Result<Investigation> {
    let parties_json: String = row.get("parties")?;
    let questions_json: String = row.get("questions")?;
    let status_str: String = row.get("status")?;
    Ok(Investigation {
        id: Id(row.get("id")?),
        court: row.get("court")?,
        case_number: row.get("case_number")?,
        assignment_date: parse_date(row.get("assignment_date")?),
        due_date: parse_date(row.get("due_date")?),
        allowed_days: row.get("allowed_days")?,
        parties: serde_json::from_str(&parties_json).unwrap_or_default(),
        subject: row.get("subject")?,
        expert_name: row.get("expert_name")?,
        expert_registry_no: row.get("expert_registry_no")?,
        expertise_field: row.get("expertise_field")?,
        questions: serde_json::from_str(&questions_json).unwrap_or_default(),
        storage_key: row.get("storage_key")?,
        scope_notes: row.get("scope_notes")?,
        status: InvestigationStatus::from_str_opt(&status_str).unwrap_or(InvestigationStatus::Draft),
        created_at: parse_dt(row.get("created_at")?).unwrap_or_else(Utc::now),
        updated_at: parse_dt(row.get("updated_at")?).unwrap_or_else(Utc::now),
    })
}

impl Store {
    /// Yeni bir inceleme kaydeder. Depolama anahtarı çakışırsa hata verir.
    pub fn insert_investigation(&self, inv: &Investigation) -> CoreResult<()> {
        let parties = serde_json::to_string(&inv.parties)?;
        let questions = serde_json::to_string(&inv.questions)?;
        self.conn().execute(
            r#"
            INSERT INTO investigations (
                id, court, case_number, assignment_date, due_date, allowed_days,
                parties, subject, expert_name, expert_registry_no, expertise_field,
                questions, scope_notes, status, storage_key, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
            "#,
            params![
                inv.id.0,
                inv.court,
                inv.case_number,
                inv.assignment_date.map(|d| d.to_string()),
                inv.due_date.map(|d| d.to_string()),
                inv.allowed_days,
                parties,
                inv.subject,
                inv.expert_name,
                inv.expert_registry_no,
                inv.expertise_field,
                questions,
                inv.scope_notes,
                inv.status.as_str(),
                inv.storage_key(),
                timefmt::to_iso(&inv.created_at),
                timefmt::to_iso(&inv.updated_at),
            ],
        )?;
        Ok(())
    }

    /// İnceleme bilgilerini günceller.
    pub fn update_investigation(&self, inv: &mut Investigation) -> CoreResult<()> {
        inv.updated_at = Utc::now();
        let parties = serde_json::to_string(&inv.parties)?;
        let questions = serde_json::to_string(&inv.questions)?;
        let n = self.conn().execute(
            r#"
            UPDATE investigations SET
                court = ?2, case_number = ?3, assignment_date = ?4, due_date = ?5,
                allowed_days = ?6, parties = ?7, subject = ?8, expert_name = ?9,
                expert_registry_no = ?10, expertise_field = ?11, questions = ?12,
                scope_notes = ?13, status = ?14, updated_at = ?15
            WHERE id = ?1
            "#,
            params![
                inv.id.0,
                inv.court,
                inv.case_number,
                inv.assignment_date.map(|d| d.to_string()),
                inv.due_date.map(|d| d.to_string()),
                inv.allowed_days,
                parties,
                inv.subject,
                inv.expert_name,
                inv.expert_registry_no,
                inv.expertise_field,
                questions,
                inv.scope_notes,
                inv.status.as_str(),
                timefmt::to_iso(&inv.updated_at),
            ],
        )?;
        if n == 0 {
            return Err(CoreError::not_found("inceleme"));
        }
        Ok(())
    }

    pub fn get_investigation(&self, id: &Id) -> CoreResult<Investigation> {
        self.conn()
            .query_row(
                "SELECT * FROM investigations WHERE id = ?1",
                params![id.0],
                row_to_investigation,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => CoreError::not_found("inceleme"),
                other => CoreError::Database(other),
            })
    }

    pub fn get_investigation_by_storage_key(&self, key: &str) -> CoreResult<Investigation> {
        self.conn()
            .query_row(
                "SELECT * FROM investigations WHERE storage_key = ?1",
                params![key],
                row_to_investigation,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => CoreError::not_found("inceleme"),
                other => CoreError::Database(other),
            })
    }

    /// Tüm incelemeleri son güncellenme sırasına göre listeler.
    pub fn list_investigations(&self) -> CoreResult<Vec<Investigation>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT * FROM investigations ORDER BY updated_at DESC")?;
        let rows = stmt.query_map([], row_to_investigation)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Belirli durumdaki incelemeleri listeler.
    pub fn list_investigations_by_status(
        &self,
        status: InvestigationStatus,
    ) -> CoreResult<Vec<Investigation>> {
        let mut stmt = self.conn().prepare(
            "SELECT * FROM investigations WHERE status = ?1 ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map(params![status.as_str()], row_to_investigation)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Depolama anahtarının kullanımda olup olmadığı.
    pub fn storage_key_taken(&self, key: &str) -> CoreResult<bool> {
        let count: i64 = self.conn().query_row(
            "SELECT COUNT(*) FROM investigations WHERE storage_key = ?1",
            params![key],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    }

    /// İncelemeyi ve ilişkili tüm kayıtları siler.
    pub fn delete_investigation(&self, id: &Id) -> CoreResult<()> {
        let n = self
            .conn()
            .execute("DELETE FROM investigations WHERE id = ?1", params![id.0])?;
        if n == 0 {
            return Err(CoreError::not_found("inceleme"));
        }
        Ok(())
    }

    /// Bir incelemenin son güncellenme zamanını şimdiye ayarlar.
    pub fn touch_investigation(&self, id: &Id) -> CoreResult<()> {
        let n = self.conn().execute(
            "UPDATE investigations SET updated_at = ?2 WHERE id = ?1",
            params![id.0, timefmt::to_iso(&Utc::now())],
        )?;
        if n == 0 {
            return Err(CoreError::not_found("inceleme"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Party;

    fn party(role: impl Into<String>, name: impl Into<String>) -> Party {
        Party {
            role: role.into(),
            name: name.into(),
        }
    }

    fn sample() -> Investigation {
        let mut inv = Investigation::new();
        inv.court = "Ankara 1. Asliye Ceza Mahkemesi".into();
        inv.case_number = "2026/125".into();
        inv.subject = "Ses kaydının çözümlenmesi".into();
        inv.expert_name = "A. Bilirkişi".into();
        inv.expert_registry_no = "12345".into();
        inv.questions = vec!["Kayıtta ne konuşulmaktadır?".into()];
        inv.parties = vec![party("Şüpheli", "X"), party("Mağdur", "Y")];
        inv
    }

    #[test]
    fn insert_and_get_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        let inv = sample();
        store.insert_investigation(&inv).unwrap();
        let got = store.get_investigation(&inv.id).unwrap();
        assert_eq!(got.case_number, "2026/125");
        assert_eq!(got.parties.len(), 2);
        assert_eq!(got.questions.len(), 1);
        assert_eq!(got.status, InvestigationStatus::Draft);
    }

    #[test]
    fn storage_key_must_be_unique() {
        let store = Store::open_in_memory().unwrap();
        let inv = sample();
        store.insert_investigation(&inv).unwrap();
        let mut other = Investigation::new();
        other.case_number = "2026/125".into();
        assert!(store.insert_investigation(&other).is_err());
        assert!(store.storage_key_taken("2026-125").unwrap());
    }

    #[test]
    fn update_persists_changes() {
        let store = Store::open_in_memory().unwrap();
        let mut inv = sample();
        store.insert_investigation(&inv).unwrap();
        inv.status = InvestigationStatus::InProgress;
        inv.subject = "Güncellenmiş konu".into();
        store.update_investigation(&mut inv).unwrap();
        let got = store.get_investigation(&inv.id).unwrap();
        assert_eq!(got.status, InvestigationStatus::InProgress);
        assert_eq!(got.subject, "Güncellenmiş konu");
    }

    #[test]
    fn list_by_status_filters() {
        let store = Store::open_in_memory().unwrap();
        let mut a = sample();
        a.case_number = "A/1".into();
        store.insert_investigation(&a).unwrap();
        let mut b = sample();
        b.case_number = "B/1".into();
        b.status = InvestigationStatus::Completed;
        store.insert_investigation(&b).unwrap();

        let in_progress = store
            .list_investigations_by_status(InvestigationStatus::Draft)
            .unwrap();
        assert_eq!(in_progress.len(), 1);
        let completed = store
            .list_investigations_by_status(InvestigationStatus::Completed)
            .unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].case_number, "B/1");
    }

    #[test]
    fn delete_cascades() {
        let store = Store::open_in_memory().unwrap();
        let inv = sample();
        store.insert_investigation(&inv).unwrap();
        let src = crate::transcript::EvidenceSource::new(
            &inv.id,
            "USB",
            "/tmp/usb",
            crate::device::DeviceKind::Removable,
            1,
        );
        store.insert_source(&src).unwrap();
        store.delete_investigation(&inv.id).unwrap();
        assert!(store.list_sources(&inv.id).unwrap().is_empty());
        assert!(store.get_investigation(&inv.id).is_err());
    }

    #[test]
    fn missing_investigation_reports_not_found() {
        let store = Store::open_in_memory().unwrap();
        let err = store.get_investigation(&Id::new()).unwrap_err();
        assert!(matches!(err, CoreError::NotFound(_)));
    }
}
