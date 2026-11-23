//! The aquasuite package database as an image source (design D2). The caller passes a file
//! descriptor. The daemon reads it into memory and opens it with SQLite from there, read-only, so
//! that it never opens a path and never changes the file or its directory.

use std::fs::File;
use std::io::{self, Read};
use std::os::unix::fs::FileExt;
use std::sync::Arc;
use std::time::Duration;

use nicecooling_protocol::firmware::bootloader::Language;
use rusqlite::config::DbConfig;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, MAIN_DB};
use tokio::sync::Semaphore;

/// The largest database file that the daemon reads.
pub const MAX_FILE_BYTES: u64 = 16 << 20;
/// The largest `data` of a firmware row.
pub const MAX_DATA_BYTES: i64 = 4 << 20;
/// The time that one parse may take. A descriptor on a FUSE mount can block the read.
pub const READ_LIMIT: Duration = Duration::from_secs(10);

const DEVICES: [&str; 2] = ["leakshield_EN", "leakshield_DE"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageError {
    /// The descriptor or the database is not what the call needs.
    InvalidArgument(String),
    /// Another parse runs.
    Busy,
    /// The database has no such Leakshield row.
    NotFound,
}

impl std::fmt::Display for PackageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidArgument(why) => write!(f, "{why}"),
            Self::Busy => write!(f, "another database is being read"),
            Self::NotFound => write!(f, "the database has no such image"),
        }
    }
}

impl std::error::Error for PackageError {}

fn invalid(why: impl Into<String>) -> PackageError {
    PackageError::InvalidArgument(why.into())
}

fn sql(e: rusqlite::Error) -> PackageError {
    invalid(format!("the database is not readable: {e}"))
}

/// One Leakshield row of the table `firmware`, without its data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRow {
    pub id: i64,
    pub language: Language,
    pub version: u16,
    pub bootloader: u16,
    pub hardware: u16,
}

/// The bytes of one database and where to read them. A file reads at offset 0 with `read_at`, so
/// the offset of the descriptor that the caller shares does not move.
pub struct Source {
    reader: Box<dyn Read + Send>,
    len: u64,
}

struct FileReader {
    file: File,
    offset: u64,
}

impl Read for FileReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.file.read_at(buf, self.offset)?;
        self.offset += n as u64;
        Ok(n)
    }
}

impl Source {
    pub fn new(reader: impl Read + Send + 'static, len: u64) -> Self {
        Self {
            reader: Box::new(reader),
            len,
        }
    }

    /// Checks the descriptor: a regular file of 1 byte to 16 MiB.
    pub fn from_file(file: File) -> Result<Self, PackageError> {
        let meta = file
            .metadata()
            .map_err(|e| invalid(format!("the descriptor cannot be read: {e}")))?;
        if !meta.is_file() {
            return Err(invalid("the descriptor is not a regular file"));
        }
        if meta.len() == 0 || meta.len() > MAX_FILE_BYTES {
            return Err(invalid(format!(
                "the file has {} bytes, the limit is {MAX_FILE_BYTES}",
                meta.len()
            )));
        }
        Ok(Self::new(FileReader { file, offset: 0 }, meta.len()))
    }
}

/// Opens the database from memory: read-only, with `trusted_schema` off and the defensive mode.
/// It refuses a file that has no table `firmware`. A view of that name is not a table.
fn open(source: Source) -> Result<Connection, PackageError> {
    let mut conn = Connection::open_in_memory().map_err(sql)?;
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false)
        .map_err(sql)?;
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
        .map_err(sql)?;
    let len = usize::try_from(source.len).map_err(|_| invalid("the file is too large"))?;
    conn.deserialize_read_exact(MAIN_DB, source.reader, len, true)
        .map_err(sql)?;
    let kind: Option<String> = conn
        .query_row(
            "SELECT type FROM sqlite_schema WHERE name = 'firmware'",
            [],
            |row| row.get(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            e => Err(sql(e)),
        })?;
    if kind.as_deref() != Some("table") {
        return Err(invalid("the database has no table firmware"));
    }
    Ok(conn)
}

fn row_of(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<ImageRow, PackageError>> {
    let id: i64 = row.get(0)?;
    let number = |at: usize| -> rusqlite::Result<Option<u16>> {
        Ok(row
            .get::<_, Option<i64>>(at)?
            .and_then(|v| u16::try_from(v).ok()))
    };
    let device: String = row.get(4)?;
    let language = if device == DEVICES[0] {
        Language::English
    } else {
        Language::German
    };
    Ok(match (number(1)?, number(2)?, number(3)?) {
        (Some(version), Some(bootloader), Some(hardware)) => Ok(ImageRow {
            id,
            language,
            version,
            bootloader,
            hardware,
        }),
        _ => Err(invalid(format!("the row {id} has a value out of range"))),
    })
}

const COLUMNS: &str = "SELECT id, version, bootloader, hardware, device FROM firmware \
                       WHERE device IN (?1, ?2)";

fn rows(conn: &Connection) -> Result<Vec<ImageRow>, PackageError> {
    let mut statement = conn
        .prepare(&format!("{COLUMNS} ORDER BY id"))
        .map_err(sql)?;
    let rows = statement
        .query_map(DEVICES, row_of)
        .map_err(sql)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql)?;
    rows.into_iter().collect()
}

/// The row `id` and its data, as the bytes of a BLOB or a TEXT value.
fn read_row(conn: &Connection, id: i64) -> Result<(ImageRow, Vec<u8>), PackageError> {
    let row = conn
        .query_row(
            &format!("{COLUMNS} AND id = ?3"),
            rusqlite::params![DEVICES[0], DEVICES[1], id],
            row_of,
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => PackageError::NotFound,
            e => sql(e),
        })??;
    let size: Option<i64> = conn
        .query_row(
            "SELECT octet_length(data) FROM firmware WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if size.unwrap_or(0) > MAX_DATA_BYTES {
        return Err(invalid(format!(
            "the data of the row {id} is above {MAX_DATA_BYTES} bytes"
        )));
    }
    let data = conn
        .query_row("SELECT data FROM firmware WHERE id = ?1", [id], |r| {
            Ok(match r.get_ref(0)? {
                ValueRef::Blob(bytes) => Some(bytes.to_vec()),
                ValueRef::Text(bytes) => Some(bytes.to_vec()),
                _ => None,
            })
        })
        .map_err(sql)?
        .ok_or_else(|| invalid(format!("the data of the row {id} is neither BLOB nor TEXT")))?;
    Ok((row, data))
}

/// Parses one database at a time. A call that finds a parse running returns `Busy`. A read that
/// outlasts the limit returns `InvalidArgument`, and its thread keeps the slot until it ends.
#[derive(Clone)]
pub struct PackageReader {
    slot: Arc<Semaphore>,
    limit: Duration,
}

impl Default for PackageReader {
    fn default() -> Self {
        Self::with_limit(READ_LIMIT)
    }
}

impl PackageReader {
    pub fn with_limit(limit: Duration) -> Self {
        Self {
            slot: Arc::new(Semaphore::new(1)),
            limit,
        }
    }

    /// The Leakshield rows, ordered by id.
    pub async fn list(&self, file: File) -> Result<Vec<ImageRow>, PackageError> {
        self.list_source(Source::from_file(file)?).await
    }

    /// One Leakshield row with its data.
    pub async fn read(&self, file: File, id: i64) -> Result<(ImageRow, Vec<u8>), PackageError> {
        self.read_source(Source::from_file(file)?, id).await
    }

    pub async fn list_source(&self, source: Source) -> Result<Vec<ImageRow>, PackageError> {
        self.run(source, |conn| rows(&conn)).await
    }

    pub async fn read_source(
        &self,
        source: Source,
        id: i64,
    ) -> Result<(ImageRow, Vec<u8>), PackageError> {
        self.run(source, move |conn| read_row(&conn, id)).await
    }

    async fn run<T: Send + 'static>(
        &self,
        source: Source,
        job: impl FnOnce(Connection) -> Result<T, PackageError> + Send + 'static,
    ) -> Result<T, PackageError> {
        let permit = self
            .slot
            .clone()
            .try_acquire_owned()
            .map_err(|_| PackageError::Busy)?;
        let task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            job(open(source)?)
        });
        match tokio::time::timeout(self.limit, task).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(invalid("the database parse failed")),
            Err(_) => Err(invalid("the read of the database took too long")),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Cursor;
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;

    use rusqlite::types::Value;
    use sha2::{Digest, Sha256};

    use super::*;

    /// A directory in the temporary directory that goes away at the end of the test.
    struct TempDir(PathBuf);

    impl std::ops::Deref for TempDir {
        type Target = Path;

        fn deref(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn dir(name: &str) -> TempDir {
        let dir =
            std::env::temp_dir().join(format!("nicecooling-package-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    struct Row(i64, i64, i64, i64, &'static str, Value);

    fn blob(bytes: &[u8]) -> Value {
        Value::Blob(bytes.to_vec())
    }

    /// The table of the aquasuite package, as the notes describe it (`docs/re/firmware.md`).
    fn database(dir: &Path, rows: &[Row]) -> PathBuf {
        let path = dir.join("package.database");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE firmware (id INTEGER PRIMARY KEY, version INTEGER, bootloader INTEGER, \
             hardware INTEGER, device TEXT, data BLOB)",
        )
        .unwrap();
        for Row(id, version, bootloader, hardware, device, data) in rows {
            conn.execute(
                "INSERT INTO firmware VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![id, version, bootloader, hardware, device, data],
            )
            .unwrap();
        }
        path
    }

    fn leakshield_rows() -> Vec<Row> {
        vec![
            Row(26, 1022, 100, 1000, "leakshield_EN", blob(b"english image")),
            Row(27, 1022, 100, 1000, "leakshield_DE", blob(b"german image")),
        ]
    }

    fn open_file(path: &Path) -> File {
        File::open(path).unwrap()
    }

    #[tokio::test]
    async fn lists_en_and_de_rows() {
        let dir = dir("list");
        let path = database(&dir, &leakshield_rows());
        let rows = PackageReader::default()
            .list(open_file(&path))
            .await
            .unwrap();
        assert_eq!(
            rows,
            [
                ImageRow {
                    id: 26,
                    language: Language::English,
                    version: 1022,
                    bootloader: 100,
                    hardware: 1000
                },
                ImageRow {
                    id: 27,
                    language: Language::German,
                    version: 1022,
                    bootloader: 100,
                    hardware: 1000
                },
            ]
        );
    }

    #[tokio::test]
    async fn ignores_other_devices() {
        let dir = dir("others");
        let mut rows = leakshield_rows();
        rows.push(Row(1, 5, 5, 5, "octo_EN", blob(b"x")));
        rows.push(Row(2, 5, 5, 5, "leakshield_FR", blob(b"x")));
        rows.push(Row(3, 5, 5, 5, "LEAKSHIELD_EN", blob(b"x")));
        let path = database(&dir, &rows);
        let listed = PackageReader::default()
            .list(open_file(&path))
            .await
            .unwrap();
        assert_eq!(listed.iter().map(|r| r.id).collect::<Vec<_>>(), [26, 27]);
        let other = PackageReader::default().read(open_file(&path), 1).await;
        assert_eq!(other, Err(PackageError::NotFound));
    }

    #[tokio::test]
    async fn reads_data_of_one_row() {
        let dir = dir("data");
        let path = database(&dir, &leakshield_rows());
        let (row, data) = PackageReader::default()
            .read(open_file(&path), 27)
            .await
            .unwrap();
        assert_eq!(row.language, Language::German);
        assert_eq!(data, b"german image");
        assert_eq!(
            PackageReader::default().read(open_file(&path), 99).await,
            Err(PackageError::NotFound)
        );
    }

    #[tokio::test]
    async fn reads_text_data() {
        let dir = dir("text");
        let rows = vec![Row(
            26,
            1022,
            100,
            1000,
            "leakshield_EN",
            Value::Text("\u{feff}<xml/>".into()),
        )];
        let path = database(&dir, &rows);
        let (_, data) = PackageReader::default()
            .read(open_file(&path), 26)
            .await
            .unwrap();
        assert_eq!(data, "\u{feff}<xml/>".as_bytes());
    }

    fn listing(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[tokio::test]
    async fn database_bytes_unchanged_after_read() {
        let dir = dir("unchanged");
        let path = database(&dir, &leakshield_rows());
        let state = |p: &Path| {
            let meta = fs::metadata(p).unwrap();
            (
                meta.len(),
                meta.modified().unwrap(),
                Sha256::digest(fs::read(p).unwrap()),
            )
        };
        let before = state(&path);
        let reader = PackageReader::default();
        reader.list(open_file(&path)).await.unwrap();
        reader.read(open_file(&path), 26).await.unwrap();
        assert_eq!(state(&path), before);
    }

    #[tokio::test]
    async fn no_new_file_in_the_directory() {
        let dir = dir("nofile");
        let path = database(&dir, &leakshield_rows());
        let before = listing(&dir);
        let reader = PackageReader::default();
        reader.list(open_file(&path)).await.unwrap();
        reader.read(open_file(&path), 26).await.unwrap();
        assert_eq!(listing(&dir), before);
    }

    #[tokio::test]
    async fn the_descriptor_offset_does_not_move() {
        let dir = dir("offset");
        let path = database(&dir, &leakshield_rows());
        let mut file = open_file(&path);
        let mut head = [0u8; 16];
        file.read_exact(&mut head).unwrap();
        let reader = PackageReader::default();
        assert_eq!(
            reader.list(file.try_clone().unwrap()).await.unwrap().len(),
            2
        );
        let mut next = [0u8; 16];
        file.read_exact(&mut next).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_eq!(next, bytes[16..32]);
    }

    #[tokio::test]
    async fn refuses_fifo_descriptor() {
        let dir = dir("fifo");
        let path = dir.join("pipe");
        rustix::fs::mknodat(
            rustix::fs::CWD,
            &path,
            rustix::fs::FileType::Fifo,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
            0,
        )
        .unwrap();
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path)
            .unwrap();
        let result = PackageReader::default().list(file).await;
        assert!(
            matches!(result, Err(PackageError::InvalidArgument(_))),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn refuses_file_above_16_mib() {
        let dir = dir("big");
        let path = dir.join("big.database");
        let file = File::create(&path).unwrap();
        file.set_len(MAX_FILE_BYTES + 1).unwrap();
        let result = PackageReader::default().list(open_file(&path)).await;
        assert!(
            matches!(result, Err(PackageError::InvalidArgument(_))),
            "{result:?}"
        );
        file.set_len(0).unwrap();
        let empty = PackageReader::default().list(open_file(&path)).await;
        assert!(matches!(empty, Err(PackageError::InvalidArgument(_))));
    }

    #[tokio::test]
    async fn refuses_data_above_4_mib() {
        let dir = dir("bigdata");
        let rows = vec![
            Row(
                26,
                1022,
                100,
                1000,
                "leakshield_EN",
                blob(&vec![7u8; MAX_DATA_BYTES as usize + 1]),
            ),
            Row(
                27,
                1022,
                100,
                1000,
                "leakshield_DE",
                blob(&vec![7u8; MAX_DATA_BYTES as usize]),
            ),
        ];
        let path = database(&dir, &rows);
        let reader = PackageReader::default();
        let refused = reader.read(open_file(&path), 26).await;
        assert!(
            matches!(refused, Err(PackageError::InvalidArgument(_))),
            "{refused:?}"
        );
        let (_, data) = reader.read(open_file(&path), 27).await.unwrap();
        assert_eq!(data.len(), MAX_DATA_BYTES as usize);
        assert_eq!(reader.list(open_file(&path)).await.unwrap().len(), 2);
    }

    fn plain(dir: &Path, sql: &str) -> PathBuf {
        let path = dir.join("other.database");
        Connection::open(&path).unwrap().execute_batch(sql).unwrap();
        path
    }

    #[tokio::test]
    async fn refuses_firmware_view() {
        let dir = dir("view");
        let path = plain(
            &dir,
            "CREATE TABLE base (id INTEGER PRIMARY KEY, version INTEGER, bootloader INTEGER, \
             hardware INTEGER, device TEXT, data BLOB);\
             INSERT INTO base VALUES (26, 1022, 100, 1000, 'leakshield_EN', x'00');\
             CREATE VIEW firmware AS SELECT * FROM base;",
        );
        let result = PackageReader::default().list(open_file(&path)).await;
        assert!(
            matches!(result, Err(PackageError::InvalidArgument(_))),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn refuses_file_without_firmware_table() {
        let dir = dir("notable");
        let path = plain(&dir, "CREATE TABLE other (id INTEGER)");
        let reader = PackageReader::default();
        let result = reader.list(open_file(&path)).await;
        assert!(
            matches!(result, Err(PackageError::InvalidArgument(_))),
            "{result:?}"
        );
        let text = dir.join("text.database");
        fs::write(&text, b"this is not a database at all").unwrap();
        assert!(matches!(
            reader.list(open_file(&text)).await,
            Err(PackageError::InvalidArgument(_))
        ));
    }

    fn connection(dir: &Path) -> Connection {
        let path = database(dir, &leakshield_rows());
        open(Source::from_file(open_file(&path)).unwrap()).unwrap()
    }

    #[test]
    fn trusted_schema_is_off() {
        let conn = connection(&dir("trusted"));
        assert!(
            !conn
                .db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA)
                .unwrap()
        );
    }

    #[test]
    fn defensive_mode_is_on() {
        let conn = connection(&dir("defensive"));
        assert!(conn.db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE).unwrap());
        assert!(
            conn.execute("DELETE FROM firmware", []).is_err(),
            "the database is read-only"
        );
    }

    /// Blocks its first read until the test lets it go.
    struct Gated {
        release: mpsc::Receiver<()>,
        inner: Cursor<Vec<u8>>,
    }

    impl Read for Gated {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.inner.position() == 0 {
                let _ = self.release.recv();
            }
            self.inner.read(buf)
        }
    }

    fn bytes_of(dir: &Path) -> Vec<u8> {
        fs::read(database(dir, &leakshield_rows())).unwrap()
    }

    async fn wait_until_busy(reader: &PackageReader) {
        while reader.slot.available_permits() > 0 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }

    #[tokio::test]
    async fn second_parse_returns_busy() {
        let dir = dir("busy");
        let bytes = bytes_of(&dir);
        let path = dir.join("package.database");
        let reader = PackageReader::default();
        let (release, gate) = mpsc::channel();
        let slow = Source::new(
            Gated {
                release: gate,
                inner: Cursor::new(bytes.clone()),
            },
            bytes.len() as u64,
        );
        let first = tokio::spawn({
            let reader = reader.clone();
            async move { reader.list_source(slow).await }
        });
        wait_until_busy(&reader).await;
        assert_eq!(reader.list(open_file(&path)).await, Err(PackageError::Busy));
        assert_eq!(
            reader.read(open_file(&path), 26).await,
            Err(PackageError::Busy)
        );
        release.send(()).unwrap();
        assert_eq!(first.await.unwrap().unwrap().len(), 2);
        assert_eq!(reader.list(open_file(&path)).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn slow_read_times_out() {
        let dir = dir("slow");
        let bytes = bytes_of(&dir);
        let path = dir.join("package.database");
        let reader = PackageReader::with_limit(Duration::from_millis(50));
        let (release, gate) = mpsc::channel();
        let slow = Source::new(
            Gated {
                release: gate,
                inner: Cursor::new(bytes.clone()),
            },
            bytes.len() as u64,
        );
        let result = reader.list_source(slow).await;
        assert!(
            matches!(result, Err(PackageError::InvalidArgument(_))),
            "{result:?}"
        );
        // The thread still blocks in its read and holds the slot.
        assert_eq!(reader.list(open_file(&path)).await, Err(PackageError::Busy));
        release.send(()).unwrap();
        while reader.slot.available_permits() == 0 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        assert_eq!(reader.list(open_file(&path)).await.unwrap().len(), 2);
    }
}
