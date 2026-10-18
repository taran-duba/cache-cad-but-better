//! Local file helpers: the read-only attribute that stops accidental saves,
//! detecting files open in SolidWorks, and atomic replacement.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use md5::{Digest, Md5};

use crate::error::Result;
use crate::names::open_marker_name;

/// Mark a file read-only (or writable). SolidWorks opens read-only files in
/// read-only mode, so nobody can save over a part they haven't checked out.
pub fn set_read_only(path: &Path, read_only: bool) -> Result<()> {
    let mut perms = fs::metadata(path)?.permissions();
    if perms.readonly() != read_only {
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(read_only);
        fs::set_permissions(path, perms)?;
    }
    Ok(())
}

pub fn is_read_only(path: &Path) -> bool {
    fs::metadata(path).map(|m| m.permissions().readonly()).unwrap_or(false)
}

/// True if SolidWorks has the document open (it keeps a `~$name` file beside it).
pub fn is_open_in_cad(path: &Path) -> bool {
    match (path.parent(), path.file_name()) {
        (Some(dir), Some(name)) => dir.join(open_marker_name(&name.to_string_lossy())).exists(),
        _ => false,
    }
}

pub fn md5_file(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path)?;
    let mut h = Md5::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

/// (size, mtime in ms) — a cheap fingerprint checked before hashing.
pub fn fingerprint(path: &Path) -> Option<(u64, i64)> {
    let m = fs::metadata(path).ok()?;
    let mtime = m.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_millis() as i64;
    Some((m.len(), mtime))
}

/// Has the file changed since we last synced it? Uses size+mtime first and
/// only hashes when those differ (touching a file without editing it is fine).
pub fn is_modified(path: &Path, synced_md5: Option<&str>, synced: Option<(u64, i64)>) -> Result<bool> {
    let Some(now) = fingerprint(path) else { return Ok(false) };
    if Some(now) == synced {
        return Ok(false);
    }
    Ok(synced_md5 != Some(md5_file(path)?.as_str()))
}

/// Move `staged` over `dest`, replacing it. Clears read-only first because
/// Windows refuses to replace a read-only file.
pub fn replace(staged: &Path, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    if dest.exists() {
        set_read_only(dest, false)?;
    }
    if fs::rename(staged, dest).is_err() {
        // Different volume: copy then remove.
        fs::copy(staged, dest)?;
        fs::remove_file(staged)?;
    }
    Ok(())
}

/// Move a local file out of the vault into the app's trash instead of deleting it.
pub fn move_to_trash(path: &Path, trash_dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(trash_dir)?;
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let target = trash_dir.join(format!("{stamp} {name}"));
    set_read_only(path, false)?;
    if fs::rename(path, &target).is_err() {
        fs::copy(path, &target)?;
        fs::remove_file(path)?;
    }
    Ok(target)
}

/// `Arm.SLDPRT` -> `Arm (conflict alex 2026-10-18 1342).SLDPRT`
pub fn conflict_name(path: &Path, who: &str) -> PathBuf {
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let who = who.split('@').next().unwrap_or(who);
    let stamp = chrono::Local::now().format("%Y-%m-%d %H%M");
    path.with_file_name(format!("{stem} (conflict {who} {stamp}){ext}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_and_replace() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a").join("Arm.SLDPRT");
        let staged = dir.path().join("staged");
        fs::write(&staged, b"v1").unwrap();
        replace(&staged, &dest).unwrap();
        set_read_only(&dest, true).unwrap();
        assert!(is_read_only(&dest));

        fs::write(&staged, b"v2").unwrap();
        replace(&staged, &dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"v2");
        set_read_only(&dest, false).unwrap();
    }

    #[test]
    fn detects_open_and_modified() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("Arm.SLDPRT");
        fs::write(&p, b"hello").unwrap();
        assert!(!is_open_in_cad(&p));
        fs::write(dir.path().join("~$Arm.SLDPRT"), b"").unwrap();
        assert!(is_open_in_cad(&p));

        let md5 = md5_file(&p).unwrap();
        assert_eq!(md5, "5d41402abc4b2a76b9719d911017c592");
        let fp = fingerprint(&p);
        assert!(!is_modified(&p, Some(&md5), fp).unwrap());
        assert!(!is_modified(&p, Some(&md5), None).unwrap(), "same content after touch");
        fs::write(&p, b"changed").unwrap();
        assert!(is_modified(&p, Some(&md5), fp).unwrap());
    }

    #[test]
    fn names_conflict_copies() {
        let p = Path::new("C:/CAD/Arm.SLDPRT");
        let c = conflict_name(p, "alex@team.org");
        let s = c.file_name().unwrap().to_string_lossy();
        assert!(s.starts_with("Arm (conflict alex ") && s.ends_with(").SLDPRT"), "{s}");
    }
}
