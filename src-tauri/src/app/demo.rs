//! Demo mode (`CADSYNC_DEMO=1` or the "Try the demo" button): an in-memory
//! drive with a small robot, so the UI can be tried without Google setup.

use std::path::Path;
use std::time::Duration;

use crate::drive::fake::FakeDrive;
use crate::drive::{Backend, DriveApi, LockInfo, MetadataPatch, props};
use crate::error::Result;
use crate::index::Index;
use crate::sync::Vault;

pub async fn build(dir: &Path) -> Result<Vault<Backend>> {
    let _ = std::fs::remove_dir_all(dir);
    let root_dir = dir.join("vault");
    std::fs::create_dir_all(&root_dir)?;

    let drive = FakeDrive::new("you@demo.team");
    let root = drive.drive_id.clone();
    let robot = drive.seed_folder(&root, "2026 Robot");
    let dt = drive.seed_folder(&robot, "Drivetrain");
    let arm = drive.seed_folder(&robot, "Arm");
    let cots = drive.seed_folder(&root, "COTS Library");
    drive.seed_file(&robot, "Robot.SLDASM", b"robot assembly v1");
    for n in ["Drivetrain.SLDASM", "Swerve Module.SLDASM", "Frame Rail.SLDPRT", "Belly Pan.SLDPRT"] {
        drive.seed_file(&dt, n, format!("{n} v1").as_bytes());
    }
    for n in ["Arm.SLDASM", "Arm Tube.SLDPRT", "Pivot Plate.SLDPRT", "Spacer.SLDPRT"] {
        drive.seed_file(&arm, n, format!("{n} v1").as_bytes());
    }
    let falcon = drive.seed_file(&cots, "Kraken X60.SLDPRT", b"motor");
    drive.seed_file(&cots, "Spacer.SLDPRT", b"spacer from library");
    drive.seed_file(&dt, "Wiring/Notes.txt", b"name with a slash");

    // A teammate has a file checked out.
    let lock = LockInfo {
        owner: "jordan@demo.team".into(),
        lock_id: "demo".into(),
        // Three days old, so the demo shows a probably-forgotten lock.
        locked_at: (chrono::Utc::now() - chrono::Duration::days(3)).to_rfc3339(),
        machine: "SHOP-PC-2".into(),
    };
    let teammate = drive.as_user("jordan@demo.team");
    let patch = MetadataPatch::default()
        .set(props::LOCK_OWNER, &lock.owner)
        .set(props::LOCK_ID, &lock.lock_id)
        .set(props::LOCKED_AT, &lock.locked_at)
        .set(props::LOCK_MACHINE, &lock.machine)
        .read_only(true, None);
    teammate.update_metadata(&falcon, patch).await?;

    let index = Index::open(&dir.join("index.sqlite"))?;
    index.set_selection(&robot, Some(true))?;
    Ok(Vault {
        drive: Backend::Fake(drive),
        index,
        root_id: root,
        scope: None,
        root: root_dir,
        data_dir: dir.to_path_buf(),
        me: "you@demo.team".into(),
        machine: "THIS-PC".into(),
        lock_verify_delay: Duration::from_millis(300),
    })
}
