//! Turns the flat file index into local relative paths. Every machine runs
//! the same deterministic computation, so the whole team ends up with the same
//! paths under the same root — which is what keeps SolidWorks' stored
//! absolute references valid on everyone's computer.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use crate::drive::RemoteFile;
use crate::names::{is_temp_file, FolderNamer};

/// Name of the hidden folder on the shared drive that holds app metadata.
pub const META_FOLDER: &str = ".cadsync";

#[derive(Default)]
pub struct PathMap {
    by_id: HashMap<String, PathBuf>,
    by_path: HashMap<String, String>,
    /// Files whose local name differs from their Drive name to stay unique.
    pub renamed: Vec<String>,
    pub meta_folder_id: Option<String>,
}

fn key(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/").to_lowercase()
}

impl PathMap {
    pub fn build(root_id: &str, files: &[RemoteFile]) -> Self {
        let mut kids: HashMap<&str, Vec<&RemoteFile>> = HashMap::new();
        for f in files {
            if let Some(p) = f.parent.as_deref() {
                kids.entry(p).or_default().push(f);
            }
        }
        let mut map = PathMap::default();
        let mut stack = vec![(root_id.to_string(), PathBuf::new())];
        while let Some((folder, path)) = stack.pop() {
            let Some(children) = kids.get_mut(folder.as_str()) else { continue };
            // Oldest first so an existing file never loses its name to a newer duplicate.
            children.sort_by(|a, b| (&a.created_time, &a.id).cmp(&(&b.created_time, &b.id)));
            let mut namer = FolderNamer::new();
            for f in children.iter() {
                if folder == root_id && f.name == META_FOLDER && f.is_folder() {
                    map.meta_folder_id = Some(f.id.clone());
                    continue;
                }
                if is_temp_file(&f.name) {
                    continue;
                }
                let (name, changed) = namer.assign(&f.name);
                if changed {
                    map.renamed.push(f.id.clone());
                }
                let p = path.join(name);
                map.by_path.insert(key(&p), f.id.clone());
                if f.is_folder() {
                    stack.push((f.id.clone(), p.clone()));
                }
                map.by_id.insert(f.id.clone(), p);
            }
        }
        map
    }

    pub fn path(&self, id: &str) -> Option<&Path> {
        self.by_id.get(id).map(PathBuf::as_path)
    }

    /// Look up a file by relative path (case-insensitive, either slash style).
    pub fn id_for(&self, rel: &Path) -> Option<&str> {
        self.by_path.get(&key(rel)).map(String::as_str)
    }

    pub fn contains(&self, id: &str) -> bool {
        self.by_id.contains_key(id)
    }

    /// Is `id` equal to or inside folder `ancestor`?
    pub fn is_within(&self, id: &str, ancestor: &str) -> bool {
        match (self.by_id.get(id), self.by_id.get(ancestor)) {
            (Some(p), Some(a)) => p.starts_with(a),
            _ => false,
        }
    }

    /// Non-folder files sharing a file name (case-insensitive) anywhere in the
    /// vault. SolidWorks resolves references by file name when the stored path
    /// doesn't match, so these can load the wrong part and break mates.
    pub fn duplicate_names(&self, files: &[RemoteFile]) -> BTreeMap<String, Vec<String>> {
        let mut by_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for f in files {
            if f.is_folder() || !self.contains(&f.id) {
                continue;
            }
            if crate::names::cad_kind(&f.name) == crate::names::CadKind::Other {
                continue;
            }
            by_name.entry(f.name.to_lowercase()).or_default().push(f.id.clone());
        }
        by_name.retain(|_, ids| ids.len() > 1);
        by_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drive::FOLDER_MIME;

    fn f(id: &str, parent: &str, name: &str, folder: bool, created: &str) -> RemoteFile {
        RemoteFile {
            id: id.into(),
            name: name.into(),
            parent: Some(parent.into()),
            mime_type: if folder { FOLDER_MIME.into() } else { "x".into() },
            md5: None,
            size: None,
            head_revision_id: None,
            created_time: created.into(),
            modified_time: String::new(),
            trashed: false,
            read_only: false,
            app_properties: Default::default(),
            last_modifier: None,
        }
    }

    #[test]
    fn builds_deterministic_paths() {
        let files = vec![
            f("d1", "root", "Drive/Train", true, "1"),
            f("p2", "d1", "Gear.SLDPRT", false, "3"),
            f("p1", "d1", "gear.sldprt", false, "2"),
            f("m", "root", ".cadsync", true, "0"),
            f("cs", "m", "x.json", false, "0"),
            f("tmp", "d1", "~$Gear.SLDPRT", false, "4"),
            f("orphan", "nowhere", "lost.sldprt", false, "5"),
        ];
        let map = PathMap::build("root", &files);
        assert_eq!(map.path("d1").unwrap(), Path::new("Drive_Train"));
        assert_eq!(map.path("p1").unwrap(), Path::new("Drive_Train/gear.sldprt"));
        assert_eq!(map.path("p2").unwrap(), Path::new("Drive_Train/Gear (2).SLDPRT"));
        assert_eq!(map.renamed, ["p2"]);
        assert_eq!(map.meta_folder_id.as_deref(), Some("m"));
        assert!(!map.contains("cs") && !map.contains("tmp") && !map.contains("orphan"));
        assert_eq!(map.id_for(Path::new("drive_train\\GEAR.sldprt")), Some("p1"));
        assert!(map.is_within("p1", "d1"));

        // Input order doesn't matter.
        let mut rev = files.clone();
        rev.reverse();
        assert_eq!(PathMap::build("root", &rev).path("p2"), map.path("p2"));
    }

    #[test]
    fn finds_vault_wide_duplicate_names() {
        let files = vec![
            f("a", "root", "Intake", true, "1"),
            f("b", "root", "Shooter", true, "2"),
            f("s1", "a", "Spacer.SLDPRT", false, "3"),
            f("s2", "b", "spacer.sldprt", false, "4"),
            f("r1", "a", "README.txt", false, "5"),
            f("r2", "b", "README.txt", false, "6"),
        ];
        let map = PathMap::build("root", &files);
        let dups = map.duplicate_names(&files);
        assert_eq!(dups.len(), 1);
        assert_eq!(dups["spacer.sldprt"], ["s1", "s2"]);
    }
}
