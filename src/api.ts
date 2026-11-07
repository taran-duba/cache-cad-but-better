// Typed wrappers around the Rust commands in src-tauri/src/app/commands.rs.
import { invoke } from "@tauri-apps/api/core";

export type CadKind = "part" | "assembly" | "drawing" | "other";
export type FileStatus = "synced" | "outdated" | "not_synced" | "modified" | "pending";

export interface LockInfo {
  owner: string;
  owner_name: string;
  lock_id: string;
  locked_at: string;
  machine: string;
}

export interface AreaInfo {
  id: string;
  /** Path of the area's folder or file in the CAD folder ("" = the whole CAD folder). */
  path: string;
  /** Who may edit it besides CADmins. Empty = CADmins only. */
  editors: string[];
}

export interface Team {
  admins: string[];
  areas: AreaInfo[];
}

export interface Entry {
  id: string;
  name: string;
  localName: string | null;
  isFolder: boolean;
  kind: CadKind;
  size: number | null;
  modifiedTime: string;
  lastModifier: string | null;
  lock: LockInfo | null;
  mine: boolean;
  /** You may check it out: its area lets you, or you're a CADmin. */
  canEdit: boolean;
  /** The area that decides who edits it (only while roles are on). */
  area: AreaInfo | null;
  status: FileStatus;
}

export interface ProjectInfo {
  id: string;
  name: string;
  /** Where it lives on this PC. */
  root: string;
  active: boolean;
}

export interface Status {
  signInAvailable: boolean;
  account: string | null;
  accountName: string | null;
  signedIn: boolean;
  signInExpired: boolean;
  restoring: boolean;
  syncing: boolean;
  demo: boolean;
  me: string | null;
  root: string | null;
  folderId: string | null;
  folderName: string | null;
  fileCount: number;
  pendingCheckIns: number;
  refsAvailable: boolean;
  refsEnabled: boolean;
  /** The team has CADmins, so areas limit who edits what. */
  rolesOn: boolean;
  /** You're a CADmin (always true while roles are off). */
  isAdmin: boolean;
  /** Every project this PC has opened; the open one has `active` set. */
  projects: ProjectInfo[];
}

export interface OAuthClient {
  client_id: string;
  client_secret: string;
}

export interface AppConfig {
  oauth: OAuthClient | null;
  folderId: string | null;
  root: string | null;
  sidecarPath: string | null;
}

export interface Location {
  id: string;
  name: string;
  kind: "shared_drive" | "my_drive" | "shared_with_me";
}

export interface BrowseFolder {
  id: string;
  name: string;
}

export type Deferred =
  | { reason: "pending"; file_id: string; changeset: string }
  | { reason: "open"; file_id: string }
  | { reason: "modified_orphan"; file_id: string };

export interface CheckOutReport {
  /** Newly checked out, not counting files you already had. */
  checkedOut: number;
  /** How many of those were added because an assembly uses them. */
  depsCheckedOut: number;
  lockedByOthers: string[];
  /** In areas you can't edit, so left read-only. */
  notYours: string[];
  missing: number;
  failed: number;
  firstError: string | null;
}

export interface RenameReport {
  newPath: string;
  updated: string[];
  changeset: string;
}

export interface RestoreReport {
  changeset: string;
  restored: string[];
  unchanged: number;
}

export interface PullReport {
  downloaded: number;
  restored: number;
  moved: number;
  removed: number;
  conflicts: string[];
  errors: string[];
  deferred: Deferred[];
}

export type Problem =
  | { kind: "missing_reference"; from: string; reference: string }
  | { kind: "outside_vault"; from: string; reference: string; in_vault_match: string | null }
  | { kind: "duplicate_name"; from: string; name: string; copies: string[] }
  | { kind: "modified_not_checked_out"; from: string; dependency: string };

export interface CheckInPlan {
  files: string[];
  new_files: string[];
  auto_included: string[];
  matched_by_name: { from: string; reference: string; found: string }[];
  unreadable: { file: string; error: string; used_saved: boolean }[];
  problems: Problem[];
}

export interface CheckInPreview {
  plan: CheckInPlan;
  refsChecked: boolean;
}

export interface MyChanges {
  checkedOut: Entry[];
  newFiles: string[];
}

export interface Manifest {
  id: string;
  author: string;
  author_name: string;
  message: string;
  created_time: string;
  files: Record<string, string>;
}

export interface HistoryEntry extends Manifest {
  fileList: { id: string; path: string | null; name: string }[];
}

export const api = {
  status: () => invoke<Status>("get_status"),
  setRefsEnabled: (enabled: boolean) => invoke<void>("set_refs_enabled", { enabled }),
  getConfig: () => invoke<AppConfig>("get_config"),
  saveConfig: (config: AppConfig) => invoke<void>("save_config", { config }),
  signIn: () => invoke<void>("sign_in"),
  browseLocations: () => invoke<Location[]>("browse_locations"),
  browseFolders: (parentId: string) => invoke<BrowseFolder[]>("browse_folders", { parentId }),
  chooseFolder: (input: string) => invoke<void>("choose_folder", { input }),
  openProject: (folderId: string) => invoke<void>("open_project", { folderId }),
  removeProject: (folderId: string) => invoke<void>("remove_project", { folderId }),
  switchDrive: () => invoke<void>("switch_drive"),
  setVaultFolder: (root: string) => invoke<void>("set_vault_folder", { root }),
  signOut: () => invoke<void>("sign_out"),
  startDemo: () => invoke<void>("start_demo"),
  listChildren: (parentId: string | null) => invoke<Entry[]>("list_children", { parentId }),
  checkOutWithRefs: (fileId: string) => invoke<CheckOutReport>("check_out_with_refs", { fileId }),
  checkOutAll: (folderId: string | null) => invoke<CheckOutReport>("check_out_all", { folderId }),
  renameItem: (fileId: string, newPath: string) => invoke<RenameReport>("rename_item", { fileId, newPath }),
  restorePush: (changesetId: string, files: string[] | null) =>
    invoke<RestoreReport>("restore_push", { changesetId, files }),
  syncNow: () => invoke<PullReport>("sync_now"),
  checkOut: (fileId: string) => invoke<LockInfo>("check_out", { fileId }),
  undoCheckOut: (fileId: string, discardChanges: boolean) =>
    invoke<void>("undo_check_out", { fileId, discardChanges }),
  breakLock: (fileId: string) => invoke<void>("break_lock", { fileId }),
  myChanges: () => invoke<MyChanges>("my_changes"),
  previewCheckIn: (files: string[], newFiles: string[]) =>
    invoke<CheckInPreview>("preview_check_in", { files, newFiles }),
  checkIn: (args: {
    files: string[];
    newFiles: string[];
    message: string;
    keepLocked: boolean;
    allowDuplicateNames: boolean;
  }) => invoke<{ changeset: string; files: number }>("check_in", { args }),
  history: (limit?: number) => invoke<HistoryEntry[]>("history", { limit }),
  openFile: (fileId: string) => invoke<void>("open_file", { fileId }),
  revealRoot: () => invoke<void>("reveal_root"),
  frontendReady: () => invoke<boolean>("frontend_ready"),
  smokeReport: (steps: { name: string; ok: boolean; detail?: string }[]) => invoke<void>("smoke_report", { steps }),
  deleteFromDrive: (fileId: string, force: boolean) =>
    invoke<{ name: string; local_copy: string | null }>("delete_from_drive", { fileId, force }),
  openLogs: () => invoke<void>("open_logs"),
  getTeam: () => invoke<Team>("get_team"),
  setAdmins: (admins: string[]) => invoke<Team>("set_admins", { admins }),
  /** Give a folder or file its own editors; null removes the area. */
  setArea: (fileId: string, editors: string[] | null) => invoke<Team>("set_area", { fileId, editors }),
  openInDrive: (fileId: string | null) => invoke<void>("open_in_drive", { fileId }),
  createDriveFolder: (parentId: string, name: string) =>
    invoke<BrowseFolder>("create_drive_folder", { parentId, name }),
  takeNewCrashReport: () => invoke<string | null>("take_new_crash_report"),
};

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

/** How to name a teammate: their Google name, else (older data) the part of their email before the @. */
export function who(email: string | null | undefined, name?: string | null): string {
  return name || (email ? email.split("@")[0] : "someone");
}

/** "alex, blair" from emails; "CADmins only" for nobody. */
export function editorsText(editors: string[]): string {
  return editors.length ? editors.map((e) => who(e)).join(", ") : "CADmins only";
}

/** Split typed emails on commas, semicolons, spaces or new lines. */
export function parseEmails(text: string): string[] {
  return text
    .split(/[\s,;]+/)
    .map((e) => e.trim())
    .filter(Boolean);
}

export function ago(iso: string): string {
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return "";
  const s = Math.round((Date.now() - t) / 1000);
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
  return `${Math.floor(s / 86400)} d ago`;
}
