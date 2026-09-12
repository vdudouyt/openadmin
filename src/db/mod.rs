//! SQLCipher-backed storage, schema-compatible with qhostman
//! (`/root/qhostman/gui/database.cpp`): same DDL, same queries, same column
//! order, so an existing `~/.qhostman/qhostman.sqlite` opens as-is.
//!
//! Two deliberate divergences, both additive:
//!   * `save()` persists `key_name`/`key_value`, which qhostman silently
//!     dropped on every edit.
//!   * `migrate()` adds a `proxy` column, which qhostman kept as runtime-only
//!     plugin state.

pub mod model;

use anyhow::{Context, Result, anyhow};
use model::HostRecord;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

/// The DDL qhostman's `DataBase::create()` writes, verbatim.
const SCHEMA: &str = "\
CREATE TABLE hosts (id integer primary key autoincrement, host_name varchar(32) not null, \
proto varchar(7) not null, addr varchar(64) not null, mount_point varchar(64) not null, \
port int, login varchar(64), pass varchar(64), id_path varchar(128), key_name varchar(64), \
key_value blob, key_data blob);
CREATE TABLE misc (name varchar(32) primary key, value text);";

/// Columns every read selects, in qhostman's order plus our additions.
const COLUMNS: &str =
    "id,host_name,proto,addr,mount_point,port,login,pass,key_name,key_value,proxy";

pub struct DataBase {
    conn: Option<Connection>,
    filename: PathBuf,
}

impl DataBase {
    pub fn new(filename: impl Into<PathBuf>) -> Self {
        DataBase {
            conn: None,
            filename: filename.into(),
        }
    }

    pub fn exists(&self) -> bool {
        Path::new(&self.filename).exists()
    }

    /// Open + key + decryption test. Returns `Ok(false)` (dropping the
    /// connection) when the password does not decrypt the file — the same
    /// probe qhostman uses: any statement touching `sqlite_master` fails to
    /// prepare under a wrong key.
    pub fn open(&mut self, password: &str) -> Result<bool> {
        let conn = Connection::open(&self.filename).context("open database file")?;
        if conn.pragma_update(None, "key", password).is_err() {
            return Ok(false);
        }
        if conn.prepare("SELECT * FROM sqlite_master").is_err() {
            return Ok(false);
        }
        self.conn = Some(conn);
        self.migrate()?;
        Ok(true)
    }

    /// Create a fresh database with qhostman's exact schema.
    pub fn create(&mut self, new_password: &str) -> Result<()> {
        if let Some(dir) = self.filename.parent() {
            std::fs::create_dir_all(dir).context("create data directory")?;
        }
        let conn = Connection::open(&self.filename).context("create database file")?;
        conn.pragma_update(None, "key", new_password)
            .context("set encryption key")?;
        conn.execute_batch(SCHEMA).context("initialize schema")?;
        self.conn = Some(conn);
        self.restrict_permissions();
        self.migrate()
    }

    /// The database holds host passwords in cleartext, so the file itself is
    /// the security boundary: owner-only.
    fn restrict_permissions(&self) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ =
                std::fs::set_permissions(&self.filename, std::fs::Permissions::from_mode(0o600));
        }
    }

    /// Idempotent additive migration: introspect `pragma table_info(hosts)` and
    /// add what is missing. Same approach as qhostman's p2p plugin
    /// (`databasewrapper.cpp:checkSchema`), which is the only migration pattern
    /// the original has.
    fn migrate(&self) -> Result<()> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("pragma table_info(hosts)")?;
        let cols: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(Result::ok)
            .collect();
        if !cols.iter().any(|c| c == "proxy") {
            conn.execute("ALTER TABLE hosts ADD COLUMN proxy INTEGER DEFAULT 0", [])
                .context("add proxy column")?;
        }
        Ok(())
    }

    fn conn(&self) -> Result<&Connection> {
        self.conn
            .as_ref()
            .ok_or_else(|| anyhow!("database is not open"))
    }

    /// `%query%` LIKE over host_name/addr ordered by id — qhostman's
    /// `DataBase::search`. An empty query returns every host.
    pub fn search(&self, query: &str) -> Result<Vec<HostRecord>> {
        let pattern = format!("%{}%", query);
        let sql = format!(
            "SELECT {COLUMNS} FROM hosts WHERE host_name LIKE ? OR addr LIKE ? ORDER BY id"
        );
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([&pattern, &pattern], parse_row)?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// INSERT or UPDATE depending on `rec.id`, returning the record's id.
    pub fn save(&self, rec: &HostRecord) -> Result<i64> {
        let conn = self.conn()?;
        if rec.id != 0 {
            conn.execute(
                "UPDATE hosts SET host_name=?,proto=?,addr=?,mount_point=?,port=?,login=?,\
                 pass=?,key_name=?,key_value=?,proxy=? WHERE id=?",
                rusqlite::params![
                    rec.name,
                    rec.proto,
                    rec.addr,
                    rec.mount_point,
                    rec.port,
                    rec.login,
                    rec.pass,
                    rec.key_name,
                    rec.key_value,
                    rec.proxy as i64,
                    rec.id
                ],
            )?;
            Ok(rec.id)
        } else {
            conn.execute(
                "INSERT INTO hosts (host_name,proto,addr,mount_point,port,login,pass,\
                 key_name,key_value,proxy) VALUES (?,?,?,?,?,?,?,?,?,?)",
                rusqlite::params![
                    rec.name,
                    rec.proto,
                    rec.addr,
                    rec.mount_point,
                    rec.port,
                    rec.login,
                    rec.pass,
                    rec.key_name,
                    rec.key_value,
                    rec.proxy as i64
                ],
            )?;
            Ok(conn.last_insert_rowid())
        }
    }

    pub fn remove(&self, id: i64) -> Result<()> {
        self.conn()?.execute("DELETE FROM hosts WHERE id=?", [id])?;
        Ok(())
    }

    /// Clear the proxy flag on every host except `keep` — only one host routes
    /// traffic at a time, matching qhostman's single-proxy model.
    pub fn clear_proxy_except(&self, keep: i64) -> Result<()> {
        self.conn()?
            .execute("UPDATE hosts SET proxy=0 WHERE id<>?", [keep])?;
        Ok(())
    }

    /// The `misc` key/value table qhostman uses for app settings. Kept as
    /// part of the compatible surface; the agent screen will need it.
    #[allow(dead_code)]
    pub fn load_option(&self, name: &str) -> Result<Option<String>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT value FROM misc WHERE name=?")?;
        let mut rows = stmt.query([name])?;
        Ok(match rows.next()? {
            Some(row) => row.get::<_, Option<String>>(0)?,
            None => None,
        })
    }

    #[allow(dead_code)]
    pub fn save_option(&self, name: &str, value: &str) -> Result<()> {
        self.conn()?.execute(
            "INSERT OR REPLACE INTO misc (name, value) VALUES (?,?)",
            [name, value],
        )?;
        Ok(())
    }
}

fn parse_row(row: &rusqlite::Row) -> rusqlite::Result<HostRecord> {
    // Text columns may be NULL in databases written by qhostman, which funnels
    // them all through a stringwrapper() -> "".
    let text = |i: usize| -> String {
        row.get::<_, Option<String>>(i)
            .unwrap_or(None)
            .unwrap_or_default()
    };
    Ok(HostRecord {
        id: row.get::<_, Option<i64>>(0)?.unwrap_or(0),
        name: text(1),
        proto: text(2),
        addr: text(3),
        mount_point: text(4),
        port: row.get::<_, Option<i64>>(5)?.unwrap_or(0),
        login: text(6),
        pass: text(7),
        key_name: text(8),
        key_value: text(9),
        proxy: row.get::<_, Option<i64>>(10)?.unwrap_or(0) != 0,
        mounted: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("openadmin-test-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("openadmin.sqlite")
    }

    #[test]
    fn crud_roundtrip_and_wrong_password_is_rejected() {
        let path = tmp("crud");
        let mut db = DataBase::new(&path);
        assert!(!db.exists());
        db.create("secret123").unwrap();
        assert!(db.exists());

        let rec = HostRecord {
            name: "web-01".into(),
            proto: "ssh".into(),
            addr: "10.0.0.1".into(),
            mount_point: "/net/web-01".into(),
            port: 22,
            login: "deploy".into(),
            pass: "pw".into(),
            ..Default::default()
        };
        let id = db.save(&rec).unwrap();
        assert!(id > 0);

        let found = db.search("web").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "web-01");
        assert_eq!(found[0].port, 22);
        assert!(!found[0].proxy);

        let mut db2 = DataBase::new(&path);
        assert!(!db2.open("wrong").unwrap(), "wrong key must not decrypt");
        let mut db3 = DataBase::new(&path);
        assert!(db3.open("secret123").unwrap());
        assert_eq!(db3.search("").unwrap().len(), 1);

        db.remove(id).unwrap();
        assert!(db.search("").unwrap().is_empty());
    }

    /// qhostman's save() wrote only 7 columns, wiping key_name/key_value on
    /// every edit. Ours must round-trip them.
    #[test]
    fn save_preserves_key_fields_and_proxy() {
        let path = tmp("keys");
        let mut db = DataBase::new(&path);
        db.create("pw123456").unwrap();

        let mut rec = HostRecord {
            name: "bastion".into(),
            proto: "ssh".into(),
            addr: "edge.corp.net".into(),
            port: 2222,
            key_name: "bastion".into(),
            key_value: "passphrase".into(),
            proxy: true,
            ..Default::default()
        };
        rec.id = db.save(&rec).unwrap();

        // Edit an unrelated field and re-save, as the Edit dialog would.
        rec.login = "jump".into();
        db.save(&rec).unwrap();

        let got = &db.search("bastion").unwrap()[0];
        assert_eq!(got.key_name, "bastion");
        assert_eq!(got.key_value, "passphrase");
        assert!(got.proxy);
        assert_eq!(got.login, "jump");
    }

    #[test]
    fn migration_is_idempotent_and_upgrades_a_qhostman_database() {
        let path = tmp("migrate");
        // Write a database with qhostman's original schema — no proxy column.
        {
            let conn = Connection::open(&path).unwrap();
            conn.pragma_update(None, "key", "legacy").unwrap();
            conn.execute_batch(SCHEMA).unwrap();
            conn.execute(
                "INSERT INTO hosts (host_name,proto,addr,mount_point,port,login,pass) \
                 VALUES ('old','ssh','10.0.0.9','/net/old',22,'root','toor')",
                [],
            )
            .unwrap();
        }
        // Opening it migrates in place and the legacy row survives.
        let mut db = DataBase::new(&path);
        assert!(db.open("legacy").unwrap());
        let rows = db.search("").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "old");
        assert!(!rows[0].proxy, "migrated rows default to not-proxy");

        // Re-opening runs migrate() again; it must be a no-op, not an error.
        let mut again = DataBase::new(&path);
        assert!(again.open("legacy").unwrap());
        assert_eq!(again.search("").unwrap().len(), 1);
    }

    #[test]
    fn misc_options_roundtrip() {
        let path = tmp("misc");
        let mut db = DataBase::new(&path);
        db.create("pw123456").unwrap();
        assert_eq!(db.load_option("nope").unwrap(), None);
        db.save_option("model", "claude-sonnet-4.5").unwrap();
        db.save_option("model", "claude-opus-5").unwrap();
        assert_eq!(
            db.load_option("model").unwrap().as_deref(),
            Some("claude-opus-5")
        );
    }
}
