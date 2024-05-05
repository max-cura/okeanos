use std::fs::DirEntry;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::FileTypeExt;
use std::path::PathBuf;
use color_eyre::eyre;

static PATTERNS: [&str; 6] = [
    "ttyUSB",
    "ttyACM",
    "tty.usbserial",
    "cu.usbserial",
    "tty.SLAB_USB",
    "cu.SLAB_USB",
];

pub fn find_most_recent_tty_serial_device() -> eyre::Result<PathBuf> {
    std::fs::read_dir("/dev")?
        .filter_map(|entry| -> Option<DirEntry> {
            entry.ok().and_then(|e| {
                e.metadata().ok()?;

                let ft = e.file_type().ok()?;
                if !ft.is_char_device() {
                    return None;
                }

                let path = e.path();
                let file_name = path.file_name()?;
                if !PATTERNS.iter().any(
                    |pattern|
                        std::str::from_utf8(file_name.as_bytes())
                            .map(|f| f.starts_with(pattern))
                            .unwrap_or(false)) {
                    return None;
                }

                Some(e)
            })
        })
        .max_by_key(|e| {
            e.metadata().unwrap().modified().unwrap()
        })
        .ok_or_else(|| eyre::eyre!("No device like: /dev/{{{}}}",
            PATTERNS.iter()
            .map(|pattern| pattern.to_string() + "*")
            .intersperse(", ".to_string())
            .collect::<String>()))
        .map(|e| e.path().to_path_buf())
}

