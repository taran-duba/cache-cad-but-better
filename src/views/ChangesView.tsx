import { useEffect, useMemo, useState } from "react";
import { api, CheckInPreview, errorText, MyChanges, Problem } from "../api";
import type { ToastApi } from "../Toasts";

function describe(p: Problem): { title: string; detail: string; fix: string } {
  switch (p.kind) {
    case "missing_reference":
      return {
        title: "Missing file",
        detail: `${p.from} uses ${p.reference}, which isn't on Drive or this PC.`,
        fix: "Open the assembly in SolidWorks and replace or remove the missing component.",
      };
    case "outside_vault":
      return {
        title: "File outside the vault",
        detail: `${p.from} uses ${p.reference}, which teammates won't have.`,
        fix: p.in_vault_match
          ? `Point it at ${p.in_vault_match} instead (SolidWorks: File › Find References, or Replace Components).`
          : "Copy the file into the vault, then replace the component so it points at the copy.",
      };
    case "duplicate_name":
      return {
        title: "Same file name in several folders",
        detail: `${p.from} uses ${p.name}, and ${p.copies.length} files share that name: ${p.copies.join(", ")}. SolidWorks may load the wrong one, which breaks mates.`,
        fix: "Rename one copy (from inside SolidWorks) so every file name is unique.",
      };
    case "modified_not_checked_out":
      return {
        title: "Edited without checking out",
        detail: `${p.from} uses ${p.dependency}, which has edits but isn't checked out by you.`,
        fix: "Check it out first (your edits are kept if nobody changed it), or undo the edits.",
      };
  }
}

export function ChangesView({
  revision,
  onChanged,
  toasts,
}: {
  revision: number;
  onChanged: () => void;
  toasts: ToastApi;
}) {
  const [changes, setChanges] = useState<MyChanges | null>(null);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [pickedNew, setPickedNew] = useState<Set<string>>(new Set());
  const [preview, setPreview] = useState<CheckInPreview | null>(null);
  const [message, setMessage] = useState("");
  const [keepLocked, setKeepLocked] = useState(false);
  const [allowDupes, setAllowDupes] = useState(false);
  const [busy, setBusy] = useState(false);

  const load = () =>
    api
      .myChanges()
      .then((c) => {
        setChanges(c);
        setPicked(new Set(c.checkedOut.filter((e) => e.status === "modified").map((e) => e.id)));
      })
      .catch((e) => toasts.error(errorText(e)));

  useEffect(() => {
    load();
    setPreview(null);
  }, [revision]);

  const nameOf = useMemo(() => {
    const m = new Map((changes?.checkedOut ?? []).map((e) => [e.id, e.name]));
    return (id: string) => m.get(id) ?? id;
  }, [changes]);

  const toggle = (set: Set<string>, setter: (s: Set<string>) => void, key: string) => {
    const next = new Set(set);
    next.has(key) ? next.delete(key) : next.add(key);
    setter(next);
    setPreview(null);
  };

  const review = async () => {
    setBusy(true);
    try {
      setPreview(await api.previewCheckIn([...picked], [...pickedNew]));
    } catch (e) {
      toasts.error(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const blocking = preview?.plan.problems.filter((p) => !(allowDupes && p.kind === "duplicate_name")) ?? [];
  const hasDupes = preview?.plan.problems.some((p) => p.kind === "duplicate_name") ?? false;

  const checkIn = async () => {
    if (!preview) return;
    setBusy(true);
    try {
      const out = await api.checkIn({
        files: [...picked],
        newFiles: [...pickedNew],
        message,
        keepLocked,
        allowDuplicateNames: allowDupes,
      });
      toasts.info(`Checked in ${out.files} file(s). Teammates get them all together.`);
      setMessage("");
      setPreview(null);
      onChanged();
      load();
    } catch (e) {
      toasts.error(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  if (!changes) return <p className="muted pad">Loading…</p>;
  const nothing = changes.checkedOut.length === 0 && changes.newFiles.length === 0;

  return (
    <section className="changes">
      <h2>My changes</h2>
      {nothing && (
        <p className="muted">
          Nothing checked out. Check out a file in the Vault tab before editing it — files you haven't checked out
          open read-only in SolidWorks so nobody overwrites anyone.
        </p>
      )}

      {changes.checkedOut.length > 0 && (
        <>
          <h3>Checked out by you</h3>
          <ul className="picklist">
            {changes.checkedOut.map((e) => (
              <li key={e.id}>
                <label>
                  <input type="checkbox" checked={picked.has(e.id)} onChange={() => toggle(picked, setPicked, e.id)} />
                  {e.name}
                  <span className={`pill ${e.status === "modified" ? "pill-warn" : "pill-dim"}`}>
                    {e.status === "modified" ? "Edited" : "No edits yet"}
                  </span>
                </label>
              </li>
            ))}
          </ul>
        </>
      )}

      {changes.newFiles.length > 0 && (
        <>
          <h3>New files on this PC</h3>
          <ul className="picklist">
            {changes.newFiles.map((p) => (
              <li key={p}>
                <label>
                  <input type="checkbox" checked={pickedNew.has(p)} onChange={() => toggle(pickedNew, setPickedNew, p)} />
                  {p}
                </label>
              </li>
            ))}
          </ul>
        </>
      )}

      {!nothing && (
        <div className="row">
          <button className="btn" onClick={review} disabled={busy || (picked.size === 0 && pickedNew.size === 0)}>
            Review check-in
          </button>
        </div>
      )}

      {preview && (
        <div className="review">
          {!preview.refsChecked && (
            <p className="notice notice-warn">
              References weren't checked because the Document Manager key or SwRefs helper isn't set up. Parts that
              assemblies depend on won't be added automatically.
            </p>
          )}
          <h3>Will check in</h3>
          <ul className="plain">
            {preview.plan.files.map((id) => (
              <li key={id}>{nameOf(id)}</li>
            ))}
            {preview.plan.new_files.map((p) => (
              <li key={p}>
                {p} <span className="pill pill-info">new</span>
              </li>
            ))}
          </ul>
          {preview.plan.auto_included.length > 0 && (
            <p className="muted small">
              Added automatically because your assemblies use them: {preview.plan.auto_included.join(", ")}
            </p>
          )}

          {preview.plan.problems.length > 0 && (
            <>
              <h3>Fix before checking in</h3>
              <ul className="problems">
                {preview.plan.problems.map((p, i) => {
                  const d = describe(p);
                  return (
                    <li key={i} className={allowDupes && p.kind === "duplicate_name" ? "waived" : ""}>
                      <strong>{d.title}</strong>
                      <div>{d.detail}</div>
                      <div className="muted small">{d.fix}</div>
                    </li>
                  );
                })}
              </ul>
              {hasDupes && (
                <label className="check">
                  <input type="checkbox" checked={allowDupes} onChange={(e) => setAllowDupes(e.target.checked)} />
                  Check in anyway with duplicate names (I've checked the right files are referenced)
                </label>
              )}
            </>
          )}

          <label className="field">
            <span>What changed?</span>
            <textarea
              value={message}
              maxLength={500}
              onChange={(e) => setMessage(e.target.value)}
              placeholder="e.g. Arm: shorter pivot plate, new 3:1 gear ratio"
            />
          </label>
          <label className="check">
            <input type="checkbox" checked={keepLocked} onChange={(e) => setKeepLocked(e.target.checked)} />
            Keep these files checked out
          </label>
          <div className="row">
            <button
              className="btn btn-primary"
              onClick={checkIn}
              disabled={busy || blocking.length > 0 || message.trim() === ""}
            >
              {busy ? "Checking in…" : "Check in"}
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
