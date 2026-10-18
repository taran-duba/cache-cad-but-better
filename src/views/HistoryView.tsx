import { useEffect, useState } from "react";
import { ago, api, errorText, Manifest, who } from "../api";

export function HistoryView({ revision }: { revision: number }) {
  const [items, setItems] = useState<Manifest[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .history(100)
      .then(setItems)
      .catch((e) => setError(errorText(e)));
  }, [revision]);

  if (error) return <p className="notice notice-bad">{error}</p>;
  if (!items) return <p className="muted pad">Loading…</p>;
  if (items.length === 0) return <p className="muted pad">No check-ins yet.</p>;

  return (
    <section>
      <h2>History</h2>
      <p className="muted small">
        Every check-in keeps its exact file versions forever, so an assembly and its parts can always be restored
        together.
      </p>
      <ul className="history">
        {items.map((m) => (
          <li key={m.id}>
            <div className="history-head">
              <strong>{m.message}</strong>
              <span className="muted small">
                {who(m.author)} · {ago(m.created_time)} · {Object.keys(m.files).length} file(s)
              </span>
            </div>
          </li>
        ))}
      </ul>
    </section>
  );
}
