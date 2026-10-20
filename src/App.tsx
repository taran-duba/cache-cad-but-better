import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, errorText, PullReport, Status } from "./api";
import { VaultView } from "./views/VaultView";
import { ChangesView } from "./views/ChangesView";
import { HistoryView } from "./views/HistoryView";
import { SettingsView } from "./views/SettingsView";
import { Onboarding } from "./views/Onboarding";
import { Toasts, useToasts } from "./Toasts";
import "./App.css";

type Tab = "vault" | "changes" | "history" | "settings";

export default function App() {
  const [status, setStatus] = useState<Status | null>(null);
  const [tab, setTab] = useState<Tab>("vault");
  const [syncing, setSyncing] = useState(false);
  const [lastSync, setLastSync] = useState<{ at: Date; report?: PullReport; error?: string } | null>(null);
  const [revision, setRevision] = useState(0); // bump to make views reload
  const toasts = useToasts();

  const refreshStatus = useCallback(async () => {
    try {
      setStatus(await api.status());
    } catch (e) {
      toasts.error(errorText(e));
    }
  }, [toasts.error]);

  useEffect(() => {
    // Tell Rust the UI rendered and IPC works (the CI smoke test waits for this).
    refreshStatus().then(() => api.frontendReady().catch(() => {}));
    api
      .takeNewCrashReport()
      .then((p) => p && toasts.warn("cadsync crashed last time. A crash report was saved — Settings › Open logs folder."))
      .catch(() => {});
    const unlisten = [
      listen("vault-changed", () => {
        refreshStatus();
        setRevision((r) => r + 1);
      }),
      listen<PullReport>("sync-report", (e) => {
        setLastSync({ at: new Date(), report: e.payload });
        // Reload even when nothing downloaded: locks and statuses change too.
        setRevision((r) => r + 1);
        refreshStatus();
        e.payload.conflicts.forEach((c) => toasts.warn(`Kept your unsaved edits as ${c}`));
        if (e.payload.restored > 0) toasts.info(restoredText(e.payload.restored));
      }),
      listen<string>("sync-error", (e) => setLastSync({ at: new Date(), error: e.payload })),
    ];
    return () => unlisten.forEach((p) => p.then((f) => f()));
  }, [refreshStatus, toasts.warn]);


  const syncNow = async () => {
    setSyncing(true);
    try {
      const report = await api.syncNow();
      setLastSync({ at: new Date(), report });
      report.errors.forEach((e) => toasts.error(e));
      report.conflicts.forEach((c) => toasts.warn(`Kept your unsaved edits as ${c}`));
      if (report.restored > 0) toasts.info(restoredText(report.restored));
      setRevision((r) => r + 1);
      refreshStatus();
    } catch (e) {
      setLastSync({ at: new Date(), error: errorText(e) });
      toasts.error(errorText(e));
    } finally {
      setSyncing(false);
    }
  };

  const changed = () => {
    setRevision((r) => r + 1);
    refreshStatus();
  };

  const signedIn = !!status?.signedIn;

  if (!status) return <div className="onboarding muted">Starting…</div>;
  if (!signedIn)
    return (
      <>
        <Onboarding status={status} onChanged={changed} toasts={toasts} />
        <Toasts state={toasts} />
      </>
    );

  return (
    <div className="shell">
      <header className="topbar">
        <div className="brand">
          <span className="logo" aria-hidden>
            ◆
          </span>
          cadsync
          {status?.demo && <span className="pill pill-warn">demo</span>}
        </div>
        {signedIn && (
          <div className="topbar-info">
            <span title={status.root ?? ""}>{status.folderName}</span>
            <span className="muted">· {status.me}</span>
          </div>
        )}
        <div className="topbar-actions">
          {signedIn && <SyncState syncing={syncing} last={lastSync} pending={status?.pendingCheckIns ?? 0} />}
          {signedIn && (
            <button className="btn" onClick={() => api.revealRoot().catch((e) => toasts.error(errorText(e)))}>
              Open folder
            </button>
          )}
          {signedIn && (
            <button className="btn btn-primary" onClick={syncNow} disabled={syncing}>
              {syncing ? "Syncing…" : "Sync now"}
            </button>
          )}
        </div>
      </header>

      <nav className="sidenav">
        {(
          [
            ["vault", "Vault"],
            ["changes", "My changes"],
            ["history", "History"],
            ["settings", "Settings"],
          ] as [Tab, string][]
        ).map(([id, label]) => (
          <button
            key={id}
            className={`navitem ${tab === id ? "active" : ""}`}
            onClick={() => setTab(id)}
          >
            {label}
          </button>
        ))}
        {signedIn && !status?.refsAvailable && (
          <p className="nav-note">
            Reference checking is off, so check-ins can't add an assembly's parts for you. Add the team's
            SolidWorks key in Settings (on Windows).
          </p>
        )}
      </nav>

      <main className="content">
        {tab === "vault" && signedIn && <VaultView revision={revision} onChanged={changed} toasts={toasts} />}
        {tab === "changes" && signedIn && <ChangesView revision={revision} onChanged={changed} toasts={toasts} />}
        {tab === "history" && signedIn && <HistoryView revision={revision} />}
        {tab === "settings" && <SettingsView status={status} onChanged={changed} toasts={toasts} />}
      </main>
      <Toasts state={toasts} />
    </div>
  );
}

function SyncState({
  syncing,
  last,
  pending,
}: {
  syncing: boolean;
  last: { at: Date; report?: PullReport; error?: string } | null;
  pending: number;
}) {
  if (syncing) return <span className="muted">Syncing…</span>;
  if (pending > 0) return <span className="pill pill-warn">{pending} check-in(s) waiting to finish</span>;
  if (!last) return null;
  if (last.error)
    return (
      <span className="pill pill-bad" title={last.error}>
        Sync failed — retrying
      </span>
    );
  const waiting = last.report?.deferred.filter((d) => d.reason === "open").length ?? 0;
  return (
    <span className="muted" title={last.at.toLocaleString()}>
      Up to date {last.at.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
      {waiting > 0 && ` · ${waiting} waiting for SolidWorks to close`}
    </span>
  );
}

function restoredText(n: number) {
  return `Put back ${n} file(s) deleted on this computer. To delete a file for the whole team, use Delete in the Vault tab.`;
}
