use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::ffi::CStr;
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

#[derive(Debug, Clone)]
pub struct Item {
    pub root_path: String,
    pub relative_path: String,
    pub name: String,
    pub extension: String,
    pub is_directory: bool,
    pub size_bytes: u64,
    pub modified_unix: i64,
    pub checksum_sha256: String,
    pub is_symlink: bool,
    pub symlink_target: String,
    pub owner_username: String,
    pub owner_identifier: String,
    pub group_name: String,
    pub group_identifier: String,
}

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub roots: Vec<PathBuf>,
    pub exclude_hidden: bool,
    pub exclude_caches: bool,
    pub exclude_temporary: bool,
    pub exclude_patterns: Vec<String>,
    pub boreal_home: PathBuf,
}

#[derive(Debug, Default)]
pub struct ScanResult {
    pub items: Vec<Item>,
    pub skipped: u64,
    pub errors: Vec<String>,
}

pub fn scan(
    options: &ScanOptions,
    cached: &HashMap<(String, String), (u64, i64, String)>,
) -> ScanResult {
    let mut result = ScanResult::default();
    let mut ownership = OwnershipCache::default();
    for root in &options.roots {
        let mut visited = HashSet::new();
        walk(
            root,
            root,
            options,
            &mut ownership,
            &mut visited,
            &mut result,
        );
    }
    let mut sizes = HashMap::<u64, usize>::new();
    for item in &result.items {
        if !item.is_directory && item.size_bytes > 0 {
            *sizes.entry(item.size_bytes).or_default() += 1;
        }
    }
    for item in &mut result.items {
        if item.is_directory
            || item.size_bytes == 0
            || sizes.get(&item.size_bytes).copied().unwrap_or(0) < 2
        {
            continue;
        }
        let key = (item.root_path.clone(), item.relative_path.clone());
        if let Some((size, mtime, hash)) = cached.get(&key) {
            if *size == item.size_bytes && *mtime == item.modified_unix && !hash.is_empty() {
                item.checksum_sha256 = hash.clone();
                continue;
            }
        }
        let path = Path::new(&item.root_path).join(&item.relative_path);
        match hash_file(&path) {
            Ok(hash) => item.checksum_sha256 = hash,
            Err(error) => {
                result.skipped += 1;
                result.errors.push(format!("{}: {error}", path.display()));
            }
        }
    }
    result
}

#[derive(Default)]
struct OwnershipCache {
    users: HashMap<u32, String>,
    groups: HashMap<u32, String>,
}

fn walk(
    root: &Path,
    directory: &Path,
    options: &ScanOptions,
    ownership: &mut OwnershipCache,
    visited: &mut HashSet<PathBuf>,
    result: &mut ScanResult,
) {
    let canonical_directory = directory.canonicalize().ok();
    if let Some(canonical) = &canonical_directory {
        if canonical.starts_with(&options.boreal_home) || !visited.insert(canonical.clone()) {
            return;
        }
    }
    let entries = match fs::read_dir(directory) {
        Ok(v) => v,
        Err(e) => {
            result.skipped += 1;
            result.errors.push(format!("{}: {e}", directory.display()));
            if let Some(canonical) = &canonical_directory {
                visited.remove(canonical);
            }
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap_or(&path);
        let rel = relative.to_string_lossy().replace('\\', "/");
        let name = entry.file_name().to_string_lossy().into_owned();
        if excluded(&path, &rel, &name, options) {
            result.skipped += 1;
            continue;
        }
        let link_metadata = match fs::symlink_metadata(&path) {
            Ok(v) => v,
            Err(e) => {
                result.skipped += 1;
                result.errors.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        let is_symlink = link_metadata.file_type().is_symlink();
        let symlink_target = if is_symlink {
            symlink_target(&path)
        } else {
            String::new()
        };
        let metadata = if is_symlink {
            fs::metadata(&path).unwrap_or(link_metadata)
        } else {
            link_metadata
        };
        let is_directory = metadata.is_dir();
        let modified_unix = metadata
            .modified()
            .ok()
            .and_then(|v| v.duration_since(UNIX_EPOCH).ok())
            .map(|v| v.as_secs() as i64)
            .unwrap_or(0);
        let (owner_username, owner_identifier, group_name, group_identifier) =
            file_ownership(&metadata, ownership);
        result.items.push(Item {
            root_path: root.to_string_lossy().into_owned(),
            relative_path: rel.clone(),
            name: name.clone(),
            extension: path
                .extension()
                .and_then(|v| v.to_str())
                .unwrap_or("")
                .to_string(),
            is_directory,
            size_bytes: if is_directory { 0 } else { metadata.len() },
            modified_unix,
            checksum_sha256: String::new(),
            is_symlink,
            symlink_target,
            owner_username,
            owner_identifier,
            group_name,
            group_identifier,
        });
        if is_directory {
            walk(root, &path, options, ownership, visited, result);
        }
    }
    if let Some(canonical) = &canonical_directory {
        visited.remove(canonical);
    }
}

fn symlink_target(path: &Path) -> String {
    let Ok(target) = fs::read_link(path) else {
        return String::new();
    };
    let resolved = if target.is_absolute() {
        target
    } else {
        path.parent().unwrap_or_else(|| Path::new("")).join(target)
    };
    resolved
        .canonicalize()
        .unwrap_or(resolved)
        .to_string_lossy()
        .into_owned()
}

#[cfg(unix)]
fn file_ownership(
    metadata: &fs::Metadata,
    cache: &mut OwnershipCache,
) -> (String, String, String, String) {
    use std::os::unix::fs::MetadataExt;

    let uid = metadata.uid();
    let gid = metadata.gid();
    let owner = cache
        .users
        .entry(uid)
        .or_insert_with(|| lookup_user(uid))
        .clone();
    let group = cache
        .groups
        .entry(gid)
        .or_insert_with(|| lookup_group(gid))
        .clone();
    (owner, uid.to_string(), group, gid.to_string())
}

#[cfg(not(unix))]
fn file_ownership(
    _metadata: &fs::Metadata,
    _cache: &mut OwnershipCache,
) -> (String, String, String, String) {
    (String::new(), String::new(), String::new(), String::new())
}

#[cfg(unix)]
fn lookup_user(uid: u32) -> String {
    // getpwuid_r writes both the record and its strings into caller-owned storage.
    let mut record = unsafe { std::mem::zeroed::<libc::passwd>() };
    let mut result = std::ptr::null_mut();
    let mut buffer = vec![0 as libc::c_char; 16 * 1024];
    let status = unsafe {
        libc::getpwuid_r(
            uid,
            &mut record,
            buffer.as_mut_ptr(),
            buffer.len(),
            &mut result,
        )
    };
    if status == 0 && !result.is_null() && !record.pw_name.is_null() {
        unsafe { CStr::from_ptr(record.pw_name) }
            .to_string_lossy()
            .into_owned()
    } else {
        String::new()
    }
}

#[cfg(unix)]
fn lookup_group(gid: u32) -> String {
    // getgrgid_r writes both the record and its strings into caller-owned storage.
    let mut record = unsafe { std::mem::zeroed::<libc::group>() };
    let mut result = std::ptr::null_mut();
    let mut buffer = vec![0 as libc::c_char; 16 * 1024];
    let status = unsafe {
        libc::getgrgid_r(
            gid,
            &mut record,
            buffer.as_mut_ptr(),
            buffer.len(),
            &mut result,
        )
    };
    if status == 0 && !result.is_null() && !record.gr_name.is_null() {
        unsafe { CStr::from_ptr(record.gr_name) }
            .to_string_lossy()
            .into_owned()
    } else {
        String::new()
    }
}

fn excluded(path: &Path, relative: &str, name: &str, o: &ScanOptions) -> bool {
    if path.starts_with(&o.boreal_home) {
        return true;
    }
    if o.exclude_hidden && (name.starts_with('.') || platform_hidden(path)) {
        return true;
    }
    let lower = name.to_ascii_lowercase();
    if o.exclude_caches
        && matches!(
            lower.as_str(),
            "cache" | "caches" | ".cache" | "node_modules" | "target" | "__pycache__"
        )
    {
        return true;
    }
    if o.exclude_temporary
        && matches!(
            lower.as_str(),
            "tmp" | "temp" | ".trash" | ".trashes" | "$recycle.bin"
        )
    {
        return true;
    }
    o.exclude_patterns
        .iter()
        .any(|p| wildcard_match(&p.replace('\\', "/"), relative))
}

#[cfg(windows)]
fn platform_hidden(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0)
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn platform_hidden(_path: &Path) -> bool {
    false
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    let (p, t) = (pattern.as_bytes(), text.as_bytes());
    let (mut i, mut j, mut star, mut mark) = (0, 0, None, 0);
    while j < t.len() {
        if i < p.len() && (p[i] == b'?' || p[i] == t[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == b'*' {
            star = Some(i);
            i += 1;
            mark = j;
        } else if let Some(s) = star {
            mark += 1;
            j = mark;
            i = s + 1;
        } else {
            return false;
        }
    }
    while i < p.len() && p[i] == b'*' {
        i += 1;
    }
    i == p.len()
}

fn hash_file(path: &Path) -> io::Result<String> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut b = [0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut b)?;
        if n == 0 {
            break;
        }
        h.update(&b[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

pub fn parse_roots(value: &str) -> Vec<PathBuf> {
    value
        .lines()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| {
            if v == "~" {
                dirs::home_dir().unwrap_or_default()
            } else if let Some(rest) = v.strip_prefix("~/") {
                dirs::home_dir().unwrap_or_default().join(rest)
            } else {
                PathBuf::from(v)
            }
        })
        .collect()
}

pub fn validate_roots(roots: &[PathBuf]) -> Result<(), String> {
    if roots.is_empty() {
        return Err("Add at least one local folder".into());
    }
    let mut seen = HashSet::new();
    for root in roots {
        if !root.is_absolute() {
            return Err(format!(
                "Local folder must be an absolute path: {}",
                root.display()
            ));
        }
        if !root.is_dir() {
            return Err(format!(
                "Local folder does not exist or is not a directory: {}",
                root.display()
            ));
        }
        if !seen.insert(root) {
            return Err(format!(
                "Local folder is listed more than once: {}",
                root.display()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wildcard_patterns_match_paths() {
        assert!(wildcard_match("Downloads/*.iso", "Downloads/archive.iso"));
        assert!(wildcard_match("**/cache*", "work/cache-data"));
        assert!(!wildcard_match("*.zip", "Downloads/archive.iso"));
    }
    #[test]
    fn rejects_missing_roots() {
        let path = std::env::temp_dir().join(format!("boreal-missing-{}", std::process::id()));
        assert!(validate_roots(&[path]).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn inventories_and_follows_directory_symlinks_without_cycles() {
        use std::os::unix::fs::symlink;

        let parent = std::env::temp_dir().join(format!(
            "boreal-symlink-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be valid")
                .as_nanos()
        ));
        let root = parent.join("root");
        let target = parent.join("target");
        fs::create_dir_all(&root).expect("root should be created");
        fs::create_dir_all(&target).expect("target should be created");
        fs::write(target.join("note.txt"), b"hello").expect("target file should be written");
        symlink(&target, root.join("linked")).expect("directory symlink should be created");
        symlink(&root, target.join("cycle")).expect("cycle symlink should be created");

        let result = scan(
            &ScanOptions {
                roots: vec![root],
                exclude_hidden: false,
                exclude_caches: false,
                exclude_temporary: false,
                exclude_patterns: Vec::new(),
                boreal_home: parent.join("boreal-home"),
            },
            &HashMap::new(),
        );
        let link = result
            .items
            .iter()
            .find(|item| item.relative_path == "linked")
            .expect("symlink should be inventoried");
        assert!(link.is_symlink);
        assert!(link.is_directory);
        assert_eq!(link.symlink_target, target.to_string_lossy());
        assert!(
            result
                .items
                .iter()
                .any(|item| item.relative_path == "linked/note.txt")
        );
        fs::remove_dir_all(parent).expect("test directory should be removable");
    }
}
