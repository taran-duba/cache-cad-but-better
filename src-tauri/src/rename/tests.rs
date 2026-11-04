use std::collections::HashMap;
use std::sync::Mutex;

use super::*;
use crate::drive::fake::FakeDrive;
use crate::refs::{RefEntry, StaticRefs};
use crate::sync::testutil::{vault, Fixture};

/// Stands in for SolidWorks: records each rewrite and marks the document's content.
#[derive(Default)]
struct Writer {
    calls: Mutex<Vec<(PathBuf, String, PathBuf)>>,
    fail: bool,
}

impl RefWriter for Writer {
    async fn replace_ref(&self, doc: &Path, old: &str, new: &Path) -> Result<()> {
        if self.fail {
            return Err(Error::Refs("SolidWorks said no".into()));
        }
        assert!(new.exists(), "the renamed file is in place before documents are rewritten");
        let mut bytes = std::fs::read(doc)?;
        bytes.extend_from_slice(format!("+{}", new.file_name().unwrap().to_string_lossy()).as_bytes());
        std::fs::write(doc, bytes)?;
        self.calls.lock().unwrap().push((doc.to_path_buf(), old.to_string(), new.to_path_buf()));
        Ok(())
    }
}

fn r(path: &Path) -> RefEntry {
    RefEntry {
        path: path.to_string_lossy().to_string(),
        file_name: path.file_name().unwrap().to_string_lossy().to_string(),
        exists: true,
        is_virtual: false,
    }
}

struct Team {
    drive: FakeDrive,
    arm: String,
    asm: String,
    gear: String,
    bot: String,
    a: Fixture,
    b: Fixture,
}

/// Arm/Arm.SLDASM uses Arm/Gear.SLDPRT by its exact path; Bot.SLDASM uses it through an old
/// CacheCAD path. Alex and Blair both have everything.
async fn team() -> Team {
    let drive = FakeDrive::new("seed@team.org");
    let root = drive.drive_id.clone();
    let arm = drive.seed_folder(&root, "Arm");
    let asm = drive.seed_file(&arm, "Arm.SLDASM", b"arm");
    let gear = drive.seed_file(&arm, "Gear.SLDPRT", b"gear");
    let bot = drive.seed_file(&root, "Bot.SLDASM", b"bot");
    drive.seed_file(&root, "notes.txt", b"hi");
    let a = vault(&drive, "alex@team.org", "pc-a");
    let b = vault(&drive, "blair@team.org", "pc-b");
    for v in [&a.vault, &b.vault] {
        v.index.set_selection(&v.root_id, Some(true)).unwrap();
        v.sync_down().await.unwrap();
    }
    Team { drive, arm, asm, gear, bot, a, b }
}

fn reader(v: &Vault<FakeDrive>) -> StaticRefs {
    StaticRefs(HashMap::from([
        (v.root.join("Arm/Arm.SLDASM"), vec![r(&v.root.join("Arm/Gear.SLDPRT"))]),
        (v.root.join("Bot.SLDASM"), vec![r(Path::new("C:\\Users\\old\\CacheCAD\\Gear.SLDPRT")), r(&v.root.join("Arm/Arm.SLDASM"))]),
    ]))
}

fn locked_by_anyone(v: &Vault<FakeDrive>, id: &str) -> bool {
    v.index.file(id).unwrap().unwrap().lock().is_some()
}

#[tokio::test]
async fn renaming_a_part_updates_every_assembly_and_teammates_get_it_all_together() {
    let t = team().await;
    let av = &t.a.vault;
    let w = Writer::default();
    let rep = av.rename(&reader(av), Some(&w), &t.gear, Path::new("Arm/Spur.SLDPRT")).await.unwrap();

    assert_eq!(rep.updated.len(), 2, "{rep:?}");
    let calls = w.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 2);
    assert!(calls.iter().any(|(_, old, _)| old == "C:\\Users\\old\\CacheCAD\\Gear.SLDPRT"), "old-path reference rewritten too");
    assert!(calls.iter().all(|(_, _, new)| *new == av.root.join("Arm/Spur.SLDPRT")));

    let g = t.drive.get_file(&t.gear).await.unwrap();
    assert_eq!((g.name.as_str(), g.parent.as_deref()), ("Spur.SLDPRT", Some(t.arm.as_str())));
    assert!(av.root.join("Arm/Spur.SLDPRT").exists() && !av.root.join("Arm/Gear.SLDPRT").exists());
    for id in [&t.gear, &t.asm, &t.bot] {
        assert!(!locked_by_anyone(av, id), "everything released");
    }

    let bv = &t.b.vault;
    bv.sync_down().await.unwrap();
    assert!(bv.root.join("Arm/Spur.SLDPRT").exists() && !bv.root.join("Arm/Gear.SLDPRT").exists());
    assert_eq!(std::fs::read(bv.root.join("Arm/Arm.SLDASM")).unwrap(), b"arm+Spur.SLDPRT");
    assert_eq!(std::fs::read(bv.root.join("Bot.SLDASM")).unwrap(), b"bot+Spur.SLDPRT");
    assert_eq!(bv.index.recent_changesets(10).unwrap()[0].message, "Renamed Arm/Gear.SLDPRT to Arm/Spur.SLDPRT");
}

#[tokio::test]
async fn moving_a_folder_updates_documents_inside_and_outside_it() {
    let t = team().await;
    let av = &t.a.vault;
    let w = Writer::default();
    let rep = av.rename(&reader(av), Some(&w), &t.arm, Path::new("Robot/Arm")).await.unwrap();

    // Arm.SLDASM (moved itself) uses the gear; Bot uses both.
    let mut updated = rep.updated.clone();
    updated.sort();
    assert_eq!(updated, ["Bot.SLDASM", "Robot/Arm/Arm.SLDASM"]);
    assert_eq!(w.calls.lock().unwrap().len(), 3);

    let bv = &t.b.vault;
    bv.sync_down().await.unwrap();
    assert_eq!(std::fs::read(bv.root.join("Robot/Arm/Arm.SLDASM")).unwrap(), b"arm+Gear.SLDPRT");
    assert!(bv.root.join("Robot/Arm/Gear.SLDPRT").exists());
    assert!(!bv.root.join("Arm").join("Gear.SLDPRT").exists());
}

#[tokio::test]
async fn a_teammate_holding_an_assembly_that_uses_it_blocks_the_rename_and_nothing_changes() {
    let t = team().await;
    t.b.vault.check_out(&t.bot).await.unwrap();
    let av = &t.a.vault;
    av.refresh().await.unwrap();
    let w = Writer::default();
    let e = av.rename(&reader(av), Some(&w), &t.gear, Path::new("Arm/Spur.SLDPRT")).await.unwrap_err();
    assert!(e.to_string().contains("Bot.SLDASM is checked out by"), "{e}");
    assert!(w.calls.lock().unwrap().is_empty());
    assert_eq!(t.drive.get_file(&t.gear).await.unwrap().name, "Gear.SLDPRT");
    assert!(av.root.join("Arm/Gear.SLDPRT").exists());
    assert!(!locked_by_anyone(av, &t.gear));
}

#[tokio::test]
async fn a_failed_rewrite_puts_everything_back() {
    let t = team().await;
    let av = &t.a.vault;
    let w = Writer { fail: true, ..Default::default() };
    assert!(av.rename(&reader(av), Some(&w), &t.gear, Path::new("Arm/Spur.SLDPRT")).await.is_err());

    assert!(av.root.join("Arm/Gear.SLDPRT").exists() && !av.root.join("Arm/Spur.SLDPRT").exists());
    assert_eq!(Path::new(&av.index.local(&t.gear).unwrap().unwrap().rel_path), Path::new("Arm/Gear.SLDPRT"));
    assert_eq!(t.drive.get_file(&t.gear).await.unwrap().name, "Gear.SLDPRT");
    for id in [&t.gear, &t.asm, &t.bot] {
        assert!(!t.drive.get_file(id).await.unwrap().lock().is_some(), "lock released");
    }
    assert_eq!(std::fs::read(av.root.join("Arm/Arm.SLDASM")).unwrap(), b"arm");
}

#[tokio::test]
async fn teammates_hold_the_move_back_until_the_whole_rename_is_pushed() {
    let t = team().await;
    let av = &t.a.vault;
    let w = Writer::default();
    t.drive.crash_after_uploads(0); // the move reaches Drive, then the uploads "crash"
    let e = av.rename(&reader(av), Some(&w), &t.gear, Path::new("Arm/Spur.SLDPRT")).await.unwrap_err();
    assert!(e.to_string().contains("finishes it automatically"), "{e}");
    assert_eq!(t.drive.get_file(&t.gear).await.unwrap().name, "Spur.SLDPRT", "move is on Drive");

    let bv = &t.b.vault;
    bv.sync_down().await.unwrap();
    assert!(bv.root.join("Arm/Gear.SLDPRT").exists(), "Blair keeps the old name until the push commits");

    t.drive.clear_faults();
    av.resume_pushes().await.unwrap();
    bv.sync_down().await.unwrap();
    assert!(bv.root.join("Arm/Spur.SLDPRT").exists() && !bv.root.join("Arm/Gear.SLDPRT").exists());
    assert_eq!(std::fs::read(bv.root.join("Arm/Arm.SLDASM")).unwrap(), b"arm+Spur.SLDPRT");
}

#[tokio::test]
async fn without_solidworks_only_non_cad_files_can_move() {
    let t = team().await;
    let av = &t.a.vault;
    let none: Option<&Writer> = None;
    let e = av.rename(&reader(av), none, &t.gear, Path::new("Arm/Spur.SLDPRT")).await.unwrap_err();
    assert!(e.to_string().contains("needs SolidWorks"), "{e}");

    let notes = av.paths().unwrap().id_for(Path::new("notes.txt")).unwrap().to_string();
    av.rename(&reader(av), none, &notes, Path::new("Docs/notes.txt")).await.unwrap();
    assert!(av.root.join("Docs/notes.txt").exists());
    t.b.vault.sync_down().await.unwrap();
    assert!(t.b.vault.root.join("Docs/notes.txt").exists());
}

#[tokio::test]
async fn rejects_bad_new_paths() {
    let t = team().await;
    let av = &t.a.vault;
    let w = Writer::default();
    for bad in ["Arm/Arm.SLDASM", "../Gear.SLDPRT", "Arm/Gear?.SLDPRT", "Arm/Gear.SLDASM", "Arm/Gear.SLDPRT"] {
        let e = av.rename(&reader(av), Some(&w), &t.gear, Path::new(bad)).await.unwrap_err();
        assert!(e.to_string().starts_with("can't use") || e.to_string().contains("can't use"), "{bad}: {e}");
    }
    let e = av.rename(&reader(av), Some(&w), &t.arm, Path::new("Arm/Inner")).await.unwrap_err();
    assert!(e.to_string().contains("inside itself"), "{e}");
}
