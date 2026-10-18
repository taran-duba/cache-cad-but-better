//! Local SQLite index of the shared drive. The UI reads only from here, so
//! browsing never waits on the network. Kept current by `sync::crawl`.

mod paths;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

pub use paths::{PathMap, META_FOLDER};

use crate::drive::RemoteFile;
use crate::error::Result;

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS files (
    id TEXT PRIMARY KEY,
    parent_id TEXT,
    name TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    md5 TEXT,
    size INTEGER,
    head_rev TEXT,
    created_time TEXT NOT NULL,
    modified_time TEXT NOT NULL,
    read_only INTEGER NOT NULL DEFAULT 0,
    app_props TEXT NOT NULL DEFAULT '{}',
    last_modifier TEXT
);
CREATE INDEX IF NOT EXISTS files_parent ON files(parent_id);

-- What this machine has on disk, per file.
CREATE TABLE IF NOT EXISTS local (
    file_id TEXT PRIMARY KEY,
    rel_path TEXT NOT NULL,
    synced_rev TEXT,
    synced_md5 TEXT,
    synced_size INTEGER,
    synced_mtime INTEGER
);

-- Selective sync. mode 1 = get, 0 = don't get; inherited from the nearest ancestor.
CREATE TABLE IF NOT EXISTS selection (file_id TEXT PRIMARY KEY, mode INTEGER NOT NULL);

CREATE TABLE IF NOT EXISTS changesets (
    id TEXT PRIMARY KEY,
    author TEXT NOT NULL,
    message TEXT NOT NULL,
    created_time TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS changeset_revs (
    changeset_id TEXT NOT NULL REFERENCES changesets(id) ON DELETE CASCADE,
    file_id TEXT NOT NULL,
    revision_id TEXT NOT NULL,
    PRIMARY KEY (file_id, revision_id)
);

-- Local push journal so an interrupted check-in resumes after a crash.
CREATE TABLE IF NOT EXISTS journal (
    changeset_id TEXT PRIMARY KEY,
    payload TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Assembly/drawing -> referenced file, as read by the SwRefs sidecar.
CREATE TABLE IF NOT EXISTS refs (
    from_id TEXT NOT NULL,
    to_path TEXT NOT NULL,
    PRIMARY KEY (from_id, to_path)
);
"#;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalState {
    pub file_id: String,
    pub rel_path: String,
    pub synced_rev: Option<String>,
    pub synced_md5: Option<String>,
    pub synced_size: Option<u64>,
    pub synced_mtime: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub author: String,
    pub message: String,
    pub created_time: String,
    /// file id -> revision id
    pub files: BTreeMap<String, String>,
}

pub struct Index {
    conn: Mutex<Connection>,
}

fn file_from_row(r: &Row) -> rusqlite::Result<RemoteFile> {
    let props: String = r.get("app_props")?;
    Ok(RemoteFile {
        id: r.get("id")?,
        parent: r.get("parent_id")?,
        name: r.get("name")?,
        mime_type: r.get("mime_type")?,
        md5: r.get("md5")?,
        size: r.get::<_, Option<i64>>("size")?.map(|s| s as u64),
        head_revision_id: r.get("head_rev")?,
        created_time: r.get("created_time")?,
        modified_time: r.get("modified_time")?,
        trashed: false,
        read_only: r.get::<_, i64>("read_only")? != 0,
        app_properties: serde_json::from_str(&props).unwrap_or_default(),
        last_modifier: r.get("last_modifier")?,
    })
}

fn local_from_row(r: &Row) -> rusqlite::Result<LocalState> {
    Ok(LocalState {
        file_id: r.get("file_id")?,
        rel_path: r.get("rel_path")?,
        synced_rev: r.get("synced_rev")?,
        synced_md5: r.get("synced_md5")?,
        synced_size: r.get::<_, Option<i64>>("synced_size")?.map(|s| s as u64),
        synced_mtime: r.get("synced_mtime")?,
    })
}

impl Index {
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        // A panic while holding the lock leaves the connection usable; SQLite
        // transactions roll back on their own.
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    // ---- meta ----

    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn()
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn().execute(
            "INSERT INTO meta(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
            [key, value],
        )?;
        Ok(())
    }

    // ---- remote files ----

    /// Apply a batch of remote updates and removals in one transaction.
    pub fn apply_remote(&self, upserts: &[RemoteFile], removals: &[String]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        {
            let mut up = tx.prepare_cached(
                "INSERT INTO files(id, parent_id, name, mime_type, md5, size, head_rev, created_time,
                    modified_time, read_only, app_props, last_modifier)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(id) DO UPDATE SET parent_id = ?2, name = ?3, mime_type = ?4, md5 = ?5,
                    size = ?6, head_rev = ?7, created_time = ?8, modified_time = ?9, read_only = ?10,
                    app_props = ?11, last_modifier = ?12",
            )?;
            for f in upserts {
                up.execute(params![
                    f.id,
                    f.parent,
                    f.name,
                    f.mime_type,
                    f.md5,
                    f.size.map(|s| s as i64),
                    f.head_revision_id,
                    f.created_time,
                    f.modified_time,
                    f.read_only as i64,
                    serde_json::to_string(&f.app_properties)?,
                    f.last_modifier,
                ])?;
            }
            let mut rm = tx.prepare_cached("DELETE FROM files WHERE id = ?1")?;
            for id in removals {
                rm.execute([id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn clear_remote(&self) -> Result<()> {
        self.conn().execute("DELETE FROM files", [])?;
        Ok(())
    }

    pub fn file(&self, id: &str) -> Result<Option<RemoteFile>> {
        Ok(self
            .conn()
            .query_row("SELECT * FROM files WHERE id = ?1", [id], file_from_row)
            .optional()?)
    }

    pub fn children(&self, parent_id: &str) -> Result<Vec<RemoteFile>> {
        let conn = self.conn();
        let mut st = conn.prepare_cached(
            "SELECT * FROM files WHERE parent_id = ?1
             ORDER BY mime_type <> 'application/vnd.google-apps.folder', name COLLATE NOCASE",
        )?;
        let rows = st.query_map([parent_id], file_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn all_files(&self) -> Result<Vec<RemoteFile>> {
        let conn = self.conn();
        let mut st = conn.prepare_cached("SELECT * FROM files")?;
        let rows = st.query_map([], file_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn file_count(&self) -> Result<u64> {
        Ok(self.conn().query_row("SELECT COUNT(*) FROM files", [], |r| r.get::<_, i64>(0))? as u64)
    }

    // ---- local state ----

    pub fn local(&self, file_id: &str) -> Result<Option<LocalState>> {
        Ok(self
            .conn()
            .query_row("SELECT * FROM local WHERE file_id = ?1", [file_id], local_from_row)
            .optional()?)
    }

    pub fn all_local(&self) -> Result<Vec<LocalState>> {
        let conn = self.conn();
        let mut st = conn.prepare_cached("SELECT * FROM local")?;
        let rows = st.query_map([], local_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn set_local(&self, s: &LocalState) -> Result<()> {
        self.conn().execute(
            "INSERT INTO local(file_id, rel_path, synced_rev, synced_md5, synced_size, synced_mtime)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(file_id) DO UPDATE SET rel_path = ?2, synced_rev = ?3, synced_md5 = ?4,
                synced_size = ?5, synced_mtime = ?6",
            params![s.file_id, s.rel_path, s.synced_rev, s.synced_md5, s.synced_size.map(|v| v as i64), s.synced_mtime],
        )?;
        Ok(())
    }

    pub fn remove_local(&self, file_id: &str) -> Result<()> {
        self.conn().execute("DELETE FROM local WHERE file_id = ?1", [file_id])?;
        Ok(())
    }

    // ---- selective sync ----

    pub fn set_selection(&self, file_id: &str, get: Option<bool>) -> Result<()> {
        let conn = self.conn();
        match get {
            Some(g) => conn.execute(
                "INSERT INTO selection(file_id, mode) VALUES (?1, ?2)
                 ON CONFLICT(file_id) DO UPDATE SET mode = ?2",
                params![file_id, g as i64],
            )?,
            None => conn.execute("DELETE FROM selection WHERE file_id = ?1", [file_id])?,
        };
        Ok(())
    }

    pub fn selections(&self) -> Result<BTreeMap<String, bool>> {
        let conn = self.conn();
        let mut st = conn.prepare_cached("SELECT file_id, mode FROM selection")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // ---- changesets ----

    pub fn insert_changeset(&self, m: &Manifest) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO changesets(id, author, message, created_time) VALUES (?1, ?2, ?3, ?4)",
            params![m.id, m.author, m.message, m.created_time],
        )?;
        for (file, rev) in &m.files {
            tx.execute(
                "INSERT OR IGNORE INTO changeset_revs(changeset_id, file_id, revision_id) VALUES (?1, ?2, ?3)",
                params![m.id, file, rev],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn has_changeset(&self, id: &str) -> Result<bool> {
        Ok(self
            .conn()
            .query_row("SELECT 1 FROM changesets WHERE id = ?1", [id], |_| Ok(()))
            .optional()?
            .is_some())
    }

    /// The committed changeset that produced this exact revision, if any.
    pub fn changeset_for_revision(&self, file_id: &str, rev: &str) -> Result<Option<String>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT changeset_id FROM changeset_revs WHERE file_id = ?1 AND revision_id = ?2",
                [file_id, rev],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn changeset(&self, id: &str) -> Result<Option<Manifest>> {
        let conn = self.conn();
        let head = conn
            .query_row("SELECT id, author, message, created_time FROM changesets WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .optional()?;
        let Some((id, author, message, created_time)) = head else { return Ok(None) };
        let mut st = conn.prepare_cached("SELECT file_id, revision_id FROM changeset_revs WHERE changeset_id = ?1")?;
        let files = st
            .query_map([&id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(Some(Manifest { id, author, message, created_time, files }))
    }

    pub fn recent_changesets(&self, limit: u32) -> Result<Vec<Manifest>> {
        let ids: Vec<String> = {
            let conn = self.conn();
            let mut st = conn.prepare_cached("SELECT id FROM changesets ORDER BY created_time DESC LIMIT ?1")?;
            let rows = st.query_map([limit], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut out = Vec::new();
        for id in ids {
            out.extend(self.changeset(&id)?);
        }
        Ok(out)
    }

    // ---- journal ----

    pub fn put_journal(&self, changeset_id: &str, payload: &str) -> Result<()> {
        self.conn().execute(
            "INSERT INTO journal(changeset_id, payload, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(changeset_id) DO UPDATE SET payload = ?2, updated_at = ?3",
            params![changeset_id, payload, chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn journal(&self) -> Result<Vec<(String, String)>> {
        let conn = self.conn();
        let mut st = conn.prepare_cached("SELECT changeset_id, payload FROM journal ORDER BY updated_at")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn delete_journal(&self, changeset_id: &str) -> Result<()> {
        self.conn().execute("DELETE FROM journal WHERE changeset_id = ?1", [changeset_id])?;
        Ok(())
    }

    // ---- references ----

    pub fn set_refs(&self, from_id: &str, to_paths: &[String]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM refs WHERE from_id = ?1", [from_id])?;
        for p in to_paths {
            tx.execute("INSERT OR IGNORE INTO refs(from_id, to_path) VALUES (?1, ?2)", [from_id, p])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn refs(&self, from_id: &str) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut st = conn.prepare_cached("SELECT to_path FROM refs WHERE from_id = ?1")?;
        let rows = st.query_map([from_id], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn remote(id: &str, parent: &str, name: &str) -> RemoteFile {
        RemoteFile {
            id: id.into(),
            name: name.into(),
            parent: Some(parent.into()),
            mime_type: "application/octet-stream".into(),
            md5: Some("abc".into()),
            size: Some(3),
            head_revision_id: Some(format!("{id}-r1")),
            created_time: format!("2026-01-01T00:00:00Z{id}"),
            modified_time: "2026-01-01T00:00:00Z".into(),
            trashed: false,
            read_only: false,
            app_properties: BTreeMap::from([("k".into(), "v".into())]),
            last_modifier: None,
        }
    }

    #[test]
    fn round_trips_files_and_children() {
        let ix = Index::in_memory().unwrap();
        let mut folder = remote("d", "root", "Drivetrain");
        folder.mime_type = crate::drive::FOLDER_MIME.into();
        ix.apply_remote(&[remote("b", "root", "b.sldprt"), folder, remote("a", "root", "A.sldprt")], &[])
            .unwrap();
        let kids: Vec<_> = ix.children("root").unwrap().into_iter().map(|f| f.id).collect();
        assert_eq!(kids, ["d", "a", "b"], "folders first, then case-insensitive name");
        assert_eq!(ix.file("a").unwrap().unwrap(), remote("a", "root", "A.sldprt"));
        ix.apply_remote(&[], &["a".into()]).unwrap();
        assert!(ix.file("a").unwrap().is_none());
        assert_eq!(ix.file_count().unwrap(), 2);
    }

    #[test]
    fn stores_changesets_and_journal() {
        let ix = Index::in_memory().unwrap();
        let m = Manifest {
            id: "cs1".into(),
            author: "a@x".into(),
            message: "arm".into(),
            created_time: "2026-01-01T00:00:00Z".into(),
            files: BTreeMap::from([("f1".into(), "r2".into())]),
        };
        ix.insert_changeset(&m).unwrap();
        ix.insert_changeset(&m).unwrap(); // idempotent
        assert_eq!(ix.changeset("cs1").unwrap().unwrap(), m);
        assert_eq!(ix.changeset_for_revision("f1", "r2").unwrap().as_deref(), Some("cs1"));
        assert!(ix.changeset_for_revision("f1", "r1").unwrap().is_none());

        ix.put_journal("cs2", "{}").unwrap();
        assert_eq!(ix.journal().unwrap().len(), 1);
        ix.delete_journal("cs2").unwrap();
        assert!(ix.journal().unwrap().is_empty());
    }
}
