import type { ReactNode } from "react";
import { motion } from "motion/react";
import { Check, Lock } from "lucide-react";

// Same glyphs the app uses for file kinds (VaultView.tsx).
const KIND_ICON = { part: "▣", assembly: "◈", drawing: "▤", folder: "▸" } as const;
export type Kind = keyof typeof KIND_ICON;

export function kindOf(name: string): Kind {
  if (/\.sldasm$/i.test(name)) return "assembly";
  if (/\.sldprt$/i.test(name)) return "part";
  if (/\.slddrw$/i.test(name)) return "drawing";
  return "folder";
}

export function Spinner({ size = 11 }: { size?: number }) {
  return <span className="mock-spinner" style={{ width: size, height: size }} />;
}

export function FileName({ name, kind = kindOf(name) }: { name: string; kind?: Kind }) {
  return (
    <span className="mock-name">
      <span className={`mock-icon kind-${kind}`}>{KIND_ICON[kind]}</span>
      <span>{name}</span>
    </span>
  );
}

export function CheckBox({ on = true }: { on?: boolean }) {
  return <span className={`mock-check ${on ? "on" : ""}`}>{on && <Check size={9} strokeWidth={4} />}</span>;
}

const STATUS = {
  synced: ["Up to date", "pill-ok"],
  outdated: ["Update waiting", "pill-info"],
  not_synced: ["Not on this PC", "pill-dim"],
  modified: ["Edited", "pill-warn"],
  pending: ["Being pushed", "pill-info"],
} as const;
export type Status = keyof typeof STATUS;

export function StatusPill({ status }: { status: Status }) {
  const [label, cls] = STATUS[status];
  return (
    <motion.span
      key={status}
      className={`pill ${cls}`}
      initial={{ opacity: 0, scale: 0.85 }}
      animate={{ opacity: 1, scale: 1 }}
      transition={{ type: "spring", stiffness: 500, damping: 30 }}
    >
      {label}
    </motion.span>
  );
}

export function LockPill({ who, ago, stale }: { who: string; ago: string; stale?: boolean }) {
  return (
    <span className={`pill pill-lock ${stale ? "pill-stale" : ""}`}>
      <Lock size={10} strokeWidth={2.5} />
      {who.split(" ")[0]}
      <span className="hide-narrow">{who.slice(who.split(" ")[0].length)}</span> · {ago}
      {stale && <span className="hide-narrow"> · forgotten?</span>}
    </span>
  );
}

export function MinePill() {
  return (
    <motion.span
      className="pill pill-mine"
      initial={{ opacity: 0, scale: 0.6 }}
      animate={{ opacity: 1, scale: 1 }}
      transition={{ type: "spring", stiffness: 520, damping: 24 }}
    >
      You
    </motion.span>
  );
}

export function HeadRow() {
  return (
    <div className="mock-row head">
      <span className="hide-xs">Get</span>
      <span>Name</span>
      <span className="hide-xs">Status</span>
      <span className="hide-sm">Checked out</span>
      <span />
    </div>
  );
}

export function Row({
  name,
  get = true,
  status,
  lock,
  action,
  flash,
}: {
  name: string;
  get?: boolean;
  status?: ReactNode;
  lock?: ReactNode;
  action?: ReactNode;
  flash?: boolean;
}) {
  return (
    <motion.div
      layout="position"
      className={`mock-row ${flash ? "flash" : ""}`}
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, transition: { duration: 0.12 } }}
      transition={{ duration: 0.28, ease: [0.16, 1, 0.3, 1] }}
    >
      <span className="hide-xs">
        <CheckBox on={get} />
      </span>
      <span className="mock-cell">
        <FileName name={name} />
      </span>
      <span className="mock-cell hide-xs">{status}</span>
      <span className="mock-cell hide-sm">{lock}</span>
      <span className="mock-end">{action}</span>
    </motion.div>
  );
}

export function Crumbs({ path }: { path: string[] }) {
  return (
    <div className="mock-crumbs">
      {path.map((p, i) => (
        <span key={p} style={{ display: "contents" }}>
          {i > 0 && <span className="mock-sep">›</span>}
          <span className={`mock-crumb ${i === path.length - 1 ? "here" : ""}`}>{p}</span>
        </span>
      ))}
    </div>
  );
}
