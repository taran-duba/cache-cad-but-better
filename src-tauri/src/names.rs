//! Mapping Drive names to names that are always legal on Windows (NTFS).
//!
//! Drive allows almost anything in a name — `/`, `:`, emoji, trailing dots,
//! `CON`, and duplicate names in one folder. CacheCAD crashed on several of
//! these. Every local name goes through here, and the Drive name is kept
//! separately in the index, so nothing on Drive is ever renamed.

use std::collections::HashSet;

/// Max length of one path component on NTFS, in UTF-16 code units.
const MAX_COMPONENT_UTF16: usize = 255;

const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM0", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
    "COM8", "COM9", "LPT0", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    "COM¹", "COM²", "COM³", "LPT¹", "LPT²", "LPT³", "CONIN$", "CONOUT$",
];

fn is_illegal_char(c: char) -> bool {
    matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || (c as u32) < 32
}

/// Split into (stem, extension-with-dot). A leading dot is part of the stem.
fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 && i < name.len() - 1 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// Truncate `s` to at most `max` UTF-16 units without splitting a character.
fn truncate_utf16(s: &str, max: usize) -> &str {
    let mut used = 0;
    for (i, c) in s.char_indices() {
        used += c.len_utf16();
        if used > max {
            return &s[..i];
        }
    }
    s
}

/// Make a single Drive name safe to use as one Windows path component.
pub fn sanitize(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| if is_illegal_char(c) { '_' } else { c })
        .collect();

    // Windows silently strips trailing dots and spaces, which would make two
    // different Drive names collide or a name vanish entirely.
    let trimmed_len = out.trim_end_matches(['.', ' ']).len();
    if trimmed_len != out.len() {
        out.truncate(trimmed_len);
        out.push('_');
    }
    if out.trim().is_empty() {
        out = "_".to_string();
    }

    let stem_upper = split_ext(&out).0.trim_end().to_uppercase();
    if RESERVED.contains(&stem_upper.as_str()) {
        out.insert(0, '_');
    }

    if utf16_len(&out) > MAX_COMPONENT_UTF16 {
        let (stem, ext) = split_ext(&out);
        let ext = if utf16_len(ext) < 32 { ext } else { "" };
        let stem = truncate_utf16(stem, MAX_COMPONENT_UTF16 - utf16_len(ext));
        out = format!("{stem}{ext}");
    }
    out
}

/// Assigns unique local names within one folder.
///
/// NTFS is case-insensitive, so `Arm.SLDPRT` and `arm.sldprt` collide. Callers
/// must feed entries in a stable order (oldest Drive `createdTime` first, then
/// file id) so every machine computes the same local paths — SolidWorks stores
/// absolute paths, so paths must match across the team.
#[derive(Default)]
pub struct FolderNamer {
    taken: HashSet<String>,
}

impl FolderNamer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the local name and whether it had to be changed to stay unique.
    pub fn assign(&mut self, drive_name: &str) -> (String, bool) {
        let base = sanitize(drive_name);
        if self.taken.insert(base.to_lowercase()) {
            return (base, false);
        }
        let (stem, ext) = split_ext(&base);
        for n in 2.. {
            let candidate = format!("{stem} ({n}){ext}");
            if self.taken.insert(candidate.to_lowercase()) {
                return (candidate, true);
            }
        }
        unreachable!()
    }
}

/// True for files SolidWorks or Windows create transiently; never synced.
pub fn is_temp_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with("~$")
        || lower.starts_with("~temp")
        || lower.ends_with(".swbak")
        || lower.ends_with(".tmp")
        || lower == "desktop.ini"
        || lower == "thumbs.db"
}

/// SolidWorks writes `~$Name.SLDPRT` next to a document while it is open.
pub fn open_marker_name(file_name: &str) -> String {
    format!("~${file_name}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CadKind {
    Part,
    Assembly,
    Drawing,
    Other,
}

pub fn cad_kind(name: &str) -> CadKind {
    let lower = name.to_lowercase();
    if lower.ends_with(".sldprt") {
        CadKind::Part
    } else if lower.ends_with(".sldasm") {
        CadKind::Assembly
    } else if lower.ends_with(".slddrw") {
        CadKind::Drawing
    } else {
        CadKind::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_illegal_characters() {
        assert_eq!(sanitize("Intake/Shooter"), "Intake_Shooter");
        assert_eq!(sanitize("a<b>c:d\"e\\f|g?h*i"), "a_b_c_d_e_f_g_h_i");
        assert_eq!(sanitize("tab\there"), "tab_here");
    }

    #[test]
    fn keeps_emoji_and_unicode() {
        assert_eq!(sanitize("🤖 Robot.SLDASM"), "🤖 Robot.SLDASM");
        assert_eq!(sanitize("Ösen-Platte.sldprt"), "Ösen-Platte.sldprt");
    }

    #[test]
    fn handles_trailing_dots_and_spaces() {
        assert_eq!(sanitize("Bracket."), "Bracket_");
        assert_eq!(sanitize("Bracket  "), "Bracket_");
        assert_eq!(sanitize("..."), "_");
        assert_eq!(sanitize("   "), "_");
        assert_eq!(sanitize(""), "_");
    }

    #[test]
    fn prefixes_reserved_names() {
        assert_eq!(sanitize("CON"), "_CON");
        assert_eq!(sanitize("con.sldprt"), "_con.sldprt");
        assert_eq!(sanitize("Lpt1.txt"), "_Lpt1.txt");
        assert_eq!(sanitize("CONSOLE.sldprt"), "CONSOLE.sldprt");
        assert_eq!(sanitize("COM10"), "COM10");
    }

    #[test]
    fn truncates_long_names_keeping_extension() {
        let long = format!("{}.SLDPRT", "a".repeat(300));
        let out = sanitize(&long);
        assert_eq!(utf16_len(&out), 255);
        assert!(out.ends_with(".SLDPRT"));

        let emoji = format!("{}.sldprt", "🤖".repeat(200));
        let out = sanitize(&emoji);
        assert!(utf16_len(&out) <= 255);
        assert!(out.ends_with(".sldprt"));
    }

    #[test]
    fn dedupes_case_insensitively() {
        let mut namer = FolderNamer::new();
        assert_eq!(namer.assign("Arm.SLDPRT"), ("Arm.SLDPRT".into(), false));
        assert_eq!(namer.assign("arm.sldprt"), ("arm (2).sldprt".into(), true));
        assert_eq!(namer.assign("Arm.SLDPRT"), ("Arm (3).SLDPRT".into(), true));
        // Sanitizing can create collisions too.
        assert_eq!(namer.assign("a/b"), ("a_b".into(), false));
        assert_eq!(namer.assign("a:b"), ("a_b (2)".into(), true));
    }

    #[test]
    fn detects_temp_and_kinds() {
        assert!(is_temp_file("~$Arm.SLDPRT"));
        assert!(is_temp_file("Thumbs.db"));
        assert!(!is_temp_file("Arm.SLDPRT"));
        assert_eq!(cad_kind("Drive.SLDASM"), CadKind::Assembly);
        assert_eq!(cad_kind("gear.sldprt"), CadKind::Part);
        assert_eq!(cad_kind("Arm.SLDDRW"), CadKind::Drawing);
        assert_eq!(cad_kind("notes.pdf"), CadKind::Other);
        assert_eq!(open_marker_name("Arm.SLDPRT"), "~$Arm.SLDPRT");
    }
}
