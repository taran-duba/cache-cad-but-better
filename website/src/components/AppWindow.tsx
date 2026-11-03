import { useId, type ReactNode } from "react";
import { AnimatePresence, LayoutGroup, motion } from "motion/react";
import { Spinner } from "./mockParts";

export type MockTab = "vault" | "changes" | "history" | "settings";

const TABS: [MockTab, string][] = [
  ["vault", "Vault"],
  ["changes", "My changes"],
  ["history", "History"],
  ["settings", "Settings"],
];

/** The Permafrost window, rebuilt in HTML: title bar, top bar, side nav, content. */
export function AppWindow({
  tab,
  syncing = false,
  status = "Up to date 9:41",
  toast,
  children,
  label,
}: {
  tab: MockTab;
  syncing?: boolean;
  status?: ReactNode;
  toast?: ReactNode;
  children: ReactNode;
  label: string;
}) {
  // Each window gets its own layout group so the nav highlight never flies between windows.
  const group = useId();
  return (
    <div className="mock" role="img" aria-label={label}>
      <LayoutGroup id={group}>
      <div aria-hidden>
        <div className="mock-chrome">
          <span className="mock-dot" />
          <span className="mock-dot" />
          <span className="mock-dot" />
          <span className="mock-title">Permafrost</span>
        </div>
        <AnimatePresence>
          {syncing && (
            <motion.div
              className="mock-syncbar"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
            />
          )}
        </AnimatePresence>
        <div className="mock-topbar">
          <div className="mock-brand">
            <span className="mock-logo">◆</span>
            Permafrost
          </div>
          <div className="mock-info">
            2026 Robot <span className="mock-muted">· Alex Kim</span>
          </div>
          <div className="mock-actions">
            {syncing ? (
              <span className="mock-chip">
                <Spinner /> Syncing…
              </span>
            ) : (
              <span className="mock-chip">{status}</span>
            )}
            <span className="mock-btn">Open folder</span>
            <span className="mock-btn mock-btn-primary">Sync now</span>
          </div>
        </div>
        <div className="mock-body">
          <nav className="mock-nav">
            {TABS.map(([id, name]) => (
              <span key={id} className={`mock-navitem ${tab === id ? "active" : ""}`}>
                {tab === id && (
                  <motion.span
                    layoutId="mock-nav-active"
                    className="mock-navitem-bg"
                    transition={{ type: "spring", stiffness: 420, damping: 36 }}
                  />
                )}
                {name}
              </span>
            ))}
          </nav>
          <div className="mock-content">
            {children}
            <AnimatePresence>
              {toast && (
                <motion.div
                  key={String(toast)}
                  className={`mock-toast ${tab === "changes" ? "top" : ""}`}
                  initial={{ opacity: 0, x: 16 }}
                  animate={{ opacity: 1, x: 0 }}
                  exit={{ opacity: 0, x: 10, transition: { duration: 0.15 } }}
                  transition={{ duration: 0.25, ease: [0.16, 1, 0.3, 1] }}
                >
                  {toast}
                </motion.div>
              )}
            </AnimatePresence>
          </div>
        </div>
      </div>
      </LayoutGroup>
    </div>
  );
}
