//! The aquasuite package database as an image source (design D2). The caller passes a file
//! descriptor. The daemon reads it into memory and opens it with SQLite from there, read-only, so
//! that it never opens a path and never changes the file or its directory.

use std::fs::File;
use std::io::{self, Read};
use std::os::unix::fs::FileExt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use nicecooling_protocol::firmware::bootloader::Language;
use rusqlite::config::DbConfig;
use rusqlite::limits::Limit;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, MAIN_DB};
use tokio::sync::Semaphore;

/// The largest database file that the daemon reads.
pub const MAX_FILE_BYTES: u64 = 16 << 20;
/// The largest `data` of a firmware row.
pub const MAX_DATA_BYTES: i64 = 4 << 20;
/// The time that one parse may take. A descriptor on a FUSE mount can block the read.
pub const READ_LIMIT: Duration = Duration::from_secs(10);

/// The largest string or BLOB that SQLite builds, also inside an expression of the schema. It
/// holds a row of `MAX_DATA_BYTES` read as UTF-8 from a UTF-16 database (1.5 times more).
const MAX_VALUE_BYTES: i32 = 8 << 20;
/// SQLite calls the progress handler after this many virtual machine instructions.
const PROGRESS_STEP: i32 = 1000;

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
    /// `len` is the number of bytes that the reader gives: 1 byte to 16 MiB.
    pub(crate) fn new(reader: impl Read + Send + 'static, len: u64) -> Result<Self, PackageError> {
        if len == 0 || len > MAX_FILE_BYTES {
            return Err(invalid(format!(
                "the database has {len} bytes, the limit is {MAX_FILE_BYTES}"
            )));
        }
        Ok(Self {
            reader: Box::new(reader),
            len,
        })
    }

    /// Checks the descriptor: a regular file of 1 byte to 16 MiB.
    pub fn from_file(file: File) -> Result<Self, PackageError> {
        let meta = file
            .metadata()
            .map_err(|e| invalid(format!("the descriptor cannot be read: {e}")))?;
        if !meta.is_file() {
            return Err(invalid("the descriptor is not a regular file"));
        }
        Self::new(FileReader { file, offset: 0 }, meta.len())
    }
}

/// Opens the database from memory: read-only, with `trusted_schema` off and the defensive mode.
/// A value above `MAX_VALUE_BYTES` fails, and so does a statement that runs after `deadline`: the
/// thread then ends and frees the slot. It refuses a file that has no plain table `firmware`.
fn open(source: Source, deadline: Instant) -> Result<Connection, PackageError> {
    let mut conn = Connection::open_in_memory().map_err(sql)?;
    conn.set_limit(Limit::SQLITE_LIMIT_LENGTH, MAX_VALUE_BYTES)
        .map_err(sql)?;
    conn.progress_handler(PROGRESS_STEP, Some(move || Instant::now() >= deadline))
        .map_err(sql)?;
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false)
        .map_err(sql)?;
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
        .map_err(sql)?;
    let len = usize::try_from(source.len).map_err(|_| invalid("the file is too large"))?;
    conn.deserialize_read_exact(MAIN_DB, source.reader, len, true)
        .map_err(sql)?;
    check_table(&conn)?;
    check_columns(&conn)?;
    Ok(conn)
}

/// `firmware` must be a plain table. SQLite builds the schema from the `sql` column, so that column
/// decides: a view and a virtual table have another statement, whatever the `type` column says.
fn check_table(conn: &Connection) -> Result<(), PackageError> {
    let mut statement = conn
        .prepare("SELECT type, sql FROM sqlite_schema WHERE name = 'firmware'")
        .map_err(sql)?;
    let entries = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(sql)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql)?;
    match entries.as_slice() {
        [(kind, Some(statement))]
            if kind == "table"
                && statement
                    .get(..12)
                    .is_some_and(|head| head.eq_ignore_ascii_case("CREATE TABLE")) =>
        {
            Ok(())
        }
        _ => Err(invalid("the database has no plain table firmware")),
    }
}

/// No column of `firmware` may be generated or hidden: SQLite evaluates the expression of a
/// generated column for every row, and the expression is code of the file.
fn check_columns(conn: &Connection) -> Result<(), PackageError> {
    let mut statement = conn
        .prepare("SELECT name, hidden FROM pragma_table_xinfo('firmware')")
        .map_err(sql)?;
    let columns = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(sql)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql)?;
    match columns.iter().find(|(_, hidden)| *hidden != 0) {
        Some((name, _)) => Err(invalid(format!("the column {name} is generated or hidden"))),
        None => Ok(()),
    }
}

fn row_of(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<ImageRow, PackageError>> {
    let id: i64 = row.get(0)?;
    let number = |at: usize| -> rusqlite::Result<Option<u16>> {
        Ok(row
            .get::<_, Option<i64>>(at)?
            .and_then(|v| u16::try_from(v).ok()))
    };
    let device: String = row.get(4)?;
    // The query compares the bytes, and so does this match: no other value names a Leakshield.
    let language = match device.as_str() {
        d if d == DEVICES[0] => Language::English,
        d if d == DEVICES[1] => Language::German,
        _ => return Ok(Err(PackageError::NotFound)),
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

/// The comparison ignores the collation that the file gives to the column `device`.
const COLUMNS: &str = "SELECT id, version, bootloader, hardware, device FROM firmware \
                       WHERE device COLLATE BINARY IN (?1, ?2)";

fn rows(conn: &Connection) -> Result<Vec<ImageRow>, PackageError> {
    let mut statement = conn
        .prepare(&format!("{COLUMNS} ORDER BY id"))
        .map_err(sql)?;
    let rows = statement
        .query_map(DEVICES, row_of)
        .map_err(sql)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql)?;
    rows.into_iter()
        .filter(|row| !matches!(row, Err(PackageError::NotFound)))
        .collect()
}

/// The row `id` and its data, as the bytes of a BLOB or a TEXT value. The identity, the size and
/// the data come from one filter, so the data is of the row that the checks see. Two rows that
/// match are refused: the column `id` of a hostile file need not be unique.
fn read_row(conn: &Connection, id: i64) -> Result<(ImageRow, Vec<u8>), PackageError> {
    let params = rusqlite::params![DEVICES[0], DEVICES[1], id];
    let matching: i64 = conn
        .query_row(
            "SELECT count(*) FROM firmware WHERE device COLLATE BINARY IN (?1, ?2) AND id = ?3",
            params,
            |r| r.get(0),
        )
        .map_err(sql)?;
    match matching {
        0 => return Err(PackageError::NotFound),
        1 => {}
        n => return Err(invalid(format!("{n} Leakshield rows have the id {id}"))),
    }
    conn.query_row(
        "SELECT id, version, bootloader, hardware, device, octet_length(data), data FROM firmware \
         WHERE device COLLATE BINARY IN (?1, ?2) AND id = ?3",
        params,
        |row| {
            let image = match row_of(row)? {
                Ok(image) => image,
                Err(e) => return Ok(Err(e)),
            };
            let size: Option<i64> = row.get(5)?;
            if size.unwrap_or(0) > MAX_DATA_BYTES {
                return Ok(Err(invalid(format!(
                    "the data of the row {id} is above {MAX_DATA_BYTES} bytes"
                ))));
            }
            // `octet_length` counts bytes of the database encoding. The bytes of a TEXT value
            // come back as UTF-8, which needs up to 1.5 times more than UTF-16.
            Ok(match row.get_ref(6)? {
                ValueRef::Blob(bytes) | ValueRef::Text(bytes)
                    if bytes.len() as i64 > MAX_DATA_BYTES =>
                {
                    Err(invalid(format!(
                        "the data of the row {id} is above {MAX_DATA_BYTES} bytes"
                    )))
                }
                ValueRef::Blob(bytes) | ValueRef::Text(bytes) => Ok((image, bytes.to_vec())),
                _ => Err(invalid(format!(
                    "the data of the row {id} is neither BLOB nor TEXT"
                ))),
            })
        },
    )
    .map_err(sql)?
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
        self.run(move || Source::from_file(file), |conn| rows(&conn))
            .await
    }

    /// One Leakshield row with its data.
    pub async fn read(&self, file: File, id: i64) -> Result<(ImageRow, Vec<u8>), PackageError> {
        self.run(
            move || Source::from_file(file),
            move |conn| read_row(&conn, id),
        )
        .await
    }

    #[cfg(test)]
    pub(crate) async fn list_source(&self, source: Source) -> Result<Vec<ImageRow>, PackageError> {
        self.run(move || Ok(source), |conn| rows(&conn)).await
    }

    /// `source` runs on the blocking thread, after the slot is taken and inside the time limit: the
    /// check of a descriptor can block too.
    async fn run<T: Send + 'static>(
        &self,
        source: impl FnOnce() -> Result<Source, PackageError> + Send + 'static,
        job: impl FnOnce(Connection) -> Result<T, PackageError> + Send + 'static,
    ) -> Result<T, PackageError> {
        let permit = self
            .slot
            .clone()
            .try_acquire_owned()
            .map_err(|_| PackageError::Busy)?;
        let deadline = Instant::now() + self.limit;
        let task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            job(open(source()?, deadline)?)
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
    use std::time::Instant;

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

    /// The table of the aquasuite package, as the notes describe it (`docs/protocol/leakshield-firmware.md`).
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
        open(Source::from_file(open_file(&path)).unwrap(), far()).unwrap()
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
        )
        .unwrap();
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
        )
        .unwrap();
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

    const TABLE_NO_KEY: &str = "CREATE TABLE firmware (id INTEGER, version INTEGER, bootloader INTEGER, \
         hardware INTEGER, device TEXT, data BLOB);";

    #[tokio::test]
    async fn duplicate_id_reads_data_of_the_checked_row() {
        let dir = dir("dupid");
        let path = plain(
            &dir,
            &format!(
                "{TABLE_NO_KEY}\
                 INSERT INTO firmware VALUES (26, 1022, 100, 2000, 'octo_EN', CAST('octo image' AS BLOB));\
                 INSERT INTO firmware VALUES (26, 1030, 100, 1000, 'leakshield_EN', \
                 CAST('leakshield image' AS BLOB));"
            ),
        );
        let (row, data) = PackageReader::default()
            .read(open_file(&path), 26)
            .await
            .unwrap();
        assert_eq!((row.hardware, row.version), (1000, 1030));
        assert_eq!(data, b"leakshield image");
    }

    #[tokio::test]
    async fn two_matching_rows_with_one_id_are_refused() {
        let dir = dir("twoid");
        let path = plain(
            &dir,
            &format!(
                "{TABLE_NO_KEY}\
                 INSERT INTO firmware VALUES (26, 1022, 100, 1000, 'leakshield_EN', x'01');\
                 INSERT INTO firmware VALUES (26, 1030, 100, 1000, 'leakshield_DE', x'02');"
            ),
        );
        let reader = PackageReader::default();
        let result = reader.read(open_file(&path), 26).await;
        assert!(
            matches!(result, Err(PackageError::InvalidArgument(_))),
            "{result:?}"
        );
        assert_eq!(
            reader.read(open_file(&path), 27).await,
            Err(PackageError::NotFound)
        );
    }

    #[tokio::test]
    async fn view_with_the_type_table_is_refused() {
        let dir = dir("viewtype");
        let path = plain(
            &dir,
            "CREATE TABLE base (id INTEGER PRIMARY KEY, version INTEGER, bootloader INTEGER, \
             hardware INTEGER, device TEXT, data BLOB);\
             INSERT INTO base VALUES (26, 1022, 100, 1000, 'leakshield_EN', x'00');\
             CREATE VIEW firmware AS SELECT * FROM base;\
             PRAGMA writable_schema=ON;\
             UPDATE sqlite_schema SET type='table' WHERE name='firmware';",
        );
        let result = PackageReader::default().list(open_file(&path)).await;
        assert!(
            matches!(result, Err(PackageError::InvalidArgument(_))),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn virtual_table_is_refused() {
        let dir = dir("vtab");
        let path = plain(
            &dir,
            "CREATE VIRTUAL TABLE firmware USING fts4(id, version, bootloader, hardware, device, data);\
             INSERT INTO firmware VALUES (26, 1022, 100, 1000, 'leakshield_EN', x'00');",
        );
        let result = PackageReader::default().list(open_file(&path)).await;
        assert!(
            matches!(result, Err(PackageError::InvalidArgument(_))),
            "{result:?}"
        );
        let opened = open(Source::from_file(open_file(&path)).unwrap(), far());
        assert!(
            matches!(&opened, Err(PackageError::InvalidArgument(why)) if why.contains("plain table")),
            "the refusal comes from the table check: {:?}",
            opened.err()
        );
    }

    #[tokio::test]
    async fn column_collation_does_not_change_the_device_match() {
        let dir = dir("nocase");
        let path = plain(
            &dir,
            "CREATE TABLE firmware (id INTEGER PRIMARY KEY, version INTEGER, bootloader INTEGER, \
             hardware INTEGER, device TEXT COLLATE NOCASE, data BLOB);\
             INSERT INTO firmware VALUES (26, 1022, 100, 1000, 'LEAKSHIELD_EN', x'00');\
             INSERT INTO firmware VALUES (27, 1022, 100, 1000, 'leakshield_DE', x'01');",
        );
        let reader = PackageReader::default();
        let rows = reader.list(open_file(&path)).await.unwrap();
        assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), [27]);
        assert_eq!(
            reader.read(open_file(&path), 26).await,
            Err(PackageError::NotFound)
        );
    }

    fn far() -> Instant {
        Instant::now() + READ_LIMIT
    }

    /// A schema expression runs inside the root daemon: this one needs 120 MB of a file of a few
    /// KiB. The process-wide memory counter of SQLite is shared with the parallel tests, so the
    /// test checks that the expression never runs: the table check refuses the file first.
    #[tokio::test]
    async fn generated_column_never_runs() {
        let dir = dir("genmem");
        let path = plain(
            &dir,
            "CREATE TABLE firmware (id INTEGER PRIMARY KEY, \
             version INTEGER GENERATED ALWAYS AS (length(hex(zeroblob(60000000))) % 1000 + 1000), \
             bootloader INTEGER, hardware INTEGER, device TEXT, data BLOB);\
             INSERT INTO firmware (id, bootloader, hardware, device, data) \
             VALUES (26, 100, 1000, 'leakshield_EN', x'00');",
        );
        let opened = open(Source::from_file(open_file(&path)).unwrap(), far());
        assert!(
            matches!(&opened, Err(PackageError::InvalidArgument(why)) if why.contains("generated")),
            "{:?}",
            opened.err()
        );
    }

    #[tokio::test]
    async fn generated_column_is_refused() {
        let dir = dir("gencol");
        for kind in ["VIRTUAL", "STORED"] {
            let path = plain(
                &dir,
                &format!(
                    "DROP TABLE IF EXISTS firmware; \
                     CREATE TABLE firmware (id INTEGER PRIMARY KEY, \
                     version INTEGER GENERATED ALWAYS AS (1022) {kind}, \
                     bootloader INTEGER, hardware INTEGER, device TEXT, data BLOB);"
                ),
            );
            let result = PackageReader::default().list(open_file(&path)).await;
            assert!(
                matches!(result, Err(PackageError::InvalidArgument(_))),
                "{kind}: {result:?}"
            );
        }
    }

    #[tokio::test]
    async fn sql_work_stops_at_the_limit() {
        let dir = dir("gencpu");
        let path = plain(
            &dir,
            "CREATE TABLE firmware (id INTEGER PRIMARY KEY, \
             version INTEGER GENERATED ALWAYS AS (length(hex(zeroblob(20000000 + id))) % 1000 + 1000), \
             bootloader INTEGER, hardware INTEGER, device TEXT, data BLOB);\
             WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 5) \
             INSERT INTO firmware (id, bootloader, hardware, device, data) \
             SELECT i, 100, 1000, 'leakshield_EN', x'00' FROM n;",
        );
        let reader = PackageReader::with_limit(Duration::from_millis(100));
        let result = reader.list(open_file(&path)).await;
        assert!(
            matches!(result, Err(PackageError::InvalidArgument(_))),
            "{:?}",
            result.map(|r| r.len())
        );
        let timed_out = Instant::now();
        while reader.slot.available_permits() == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(
            timed_out.elapsed() < Duration::from_millis(500),
            "the slot stayed busy"
        );
    }

    #[test]
    fn progress_handler_interrupts_a_long_statement() {
        let dir = dir("progress");
        let path = dir.join("many.database");
        let source = Connection::open(&path).unwrap();
        source
            .execute_batch(
                "CREATE TABLE firmware (id INTEGER PRIMARY KEY, version INTEGER, bootloader INTEGER, \
                 hardware INTEGER, device TEXT, data BLOB);\
                 WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100000) \
                 INSERT INTO firmware SELECT i, 1, 1, 1, 'leakshield_EN', x'00' FROM n;",
            )
            .unwrap();
        drop(source);
        let slow = open(Source::from_file(open_file(&path)).unwrap(), far()).unwrap();
        assert_eq!(rows(&slow).unwrap().len(), 100_000);
        let late = open(Source::from_file(open_file(&path)).unwrap(), Instant::now()).unwrap();
        let result = rows(&late);
        assert!(
            matches!(result, Err(PackageError::InvalidArgument(_))),
            "{:?}",
            result.map(|r| r.len())
        );
    }

    #[test]
    fn value_length_is_limited() {
        let conn = connection(&dir("length"));
        assert_eq!(
            conn.limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH)
                .unwrap(),
            8 << 20
        );
        assert!(
            conn.query_row("SELECT length(hex(zeroblob(5000000)))", [], |r| r
                .get::<_, i64>(0))
                .is_err()
        );
    }

    #[tokio::test]
    async fn descriptor_check_waits_for_the_slot() {
        let dir = dir("fstatslot");
        let bytes = bytes_of(&dir);
        let fifo = dir.join("pipe");
        rustix::fs::mknodat(
            rustix::fs::CWD,
            &fifo,
            rustix::fs::FileType::Fifo,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
            0,
        )
        .unwrap();
        let open_fifo = || {
            fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&fifo)
                .unwrap()
        };
        let reader = PackageReader::default();
        let (release, gate) = mpsc::channel();
        let slow = Source::new(
            Gated {
                release: gate,
                inner: Cursor::new(bytes.clone()),
            },
            bytes.len() as u64,
        )
        .unwrap();
        let first = tokio::spawn({
            let reader = reader.clone();
            async move { reader.list_source(slow).await }
        });
        wait_until_busy(&reader).await;
        // The descriptor check is part of the parse: it runs after the slot is taken.
        assert_eq!(reader.list(open_fifo()).await, Err(PackageError::Busy));
        release.send(()).unwrap();
        first.await.unwrap().unwrap();
        assert!(matches!(
            reader.list(open_fifo()).await,
            Err(PackageError::InvalidArgument(_))
        ));
    }

    #[tokio::test]
    async fn utf16_text_stays_under_the_data_bound() {
        let dir = dir("utf16");
        let path = dir.join("utf16.database");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "PRAGMA encoding='UTF-16le';\
             CREATE TABLE firmware (id INTEGER PRIMARY KEY, version INTEGER, bootloader INTEGER, \
             hardware INTEGER, device TEXT, data BLOB);",
        )
        .unwrap();
        // `octet_length` counts 2 bytes for each of these characters, and UTF-8 needs 3.
        let text = "\u{4e00}".repeat(MAX_DATA_BYTES as usize / 2);
        conn.execute(
            "INSERT INTO firmware VALUES (26, 1022, 100, 1000, 'leakshield_EN', ?1)",
            [Value::Text(text)],
        )
        .unwrap();
        drop(conn);
        let result = PackageReader::default().read(open_file(&path), 26).await;
        assert!(
            matches!(result, Err(PackageError::InvalidArgument(_))),
            "{:?}",
            result.map(|(_, d)| d.len())
        );
    }

    #[test]
    fn source_new_bounds_the_length() {
        for len in [0, MAX_FILE_BYTES + 1] {
            let result = Source::new(Cursor::new(Vec::new()), len);
            assert!(
                matches!(result, Err(PackageError::InvalidArgument(_))),
                "{len}"
            );
        }
        assert!(Source::new(Cursor::new(Vec::new()), MAX_FILE_BYTES).is_ok());
    }
}
