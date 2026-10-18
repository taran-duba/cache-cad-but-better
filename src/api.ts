// Typed wrappers around the Rust commands in src-tauri/src/app/commands.rs.
import { invoke } from "@tauri-apps/api/core";

export type CadKind = "part" | "assembly" | "drawing" | "other";
export type FileStatus = "synced" | "outdated" | "not_synced" | "modified" | "pending";

export interface LockInfo {
  owner: string;
  lock_id: string;
  locked_at: string;
  machine: string;
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
  selection: boolean | null;
  selected: boolean;
  status: FileStatus;
}

export interface Status {
  configured: boolean;
  signedIn: boolean;
  demo: boolean;
  me: string | null;
  root: string | null;
  driveId: string | null;
  fileCount: number;
  pendingCheckIns: number;
  refsAvailable: boolean;
}

export interface OAuthClient {
  client_id: string;
  client_secret: string;
}

export interface AppConfig {
  oauth: OAuthClient | null;
  driveId: string | null;
  root: string | null;
  sidecarPath: string | null;
  swdmKey: string | null;
  pollSeconds: number;
}

export type Deferred =
  | { reason: "pending"; file_id: string; changeset: string }
  | { reason: "open"; file_id: string }
  | { reason: "modified_orphan"; file_id: string };

export interface PullReport {
  downloaded: number;
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
  message: string;
  created_time: string;
  files: Record<string, string>;
}

export const api = {
  status: () => invoke<Status>("get_status"),
  getConfig: () => invoke<AppConfig>("get_config"),
  saveConfig: (config: AppConfig) => invoke<void>("save_config", { config }),
  signIn: () => invoke<void>("sign_in"),
  signOut: () => invoke<void>("sign_out"),
  startDemo: () => invoke<void>("start_demo"),
  listChildren: (parentId: string | null) => invoke<Entry[]>("list_children", { parentId }),
  setSelection: (fileId: string, get: boolean | null) => invoke<void>("set_selection", { fileId, get }),
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
  history: (limit?: number) => invoke<Manifest[]>("history", { limit }),
  revealRoot: () => invoke<void>("reveal_root"),
  frontendReady: () => invoke<void>("frontend_ready"),
};

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

export function who(email: string | null | undefined): string {
  return email ? email.split("@")[0] : "someone";
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
