import { useEffect, useState } from "react";

/** A small spinning ring. Pairs with text; it carries a label for screen readers. */
export function Spinner({ size = 14, label = "Working" }: { size?: number; label?: string }) {
  return <span className="spinner" style={{ width: size, height: size }} role="status" aria-label={label} />;
}

/** Placeholder rows shown while a list loads, instead of a bare "Loading…". */
export function Skeleton({ rows = 5, className = "" }: { rows?: number; className?: string }) {
  return (
    <div className={`skeleton-list ${className}`} aria-hidden>
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="skeleton-row" style={{ animationDelay: `${i * 80}ms` }}>
          <span className="skeleton-bar" style={{ width: `${55 + ((i * 17) % 35)}%` }} />
        </div>
      ))}
    </div>
  );
}

/**
 * Turns a flag into one that doesn't flicker: it only becomes true after `showAfterMs`
 * of being on, and stays true for at least `minShowMs`. Quick background syncs (about
 * 0.2 s every 15 s) then never flash the sync indicator, while slow ones show it.
 */
export function useDelayedFlag(on: boolean, showAfterMs = 600, minShowMs = 700): boolean {
  const [shown, setShown] = useState(false);
  const [since, setSince] = useState(0);
  useEffect(() => {
    if (on && !shown) {
      const t = setTimeout(() => {
        setShown(true);
        setSince(Date.now());
      }, showAfterMs);
      return () => clearTimeout(t);
    }
    if (!on && shown) {
      const wait = Math.max(0, minShowMs - (Date.now() - since));
      const t = setTimeout(() => setShown(false), wait);
      return () => clearTimeout(t);
    }
  }, [on, shown, since, showAfterMs, minShowMs]);
  return shown;
}
