//! Disk I/O lives on the worker, never in an audio callback. No audio is recorded.
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
pub fn directory() -> PathBuf {
    crate::state::data_dir().join("logs")
}
pub struct Logger {
    file: File,
    bytes: u64,
    dir: PathBuf,
}
impl Logger {
    const LIMIT: u64 = 4 * 1024 * 1024;
    pub fn open() -> io::Result<Self> {
        Self::open_in(directory())
    }
    fn open_in(dir: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&dir)?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("audio.jsonl"))?;
        let bytes = file.metadata()?.len();
        Ok(Self { file, bytes, dir })
    }
    pub fn write(&mut self, event: &str, value: Value) -> io::Result<()> {
        if self.bytes >= Self::LIMIT {
            // Keep four files, at most roughly 16 MiB total.
            let dir = &self.dir;
            let oldest = dir.join("audio.3.jsonl");
            if oldest.exists() {
                fs::remove_file(oldest)?;
            }
            for i in (1..3).rev() {
                let source = dir.join(format!("audio.{i}.jsonl"));
                if source.exists() {
                    fs::rename(source, dir.join(format!("audio.{}.jsonl", i + 1)))?;
                }
            }
            // Close the active handle before renaming on Windows.
            let replacement = OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join("rotation.tmp"))?;
            drop(std::mem::replace(&mut self.file, replacement));
            fs::rename(dir.join("audio.jsonl"), dir.join("audio.1.jsonl"))?;
            self.file = File::create(dir.join("audio.jsonl"))?;
            let _ = fs::remove_file(dir.join("rotation.tmp"));
            self.bytes = 0;
        }
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let mut data =
            serde_json::to_vec(&json!({"unix_ms":timestamp,"event":event,"data":value}))?;
        data.push(b'\n');
        self.file.write_all(&data)?;
        self.file.flush()?;
        self.bytes += data.len() as u64;
        Ok(())
    }
}
pub fn record(
    logger: &mut Option<Logger>,
    events: &std::sync::mpsc::Sender<crate::audio::Event>,
    event: &str,
    data: Value,
) {
    if let Some(log) = logger
        && let Err(e) = log.write(event, data)
    {
        let _ = events.send(crate::audio::Event::LogStatus(Err(format!(
            "診断ログの保存に失敗: {e}"
        ))));
        *logger = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logs_survive_reopen_and_rotate_with_bounded_history() {
        let dir = std::env::temp_dir().join(format!(
            "effecter-log-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut logger = Logger::open_in(dir.clone()).unwrap();
        logger.write("start", json!({"buffer":32})).unwrap();
        drop(logger);
        let mut logger = Logger::open_in(dir.clone()).unwrap();
        logger
            .write("metrics", json!({"missing_frames":10}))
            .unwrap();
        let text = fs::read_to_string(dir.join("audio.jsonl")).unwrap();
        let rows: Vec<Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1]["data"]["missing_frames"], 10);
        assert!(rows[0]["unix_ms"].as_u64().unwrap() > 0);
        for i in 0..6 {
            logger.bytes = Logger::LIMIT;
            logger.write("rotation", json!({"index":i})).unwrap();
        }
        drop(logger);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 4);
        for file in fs::read_dir(&dir).unwrap() {
            for line in fs::read_to_string(file.unwrap().path()).unwrap().lines() {
                let _: Value = serde_json::from_str(line).unwrap();
            }
        }
        fs::remove_dir_all(dir).unwrap();
    }
}
