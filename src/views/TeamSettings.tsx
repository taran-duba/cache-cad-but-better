import { useEffect, useState } from "react";
import { api, editorsText, errorText, parseEmails, Status, Team, who } from "../api";
import type { ToastApi } from "../Toasts";

/** Settings › Team: CADmins, and which areas of the CAD folder only certain people edit. */
export function TeamSettings({ status, onChanged, toasts }: { status: Status; onChanged: () => void; toasts: ToastApi }) {
  const [team, setTeam] = useState<Team | null>(null);
  const [adding, setAdding] = useState("");
  const [busy, setBusy] = useState(false);
  const [confirmOff, setConfirmOff] = useState(false);
  const me = (status.me ?? "").toLowerCase();
  const canChange = status.isAdmin;

  useEffect(() => {
    api.getTeam().then(setTeam).catch((e) => toasts.error(errorText(e)));
  }, [status.rolesOn, status.isAdmin]);

  const run = async (what: () => Promise<Team>, done: string) => {
    setBusy(true);
    try {
      setTeam(await what());
      toasts.info(done);
      onChanged();
      return true;
    } catch (e) {
      toasts.error(errorText(e));
      return false;
    } finally {
      setBusy(false);
      setConfirmOff(false);
    }
  };

  if (!team) return null;

  if (!status.rolesOn)
    return (
      <div className="setting-row">
        <div>
          <div className="setting-label">Team roles</div>
          <div className="muted small">
            Off: anyone can check out anything. Turn roles on to make yourself a CADmin. CADmins can edit everything,
            including the top-level robot assembly, and choose who edits each subsystem folder or assembly (Vault ›
            Editors). Everyone still gets every file; files you can't edit stay read-only for you.
          </div>
        </div>
        <button
          className="btn"
          disabled={busy || !status.me}
          onClick={() => run(() => api.setAdmins([status.me!]), "Roles are on. You're a CADmin.")}
        >
          Make me a CADmin
        </button>
      </div>
    );

  return (
    <div className="setting-block">
      <div className="setting-label">CADmins</div>
      <p className="muted small">
        CADmins can edit everything, including the top-level assembly, take over any check-out, and choose who edits
        what. {canChange ? "" : "Ask one of them to change who edits what."}
      </p>
      <ul className="plain">
        {team.admins.map((a) => (
          <li key={a}>
            {who(a)} <span className="muted small">{a}</span>
            {canChange && a !== me && (
              <button
                className="btn btn-small btn-ghost"
                disabled={busy}
                onClick={() => run(() => api.setAdmins(team.admins.filter((x) => x !== a)), `${a} is no longer a CADmin.`)}
              >
                Remove
              </button>
            )}
          </li>
        ))}
      </ul>
      {canChange && (
        <form
          className="inline-form"
          onSubmit={(ev) => {
            ev.preventDefault();
            const extra = parseEmails(adding);
            run(() => api.setAdmins([...team.admins, ...extra]), `Added ${extra.join(", ")} as CADmin.`).then(
              (ok) => ok && setAdding(""),
            );
          }}
        >
          <input
            value={adding}
            placeholder="email@team.org"
            onChange={(e) => setAdding(e.target.value)}
            aria-label="New CADmin email"
          />
          <button className="btn" type="submit" disabled={busy || parseEmails(adding).length === 0}>
            Add CADmin
          </button>
        </form>
      )}

      <div className="setting-label">Areas</div>
      {team.areas.length === 0 ? (
        <p className="muted small">
          None yet, so anyone can edit any file. In the Vault, use Editors on a subsystem's folder or assembly to give it
          owners. Tip: set Editors on the CAD folder's top-level assembly (or the whole CAD folder) to nobody, so only
          CADmins change how the subsystems fit together.
        </p>
      ) : (
        <ul className="plain">
          {team.areas.map((a) => (
            <li key={a.id}>
              <strong>{a.path || "Whole CAD folder"}</strong> — {editorsText(a.editors)}
              {canChange && (
                <button
                  className="btn btn-small btn-ghost"
                  disabled={busy}
                  onClick={() => run(() => api.setArea(a.id, null), `${a.path || "The CAD folder"} is no longer an area.`)}
                >
                  Remove
                </button>
              )}
            </li>
          ))}
        </ul>
      )}

      {canChange &&
        (!confirmOff ? (
          <button className="link small" onClick={() => setConfirmOff(true)}>
            Turn roles off…
          </button>
        ) : (
          <div className="notice notice-warn">
            Everyone will be able to check out everything, and the areas above are removed.
            <div className="row">
              <button className="btn btn-small btn-danger" disabled={busy} onClick={() => run(() => api.setAdmins([]), "Roles are off.")}>
                Turn roles off
              </button>
              <button className="btn btn-small btn-ghost" onClick={() => setConfirmOff(false)}>
                Cancel
              </button>
            </div>
          </div>
        ))}
    </div>
  );
}
