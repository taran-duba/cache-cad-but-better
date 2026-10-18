//! Reading SolidWorks references and checking a check-in before it happens.
//!
//! `SidecarRefs` talks to the SwRefs helper (sidecar/SwRefs), which uses the
//! SolidWorks Document Manager API. Protocol: one JSON object per line on
//! stdin/stdout, e.g. `{"id":"1","op":"refs","path":"C:\\CAD\\..\\Robot.SLDASM"}`
//! → `{"id":"1","ok":true,"refs":[{"path":..,"fileName":..,"exists":..,"isVirtual":..}]}`.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

use crate::drive::DriveApi;
use crate::error::{Error, Result};
use crate::names::{cad_kind, CadKind};
use crate::sync::{localfs, Vault};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefEntry {
    pub path: String,
    pub file_name: String,
    #[serde(default)]
    pub exists: bool,
    #[serde(default)]
    pub is_virtual: bool,
}

#[allow(async_fn_in_trait)]
pub trait RefReader: Send + Sync {
    /// Direct references of one SolidWorks document.
    async fn refs(&self, path: &Path) -> Result<Vec<RefEntry>>;
}

pub struct SidecarRefs {
    exe: PathBuf,
    license_key: String,
    proc: Mutex<Option<(Child, ChildStdin, BufReader<ChildStdout>)>>,
    next_id: std::sync::atomic::AtomicU64,
}

#[derive(Deserialize)]
struct SidecarResponse {
    ok: bool,
    #[serde(default)]
    refs: Vec<RefEntry>,
    error: Option<String>,
}

impl SidecarRefs {
    /// `license_key` is the team's Document Manager key, passed to the
    /// sidecar through its environment (never on the command line).
    pub fn new(exe: PathBuf, license_key: String) -> Self {
        Self { exe, license_key, proc: Mutex::new(None), next_id: Default::default() }
    }
}

impl RefReader for SidecarRefs {
    async fn refs(&self, path: &Path) -> Result<Vec<RefEntry>> {
        let mut guard = self.proc.lock().await;
        if guard.is_none() {
            let mut child = Command::new(&self.exe)
                .env("CADSYNC_SWDM_KEY", &self.license_key)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .kill_on_drop(true)
                .spawn()
                .map_err(|e| Error::Refs(format!("can't start {}: {e}", self.exe.display())))?;
            let stdin = child.stdin.take().expect("piped");
            let stdout = BufReader::new(child.stdout.take().expect("piped"));
            *guard = Some((child, stdin, stdout));
        }
        let (_, stdin, stdout) = guard.as_mut().expect("just set");
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed).to_string();
        let req = serde_json::json!({ "id": id, "op": "refs", "path": path });
        let io = async {
            stdin.write_all(format!("{req}\n").as_bytes()).await?;
            stdin.flush().await?;
            let mut line = String::new();
            stdout.read_line(&mut line).await?;
            Ok::<_, std::io::Error>(line)
        };
        let line = match io.await {
            Ok(l) if !l.is_empty() => l,
            other => {
                *guard = None; // restart next time
                return Err(Error::Refs(format!("sidecar stopped responding: {other:?}")));
            }
        };
        let resp: SidecarResponse = serde_json::from_str(&line)?;
        if resp.ok {
            Ok(resp.refs)
        } else {
            Err(Error::Refs(resp.error.unwrap_or_else(|| "unknown error".into())))
        }
    }
}

/// Canned references, for tests and for running without the sidecar.
#[derive(Default)]
pub struct StaticRefs(pub HashMap<PathBuf, Vec<RefEntry>>);

impl RefReader for StaticRefs {
    async fn refs(&self, path: &Path) -> Result<Vec<RefEntry>> {
        Ok(self.0.get(path).cloned().unwrap_or_default())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Problem {
    /// The referenced file isn't on Drive or on this machine.
    MissingReference { from: PathBuf, reference: String },
    /// The reference points outside the vault (Desktop, Downloads, Toolbox...),
    /// so it will be missing on everyone else's computer.
    OutsideVault { from: PathBuf, reference: String, in_vault_match: Option<PathBuf> },
    /// A referenced file name exists in more than one vault folder; SolidWorks
    /// may load the wrong one and break mates.
    DuplicateName { from: PathBuf, name: String, copies: Vec<PathBuf> },
    /// A dependency was edited but isn't checked out by you, so it can't be
    /// included.
    ModifiedNotCheckedOut { from: PathBuf, dependency: PathBuf },
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CheckInPlan {
    pub files: Vec<String>,
    pub new_files: Vec<PathBuf>,
    /// Paths added automatically because a selected assembly needs them.
    pub auto_included: Vec<PathBuf>,
    pub problems: Vec<Problem>,
}

/// Case-insensitive, slash-agnostic relative path of `p` under `root`.
fn relative_to(root: &Path, p: &str) -> Option<PathBuf> {
    let norm = |s: &str| s.replace('\\', "/").trim_end_matches('/').to_lowercase();
    let (root_s, p_s) = (norm(&root.to_string_lossy()), norm(p));
    let rest = p_s.strip_prefix(&root_s)?.strip_prefix('/')?;
    // Keep the original casing when lowercasing didn't change byte lengths.
    let original = p.trim_end_matches(['/', '\\']);
    let tail = original
        .get(original.len().saturating_sub(rest.len())..)
        .filter(|t| t.replace('\\', "/").to_lowercase() == rest)
        .unwrap_or(rest);
    Some(PathBuf::from(tail.replace('\\', "/")))
}

impl<D: DriveApi> Vault<D> {
    /// Work out what a check-in really needs: walk the reference tree of every
    /// selected assembly/drawing, pull in modified dependencies that are
    /// checked out by me (or brand new), and list anything that would break
    /// mates for teammates.
    pub async fn prepare_check_in(
        &self,
        reader: &impl RefReader,
        files: &[String],
        new_files: &[PathBuf],
    ) -> Result<CheckInPlan> {
        let all = self.index.all_files()?;
        let map = crate::index::PathMap::build(&self.drive_id, &all);
        let dups = map.duplicate_names(&all);
        let by_id: HashMap<&str, _> = all.iter().map(|f| (f.id.as_str(), f)).collect();

        let mut plan = CheckInPlan::default();
        let mut chosen_ids: BTreeSet<String> = files.iter().cloned().collect();
        let mut chosen_new: BTreeSet<PathBuf> = new_files.iter().cloned().collect();
        let mut queue: Vec<PathBuf> = files
            .iter()
            .filter_map(|id| map.path(id).map(Path::to_path_buf))
            .chain(new_files.iter().cloned())
            .collect();
        let mut visited = BTreeSet::new();

        while let Some(rel) = queue.pop() {
            if !visited.insert(rel.to_string_lossy().to_lowercase()) {
                continue;
            }
            if !matches!(cad_kind(&rel.to_string_lossy()), CadKind::Assembly | CadKind::Drawing) {
                continue;
            }
            let abs = self.abs(&rel);
            if !abs.exists() {
                continue; // not synced here; its references didn't change
            }
            let refs = reader.refs(&abs).await?;
            if let Some(id) = map.id_for(&rel) {
                self.index.set_refs(id, &refs.iter().map(|r| r.path.clone()).collect::<Vec<_>>())?;
            }
            for r in refs.iter().filter(|r| !r.is_virtual) {
                let Some(dep_rel) = relative_to(&self.root, &r.path) else {
                    let in_vault_match = all
                        .iter()
                        .find(|f| f.name.eq_ignore_ascii_case(&r.file_name) && map.contains(&f.id))
                        .and_then(|f| map.path(&f.id).map(Path::to_path_buf));
                    plan.problems.push(Problem::OutsideVault {
                        from: rel.clone(),
                        reference: r.path.clone(),
                        in_vault_match,
                    });
                    continue;
                };
                if let Some(ids) = dups.get(&r.file_name.to_lowercase()) {
                    plan.problems.push(Problem::DuplicateName {
                        from: rel.clone(),
                        name: r.file_name.clone(),
                        copies: ids.iter().filter_map(|i| map.path(i).map(Path::to_path_buf)).collect(),
                    });
                }
                match map.id_for(&dep_rel) {
                    Some(dep_id) => {
                        let dep_id = dep_id.to_string();
                        let modified = match self.index.local(&dep_id)? {
                            Some(l) => {
                                let fp = l.synced_size.zip(l.synced_mtime);
                                localfs::is_modified(&self.abs(&dep_rel), l.synced_md5.as_deref(), fp)?
                            }
                            None => false,
                        };
                        if modified && !chosen_ids.contains(&dep_id) {
                            let mine = by_id.get(dep_id.as_str()).is_some_and(|f| self.i_hold_lock(f));
                            if mine {
                                chosen_ids.insert(dep_id);
                                plan.auto_included.push(dep_rel.clone());
                            } else {
                                plan.problems.push(Problem::ModifiedNotCheckedOut {
                                    from: rel.clone(),
                                    dependency: dep_rel.clone(),
                                });
                            }
                        }
                    }
                    None if self.abs(&dep_rel).is_file() => {
                        if chosen_new.insert(dep_rel.clone()) {
                            plan.auto_included.push(dep_rel.clone());
                        }
                    }
                    None => plan.problems.push(Problem::MissingReference {
                        from: rel.clone(),
                        reference: r.path.clone(),
                    }),
                }
                queue.push(dep_rel);
            }
        }
        plan.files = chosen_ids.into_iter().collect();
        plan.new_files = chosen_new.into_iter().collect();
        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drive::fake::FakeDrive;
    use crate::sync::testutil::vault;

    fn r(path: &Path) -> RefEntry {
        let s = path.to_string_lossy().to_string();
        RefEntry {
            file_name: s.rsplit(['/', '\\']).next().unwrap().to_string(),
            path: s,
            exists: true,
            is_virtual: false,
        }
    }

    #[test]
    fn relative_paths_ignore_case_and_slashes() {
        let root = Path::new("C:/CAD/Team Drive");
        assert_eq!(relative_to(root, "c:\\cad\\team drive\\Arm\\Gear.SLDPRT"), Some("Arm/Gear.SLDPRT".into()));
        assert_eq!(relative_to(root, "C:\\Users\\alex\\Desktop\\Gear.SLDPRT"), None);
        assert_eq!(relative_to(root, "C:/CAD/Team Drive2/x.sldprt"), None);
    }

    #[tokio::test]
    async fn includes_dependencies_and_flags_problems() {
        let drive = FakeDrive::new("seed@team.org");
        let root = drive.drive_id.clone();
        let arm = drive.seed_folder(&root, "Arm");
        let intake = drive.seed_folder(&root, "Intake");
        let asm = drive.seed_file(&arm, "Arm.SLDASM", b"a1");
        let gear = drive.seed_file(&arm, "Gear.SLDPRT", b"g1");
        let plate = drive.seed_file(&arm, "Plate.SLDPRT", b"p1");
        drive.seed_file(&arm, "Spacer.SLDPRT", b"s1");
        drive.seed_file(&intake, "Spacer.SLDPRT", b"s1");

        let a = vault(&drive, "alex@team.org", "pc-a");
        let v = &a.vault;
        v.index.set_selection(&root, Some(true)).unwrap();
        v.sync_down().await.unwrap();
        v.check_out(&asm).await.unwrap();
        v.check_out(&gear).await.unwrap();
        std::fs::write(v.root.join("Arm/Gear.SLDPRT"), b"g2").unwrap();
        std::fs::write(v.root.join("Arm/Arm.SLDASM"), b"a2").unwrap();
        std::fs::write(v.root.join("Arm/NewBracket.SLDPRT"), b"new").unwrap();
        // Plate edited without a check-out (read-only cleared by hand).
        localfs::set_read_only(&v.root.join("Arm/Plate.SLDPRT"), false).unwrap();
        std::fs::write(v.root.join("Arm/Plate.SLDPRT"), b"p2").unwrap();

        let asm_abs = v.root.join("Arm/Arm.SLDASM");
        let reader = StaticRefs(HashMap::from([(
            asm_abs,
            vec![
                r(&v.root.join("Arm/Gear.SLDPRT")),
                r(&v.root.join("Arm/Plate.SLDPRT")),
                r(&v.root.join("Arm/NewBracket.SLDPRT")),
                r(&v.root.join("Arm/Spacer.SLDPRT")),
                r(&v.root.join("Arm/Gone.SLDPRT")),
                r(Path::new("C:\\Users\\alex\\Desktop\\Gear.SLDPRT")),
            ],
        )]));

        let plan = v.prepare_check_in(&reader, std::slice::from_ref(&asm), &[]).await.unwrap();
        assert!(plan.files.contains(&gear), "modified checked-out part auto-included");
        assert!(!plan.files.contains(&plate));
        assert_eq!(plan.new_files, [PathBuf::from("Arm/NewBracket.SLDPRT")]);
        assert_eq!(plan.auto_included.len(), 2);

        let kinds: Vec<_> = plan
            .problems
            .iter()
            .map(|p| match p {
                Problem::MissingReference { .. } => "missing",
                Problem::OutsideVault { .. } => "outside",
                Problem::DuplicateName { .. } => "duplicate",
                Problem::ModifiedNotCheckedOut { .. } => "not_checked_out",
            })
            .collect();
        assert_eq!(kinds.len(), 4, "{:?}", plan.problems);
        for k in ["missing", "outside", "duplicate", "not_checked_out"] {
            assert!(kinds.contains(&k), "{k} in {kinds:?}");
        }
        if let Some(Problem::OutsideVault { in_vault_match, .. }) =
            plan.problems.iter().find(|p| matches!(p, Problem::OutsideVault { .. }))
        {
            assert_eq!(in_vault_match.as_deref(), Some(Path::new("Arm/Gear.SLDPRT")));
        }
        assert_eq!(v.index.refs(&asm).unwrap().len(), 6, "reference graph recorded");
    }
}
