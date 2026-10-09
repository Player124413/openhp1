use std::{
    fs::{self, File},
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use openhp1_package::settings_dir;
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};

static LOG_BUFFER: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();

fn log_buffer() -> Arc<Mutex<Vec<String>>> {
    LOG_BUFFER.get_or_init(|| Arc::new(Mutex::new(Vec::new()))).clone()
}

pub fn add_log_line(line: impl AsRef<str>) {
    let line = line.as_ref().trim_end();
    if line.is_empty() {
        return;
    }
    if let Ok(mut buf) = log_buffer().lock() {
        if buf.len() >= 1000 {
            buf.remove(0);
        }
        buf.push(line.to_owned());
    }
}

pub fn get_all_logs() -> String {
    if let Ok(buf) = log_buffer().lock() {
        buf.join("\n")
    } else {
        String::new()
    }
}

pub fn copy_logs(egui_context: Option<&egui::Context>) -> (String, bool) {
    let logs = get_all_logs();
    if let Some(ctx) = egui_context {
        ctx.copy_text(logs.clone());
    }

    let mut saved = Vec::new();
    for dir in [
        "/sdcard",
        "/sdcard/OpenHP1",
        "/sdcard/Android/data/org.openhp1.game/files",
        "/storage/emulated/0/OpenHP1",
        "/storage/emulated/0/Android/data/org.openhp1.game/files",
    ] {
        let path = PathBuf::from(dir);
        let _ = fs::create_dir_all(&path);
        let log_file = path.join("openhp1-log.txt");
        if fs::write(&log_file, &logs).is_ok() {
            saved.push(log_file.display().to_string());
        }
    }

    let msg = if !saved.is_empty() {
        format!("Логи скопированы в буфер обмена и записаны в:\n{}", saved.join("\n"))
    } else {
        "Логи успешно скопированы в буфер обмена!".to_owned()
    };
    (msg, true)
}

#[derive(Clone)]
struct DualLogWriter {
    file: Option<Arc<Mutex<File>>>,
}

impl Write for DualLogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let text = String::from_utf8_lossy(buf);
        for line in text.lines() {
            add_log_line(line);
        }
        if let Some(file) = &self.file {
            if let Ok(mut f) = file.lock() {
                let _ = f.write_all(buf);
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(file) = &self.file {
            if let Ok(mut f) = file.lock() {
                let _ = f.flush();
            }
        }
        Ok(())
    }
}

pub fn init_logging() -> Result<PathBuf> {
    init_panic_hook();

    let directory = settings_dir().join("Logs");
    let _ = fs::create_dir_all(&directory);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let path = directory.join(format!("openhp1-{timestamp}.log"));
    let file = File::create(&path).ok().map(|f| Arc::new(Mutex::new(f)));

    let writer = DualLogWriter { file };

    let _ = tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(EnvFilter::from_default_env()))
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(Mutex::new(writer))
                .with_filter(EnvFilter::new("info,symphonia_bundle_mp3=off")),
        )
        .try_init();

    add_log_line(format!("OpenHP1 logging initialized. Log path: {}", path.display()));
    Ok(path)
}

pub fn init_panic_hook() {
    static HOOK_INSTALLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if HOOK_INSTALLED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }

    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".into());
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Box<Any>".into()
        };
        let msg = format!("CRITICAL PANIC at {location}: {payload}");
        eprintln!("{msg}");
        add_log_line(&msg);

        for dir in [
            "/sdcard",
            "/sdcard/OpenHP1",
            "/sdcard/Android/data/org.openhp1.game/files",
        ] {
            let path = PathBuf::from(dir);
            let _ = fs::create_dir_all(&path);
            let _ = fs::write(path.join("openhp1-crash.txt"), &msg);
            let _ = fs::write(path.join("openhp1-all-logs.txt"), get_all_logs());
        }

        prev(info);
    }));
}
