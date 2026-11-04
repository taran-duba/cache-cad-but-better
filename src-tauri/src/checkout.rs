//! Checking out many files at once: a folder, or an assembly plus everything it uses.

use std::collections::BTreeSet;
use std::path::Path;

use futures_util::stream::{self, StreamExt};
use serde::Serialize;

use crate::drive::DriveApi;
use crate::error::{Error, Result};
use crate::names::{cad_kind, CadKind};
use crate::refs::{resolve_ref, RefReader, Resolved};
use crate::sync::{shown, Vault};

/// How many check-outs run at once. Each one waits a moment to confirm its lock, so this is
/// most of the speed-up on big folders.
const PARALLEL: usize = 6;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckOutReport {
    /// Files newly checked out (files you already had aren't counted).
    pub checked_out: usize,
    /// How many of those were added because an assembly references them.
    pub deps_checked_out: usize,
    /// Files a teammate already has checked out (left alone).
    pub locked_by_others: Vec<String>,
    /// References that don't point at a vault file.
    pub missing: usize,
    pub failed: usize,
    pub first_error: Option<String>,
}

impl<D: DriveApi> Vault<D> {
    /// Check out a file and, if it's an assembly, everything it references (recursively), so
    /// nobody has to check out each part by hand. Failing to lock the file itself is an error;
    /// problems with individual references are reported, not fatal.
    pub async fn check_out_with_refs(&self, reader: &impl RefReader, file_id: &str) -> Result<CheckOutReport> {
        self.check_out(file_id).await?;
        self.check_out_many(reader, &[file_id.to_string()]).await
    }

    /// Check out `ids` (several at a time) and everything the assemblies among them reference,
    /// including references outside the folder they came from.
    pub async fn check_out_many(&self, reader: &impl RefReader, ids: &[String]) -> Result<CheckOutReport> {
        let all = self.index.all_files()?;
        let map = self.paths()?;
        let mut report = CheckOutReport::default();
        let mut visited: BTreeSet<String> = ids.iter().cloned().collect();
        let mut level: Vec<String> = ids.to_vec();
        let mut deps = false;
        while !level.is_empty() {
            let results: Vec<(String, Result<bool>)> = stream::iter(level.drain(..))
                .map(|id| async move {
                    let was_mine = self.index.file(&id).ok().flatten().is_some_and(|f| self.i_hold_lock(&f));
                    let r = self.check_out(&id).await.map(|_| !was_mine);
                    (id, r)
                })
                .buffer_unordered(PARALLEL)
                .collect()
                .await;

            let mut next = Vec::new();
            for (id, r) in results {
                let name = map.path(&id).map_or_else(|| id.clone(), shown);
                match r {
                    Ok(newly) => {
                        if newly {
                            report.checked_out += 1;
                            report.deps_checked_out += usize::from(deps);
                        }
                    }
                    Err(Error::Locked { .. }) => report.locked_by_others.push(name.clone()),
                    Err(e) => {
                        report.failed += 1;
                        report.first_error.get_or_insert_with(|| format!("{name}: {e}"));
                        continue;
                    }
                }
                let Some(rel) = map.path(&id) else { continue };
                if cad_kind(&rel.to_string_lossy()) != CadKind::Assembly {
                    continue;
                }
                for stored in self.local_refs(reader, &id, rel).await? {
                    let dep = match resolve_ref(&self.root, &all, &map, &stored) {
                        Resolved::Exact(p) | Resolved::ByName(p) => map.id_for(&p).map(str::to_string),
                        Resolved::Outside(_) => None,
                    };
                    match dep {
                        Some(dep) => {
                            if visited.insert(dep.clone()) {
                                next.push(dep);
                            }
                        }
                        None => report.missing += 1,
                    }
                }
            }
            level = next;
            deps = true;
        }
        Ok(report)
    }

    /// References of an assembly on this PC: asked from SolidWorks, or the saved ones if it can't answer.
    async fn local_refs(&self, reader: &impl RefReader, id: &str, rel: &Path) -> Result<Vec<String>> {
        let abs = self.abs(rel);
        if abs.exists() {
            if let Ok(refs) = reader.refs(&abs).await {
                if !refs.is_empty() {
                    return Ok(refs.into_iter().filter(|r| !r.is_virtual).map(|r| r.path).collect());
                }
            }
        }
        self.index.refs(id)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::drive::fake::FakeDrive;
    use crate::refs::{RefEntry, StaticRefs};
    use crate::sync::testutil::vault;

    fn r(path: &Path) -> RefEntry {
        RefEntry {
            path: path.to_string_lossy().to_string(),
            file_name: path.file_name().unwrap().to_string_lossy().to_string(),
            exists: true,
            is_virtual: false,
        }
    }

    #[tokio::test]
    async fn check_out_takes_referenced_parts_too() {
        let drive = FakeDrive::new("seed@team.org");
        let root = drive.drive_id.clone();
        let arm = drive.seed_folder(&root, "Arm");
        let asm = drive.seed_file(&arm, "Arm.SLDASM", b"a1");
        let gear = drive.seed_file(&arm, "Gear.SLDPRT", b"g1");
        let plate = drive.seed_file(&arm, "Plate.SLDPRT", b"p1");

        let b = vault(&drive, "bo@team.org", "pc-b");
        b.vault.index.set_selection(&root, Some(true)).unwrap();
        b.vault.sync_down().await.unwrap();
        b.vault.check_out(&plate).await.unwrap();

        let a = vault(&drive, "alex@team.org", "pc-a");
        let v = &a.vault;
        v.index.set_selection(&root, Some(true)).unwrap();
        v.sync_down().await.unwrap();
        let reader = StaticRefs(HashMap::from([(
            v.root.join("Arm/Arm.SLDASM"),
            vec![r(&v.root.join("Arm/Gear.SLDPRT")), r(&v.root.join("Arm/Plate.SLDPRT")), r(&v.root.join("Arm/Gone.SLDPRT"))],
        )]));

        let rep = v.check_out_with_refs(&reader, &asm).await.unwrap();
        assert_eq!(rep.deps_checked_out, 1);
        assert_eq!(rep.locked_by_others, ["Arm/Plate.SLDPRT"]);
        assert_eq!(rep.missing, 1);
        assert!(v.i_hold_lock(&v.index.file(&asm).unwrap().unwrap()));
        assert!(v.i_hold_lock(&v.index.file(&gear).unwrap().unwrap()));
    }

    /// Check out all on one folder also takes parts from other folders (and parts referenced by an
    /// old path from before the files moved), and they stay on disk even though their folder isn't ticked.
    #[tokio::test]
    async fn folder_check_out_follows_references_out_of_the_folder() {
        let drive = FakeDrive::new("seed@team.org");
        let root = drive.drive_id.clone();
        let arm = drive.seed_folder(&root, "Arm");
        let lib = drive.seed_folder(&root, "Library");
        let asm = drive.seed_file(&arm, "Arm.SLDASM", b"a1");
        let bolt = drive.seed_file(&lib, "Bolt.SLDPRT", b"b1");
        let nut = drive.seed_file(&lib, "Nut.SLDPRT", b"n1");

        let a = vault(&drive, "alex@team.org", "pc-a");
        let v = &a.vault;
        v.index.set_selection(&arm, Some(true)).unwrap();
        v.sync_down().await.unwrap();
        let reader = StaticRefs(HashMap::from([(
            v.root.join("Arm/Arm.SLDASM"),
            vec![r(&v.root.join("Library/Bolt.SLDPRT")), r(Path::new("C:\\Users\\old\\CacheCAD\\Nut.SLDPRT"))],
        )]));

        let rep = v.check_out_many(&reader, std::slice::from_ref(&asm)).await.unwrap();
        assert_eq!((rep.checked_out, rep.deps_checked_out, rep.missing), (3, 2, 0), "{rep:?}");
        for id in [&asm, &bolt, &nut] {
            assert!(v.i_hold_lock(&v.index.file(id).unwrap().unwrap()));
        }
        v.sync_down().await.unwrap();
        assert!(v.root.join("Library/Bolt.SLDPRT").exists(), "checked-out part kept though Library isn't ticked");
    }
}
