import { Fragment, useEffect, useState } from "react";
import { ago, api, errorText, HistoryEntry, who } from "../api";
import { Skeleton, Spinner } from "../Spinner";
import type { ToastApi } from "../Toasts";

export function HistoryView({
  revision,
  onChanged,
  toasts,
}: {
  revision: number;
  onChanged: () => void;
  toasts: ToastApi;
}) {
  const [items, setItems] = useState<HistoryEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  /** Which restore is waiting for "are you sure": the push id, plus one file id or null for all. */
  const [confirming, setConfirming] = useState<{ id: string; file: string | null } | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api
      .history(100)
      .then(setItems)
      .catch((e) => setError(errorText(e)));
  }, [revision]);

  const restore = async (m: HistoryEntry, file: string | null) => {
    setConfirming(null);
    setBusy(true);
    try {
      const r = await api.restorePush(m.id, file ? [file] : null);
      toasts.info(
        `Restored ${r.restored.length} file(s) to how "${m.message}" left them, and pushed that for everyone.` +
          (r.unchanged ? ` ${r.unchanged} were already at that version.` : ""),
      );
      onChanged();
    } catch (e) {
      toasts.error(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  if (error) return <p className="notice notice-bad">{error}</p>;
  if (!items) return <Skeleton rows={5} />;
  if (items.length === 0) return <p className="muted pad">No pushes yet.</p>;

  return (
    <section>
      <h2>History</h2>
      <p className="muted small">
        Every push keeps its exact file versions forever. Restoring brings files back to how a push left them (an assembly
        and its parts together) by checking them out and pushing the old versions as a new push, so nothing in History is
        ever lost. File names and locations aren't changed by a restore.
      </p>
      {busy && (
        <p className="status-banner">
          <Spinner size={16} label="Restoring" /> Restoring… keep Permafrost open until this finishes.
        </p>
      )}
      <ul className="history">
        {items.map((m) => (
          <li key={m.id}>
            <div className="history-head">
              <button className="link" onClick={() => setOpen(open === m.id ? null : m.id)} aria-expanded={open === m.id}>
                <strong>{m.message}</strong>
              </button>
              <span className="muted small">
                {who(m.author, m.author_name)} · {ago(m.created_time)} · {m.fileList.length} file(s)
              </span>
            </div>
            {open === m.id && (
              <div className="history-files">
                <ul className="plain">
                  {m.fileList.map((f) => (
                    <li key={f.id}>
                      {f.path ?? <span className="muted">{f.name} (deleted)</span>}{" "}
                      {f.path &&
                        (confirming?.id === m.id && confirming.file === f.id ? (
                          <ConfirmRestore what={f.name} onYes={() => restore(m, f.id)} onNo={() => setConfirming(null)} />
                        ) : (
                          <button
                            className="btn btn-small btn-ghost"
                            disabled={busy}
                            onClick={() => setConfirming({ id: m.id, file: f.id })}
                          >
                            Restore this file
                          </button>
                        ))}
                    </li>
                  ))}
                </ul>
                <div className="row">
                  {confirming?.id === m.id && confirming.file === null ? (
                    <ConfirmRestore what={`all ${m.fileList.length} file(s)`} onYes={() => restore(m, null)} onNo={() => setConfirming(null)} />
                  ) : (
                    <button className="btn btn-small" disabled={busy} onClick={() => setConfirming({ id: m.id, file: null })}>
                      Restore all files from this push
                    </button>
                  )}
                </div>
              </div>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}

function ConfirmRestore({ what, onYes, onNo }: { what: string; onYes: () => void; onNo: () => void }) {
  return (
    <Fragment>
      <span className="small">
        Put {what} back to this version for everyone? Newer versions stay in History.
      </span>{" "}
      <button className="btn btn-small btn-danger" onClick={onYes}>
        Restore
      </button>{" "}
      <button className="btn btn-small btn-ghost" onClick={onNo}>
        Cancel
      </button>
    </Fragment>
  );
}
