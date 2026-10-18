import { useMemo, useState } from "react";

type Kind = "info" | "warn" | "error";
interface Toast {
  id: number;
  kind: Kind;
  text: string;
}

export interface ToastApi {
  list: Toast[];
  info: (t: string) => void;
  warn: (t: string) => void;
  error: (t: string) => void;
  dismiss: (id: number) => void;
}

let nextId = 1;

export function useToasts(): ToastApi {
  const [list, setList] = useState<Toast[]>([]);
  const api = useMemo(() => {
    const push = (kind: Kind, text: string) => {
      const id = nextId++;
      setList((l) => [...l.slice(-4), { id, kind, text }]);
      setTimeout(() => setList((l) => l.filter((t) => t.id !== id)), kind === "error" ? 9000 : 5000);
    };
    return {
      info: (t: string) => push("info", t),
      warn: (t: string) => push("warn", t),
      error: (t: string) => push("error", t),
      dismiss: (id: number) => setList((l) => l.filter((t) => t.id !== id)),
    };
  }, []);
  return useMemo(() => ({ ...api, list }), [api, list]);
}

export function Toasts({ state }: { state: ToastApi }) {
  return (
    <div className="toasts" role="status" aria-live="polite">
      {state.list.map((t) => (
        <div key={t.id} className={`toast toast-${t.kind}`} onClick={() => state.dismiss(t.id)}>
          {t.text}
        </div>
      ))}
    </div>
  );
}
