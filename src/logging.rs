use std::{
    error::Error,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

use crate::bootstrap::Runtime;
use log::{Level, LevelFilter, Log, Metadata, Record};

pub type LoggingError = Box<dyn Error>;

/// Initialize verbose local-calendar-day file logging.
///
/// Files are named `YYYY-MM-DD.boreal.log`. The local date is checked for
/// every event so a running process rolls over at local midnight.
pub fn initialize(runtime: &Runtime) -> Result<(), LoggingError> {
    let logs_dir = runtime
        .directories
        .get("LOGS")
        .ok_or("BOREAL LOGS directory is not configured")?;

    fs::create_dir_all(logs_dir)?;

    let logger = Box::leak(Box::new(BorealLogger::new(logs_dir)?));

    log::set_logger(logger)
        .map_err(|_| io::Error::other("BOREAL logger is already initialized"))?;
    log::set_max_level(LevelFilter::Debug);

    Ok(())
}

struct BorealLogger {
    state: Mutex<LogState>,
}

struct LogState {
    directory: PathBuf,
    date: String,
    file: File,
}

impl BorealLogger {
    fn new(directory: &Path) -> io::Result<Self> {
        let date = format_date(local_now());
        let file = open_log_file(directory, &date)?;

        Ok(Self {
            state: Mutex::new(LogState {
                directory: directory.to_path_buf(),
                date,
                file,
            }),
        })
    }
}

impl Log for BorealLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= Level::Debug
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let now = local_now();
        let date = format_date(now);
        let line = format!(
            "{} {:<5} [{}] {}\n",
            format_timestamp(now),
            record.level(),
            record.target(),
            record.args(),
        );
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => {
                std::eprintln!("BOREAL logging error: log writer lock is poisoned");
                return;
            }
        };

        if state.date != date {
            match open_log_file(&state.directory, &date) {
                Ok(file) => {
                    state.file = file;
                    state.date = date;
                }
                Err(error) => {
                    std::eprintln!("BOREAL logging error: {error}");
                }
            }
        }

        if let Err(error) = state.file.write_all(line.as_bytes()) {
            std::eprintln!("BOREAL logging error: {error}");
        }
    }

    fn flush(&self) {
        if let Ok(mut state) = self.state.lock() {
            let _ = state.file.flush();
        }
    }
}

#[derive(Clone, Copy)]
struct LocalTime {
    year: i32,
    month: i32,
    day: i32,
    hour: i32,
    minute: i32,
    second: i32,
    millisecond: u128,
}

fn local_now() -> LocalTime {
    let mut raw_time: libc::time_t = 0;
    let mut local: libc::tm = unsafe {
        // A zeroed `tm` is valid storage for the platform time conversion APIs.
        std::mem::zeroed()
    };

    unsafe {
        // `raw_time` and `local` are valid writable pointers for these C APIs.
        libc::time(&mut raw_time);

        #[cfg(unix)]
        libc::localtime_r(&raw_time, &mut local);

        #[cfg(windows)]
        libc::localtime_s(&mut local, &raw_time);
    }

    let millisecond = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() % 1_000)
        .unwrap_or(0);

    LocalTime {
        year: local.tm_year + 1900,
        month: local.tm_mon + 1,
        day: local.tm_mday,
        hour: local.tm_hour,
        minute: local.tm_min,
        second: local.tm_sec,
        millisecond,
    }
}

fn format_date(now: LocalTime) -> String {
    format!("{:04}-{:02}-{:02}", now.year, now.month, now.day,)
}

fn format_timestamp(now: LocalTime) -> String {
    format!(
        "{}T{:02}:{:02}:{:02}.{:03}",
        format_date(now),
        now.hour,
        now.minute,
        now.second,
        now.millisecond,
    )
}

fn open_log_file(directory: &Path, date: &str) -> io::Result<File> {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join(format!("{date}.boreal.log")))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }

    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_requested_log_file_name() {
        let now = local_now();
        let name = format!("{}.boreal.log", format_date(now));

        assert_eq!(name.len(), "YYYY-MM-DD.boreal.log".len());
        assert!(name.ends_with(".boreal.log"));
    }

    #[test]
    fn writes_to_current_day_log_file() {
        let directory =
            std::env::temp_dir().join(format!("boreal-logging-test-{}", std::process::id(),));
        fs::create_dir_all(&directory).expect("temporary log directory should be created");
        let logger = BorealLogger::new(&directory).expect("logger should initialize");
        let message = format_args!("logging test");
        let record = Record::builder()
            .args(message)
            .level(Level::Info)
            .target("boreal::test")
            .build();

        logger.log(&record);
        logger.flush();

        let log_path = directory.join(format!("{}.boreal.log", format_date(local_now()),));
        let contents = fs::read_to_string(log_path).expect("current day log should be readable");

        assert!(contents.contains("logging test",),);

        fs::remove_dir_all(directory).expect("temporary log directory should be removed");
    }
}
