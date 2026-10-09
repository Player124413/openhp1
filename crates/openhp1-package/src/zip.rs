//! Pure Rust ZIP archive decoder and game package installer.
//!
//! Extracts game assets from `.zip` archives across desktop and mobile platforms
//! without requiring external CLI commands like `unzip`.

use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::{
    GameInstallation, GameInstallationError, configure_game_installation_with_settings_dir,
    settings_dir,
};

#[derive(Debug, Error)]
pub enum ZipError {
    #[error("I/O error while reading zip archive: {0}")]
    Io(#[from] io::Error),
    #[error("invalid zip archive: {0}")]
    InvalidArchive(String),
    #[error("unsupported zip compression method: {0}")]
    UnsupportedCompression(u16),
    #[error("deflate decompression failed: {0:?}")]
    DecompressionFailed(miniz_oxide::inflate::DecompressError),
    #[error("zip slip path traversal detected in entry: {0}")]
    PathTraversal(String),
    #[error("could not locate Maps and System directories in unpacked files")]
    GameRootNotFound,
}

struct ZipEntry {
    filename: String,
    compression_method: u16,
    compressed_size: u64,
    uncompressed_size: u64,
    local_header_offset: u64,
    is_directory: bool,
}

/// Unpacks a `.zip` archive into `destination_dir` and returns the discovered
/// Harry Potter game root containing `Maps` and `System`.
pub fn unpack_zip_archive(zip_path: &Path, destination_dir: &Path) -> Result<PathBuf, ZipError> {
    let mut file = File::open(zip_path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;

    let entries = parse_central_directory(&bytes)?;
    fs::create_dir_all(destination_dir)?;

    for entry in entries {
        // Prevent zip-slip path traversal attacks
        let safe_rel_path = sanitize_zip_path(&entry.filename)?;
        if safe_rel_path.as_os_str().is_empty() {
            continue;
        }

        let target_path = destination_dir.join(&safe_rel_path);

        if entry.is_directory {
            fs::create_dir_all(&target_path)?;
            continue;
        }

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file_data = extract_entry_data(&bytes, &entry)?;
        let mut out_file = File::create(&target_path)?;
        out_file.write_all(&file_data)?;
    }

    find_game_root(destination_dir).ok_or(ZipError::GameRootNotFound)
}

/// Installs game data from a ZIP archive, unpacks it to `target_dir` (defaulting
/// to `<settings_dir>/GameData` or `/sdcard/OpenHP1`), discovers the game root,
/// and validates/configures it in `OpenHP1.ini`.
pub fn install_from_zip(
    zip_path: &Path,
    target_dir: Option<&Path>,
) -> Result<GameInstallation, GameInstallationError> {
    let settings = settings_dir();
    let default_dest = settings.join("GameData");
    let dest = target_dir.unwrap_or(&default_dest);

    let game_root = unpack_zip_archive(zip_path, dest).map_err(|err| {
        GameInstallationError::InvalidRoot {
            root: zip_path.to_path_buf(),
            reason: format!("failed to unpack zip archive: {err}"),
        }
    })?;

    configure_game_installation_with_settings_dir(&game_root, None, &settings)
}

fn sanitize_zip_path(path_str: &str) -> Result<PathBuf, ZipError> {
    let normalized = path_str.replace('\\', "/");
    let path = Path::new(&normalized);

    let mut safe = PathBuf::new();
    for comp in path.components() {
        match comp {
            std::path::Component::Normal(c) => safe.push(c),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir | std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err(ZipError::PathTraversal(path_str.to_owned()));
            }
        }
    }
    Ok(safe)
}

fn parse_central_directory(bytes: &[u8]) -> Result<Vec<ZipEntry>, ZipError> {
    if bytes.len() < 22 {
        return Err(ZipError::InvalidArchive("file too short for zip".into()));
    }

    // Locate End of Central Directory (EOCD) signature: 0x06054b50
    let mut eocd_pos = None;
    let max_search = bytes.len().saturating_sub(22);
    let min_search = bytes.len().saturating_sub(65536 + 22);

    for i in (min_search..=max_search).rev() {
        if bytes[i..i + 4] == [0x50, 0x4b, 0x05, 0x06] {
            eocd_pos = Some(i);
            break;
        }
    }

    let eocd_offset = eocd_pos.ok_or_else(|| {
        ZipError::InvalidArchive("missing End of Central Directory record".into())
    })?;

    let total_entries = u16::from_le_bytes(
        bytes[eocd_offset + 10..eocd_offset + 12]
            .try_into()
            .unwrap(),
    ) as usize;

    let cd_offset = u32::from_le_bytes(
        bytes[eocd_offset + 16..eocd_offset + 20]
            .try_into()
            .unwrap(),
    ) as usize;

    let mut entries = Vec::with_capacity(total_entries);
    let mut cursor = cd_offset;

    for _ in 0..total_entries {
        if cursor + 46 > bytes.len() {
            return Err(ZipError::InvalidArchive("central directory truncated".into()));
        }

        if bytes[cursor..cursor + 4] != [0x50, 0x4b, 0x01, 0x02] {
            return Err(ZipError::InvalidArchive(
                "invalid central directory header magic".into(),
            ));
        }

        let compression = u16::from_le_bytes(bytes[cursor + 10..cursor + 12].try_into().unwrap());
        let comp_size = u32::from_le_bytes(bytes[cursor + 20..cursor + 24].try_into().unwrap()) as u64;
        let uncomp_size = u32::from_le_bytes(bytes[cursor + 24..cursor + 28].try_into().unwrap()) as u64;
        let fn_len = u16::from_le_bytes(bytes[cursor + 28..cursor + 30].try_into().unwrap()) as usize;
        let extra_len = u16::from_le_bytes(bytes[cursor + 30..cursor + 32].try_into().unwrap()) as usize;
        let comment_len = u16::from_le_bytes(bytes[cursor + 32..cursor + 34].try_into().unwrap()) as usize;
        let local_offset = u32::from_le_bytes(bytes[cursor + 42..cursor + 46].try_into().unwrap()) as u64;

        let fn_start = cursor + 46;
        let fn_end = fn_start + fn_len;
        if fn_end > bytes.len() {
            return Err(ZipError::InvalidArchive("entry filename truncated".into()));
        }

        let filename = String::from_utf8_lossy(&bytes[fn_start..fn_end]).to_string();
        let is_directory = filename.ends_with('/') || filename.ends_with('\\');

        entries.push(ZipEntry {
            filename,
            compression_method: compression,
            compressed_size: comp_size,
            uncompressed_size: uncomp_size,
            local_header_offset: local_offset,
            is_directory,
        });

        cursor = fn_end + extra_len + comment_len;
    }

    Ok(entries)
}

fn extract_entry_data(bytes: &[u8], entry: &ZipEntry) -> Result<Vec<u8>, ZipError> {
    let offset = entry.local_header_offset as usize;
    if offset + 30 > bytes.len() {
        return Err(ZipError::InvalidArchive("local header truncated".into()));
    }

    if bytes[offset..offset + 4] != [0x50, 0x4b, 0x03, 0x04] {
        return Err(ZipError::InvalidArchive("invalid local header magic".into()));
    }

    let fn_len = u16::from_le_bytes(bytes[offset + 26..offset + 28].try_into().unwrap()) as usize;
    let extra_len = u16::from_le_bytes(bytes[offset + 28..offset + 30].try_into().unwrap()) as usize;

    let data_start = offset + 30 + fn_len + extra_len;
    let data_end = data_start + entry.compressed_size as usize;

    if data_end > bytes.len() {
        return Err(ZipError::InvalidArchive("compressed entry data truncated".into()));
    }

    let compressed = &bytes[data_start..data_end];

    match entry.compression_method {
        0 => Ok(compressed.to_vec()), // Stored
        8 => {
            // Deflate
            miniz_oxide::inflate::decompress_to_vec(compressed)
                .map_err(ZipError::DecompressionFailed)
        }
        method => Err(ZipError::UnsupportedCompression(method)),
    }
}

/// Recursively inspects `dir` to locate a folder containing both `Maps` and `System`
/// (case-insensitively).
pub fn find_game_root(dir: &Path) -> Option<PathBuf> {
    if is_valid_root(dir) {
        return Some(dir.to_path_buf());
    }

    // Search children up to depth 3
    let mut queue = vec![(dir.to_path_buf(), 0)];
    while let Some((current, depth)) = queue.pop() {
        if depth > 3 {
            continue;
        }
        if let Ok(entries) = fs::read_dir(&current) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_dir() {
                        let path = entry.path();
                        if is_valid_root(&path) {
                            return Some(path);
                        }
                        queue.push((path, depth + 1));
                    }
                }
            }
        }
    }

    None
}

fn is_valid_root(path: &Path) -> bool {
    let has_child = |name: &str| {
        fs::read_dir(path).is_ok_and(|mut entries| {
            entries.any(|entry| {
                entry
                    .ok()
                    .and_then(|e| e.file_name().into_string().ok())
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
        })
    };

    has_child("Maps") && has_child("System")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_prevents_directory_traversal() {
        assert!(sanitize_zip_path("../../etc/passwd").is_err());
        assert!(sanitize_zip_path("/etc/passwd").is_err());
        assert!(sanitize_zip_path("..\\Windows\\System32").is_err());
        assert_eq!(
            sanitize_zip_path("Maps/Lev_Tut1.unr").unwrap(),
            PathBuf::from("Maps/Lev_Tut1.unr")
        );
        assert_eq!(
            sanitize_zip_path("Game\\System\\Default.ini").unwrap(),
            PathBuf::from("Game/System/Default.ini")
        );
    }
}
