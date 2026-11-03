import { useCallback, useEffect, useState, useSyncExternalStore } from "react";
import { useReducedMotion } from "motion/react";

/**
 * Steps through a looping little timeline while `active`. Each entry is how long that
 * phase lasts (ms). With reduced motion the final, most informative phase is shown still.
 */
export function usePhases(durations: number[], active: boolean, loop = true): number {
  const reduce = useReducedMotion();
  const last = durations.length - 1;
  const [phase, setPhase] = useState(0);

  useEffect(() => {
    if (!active) {
      setPhase(0);
      return;
    }
    if (reduce || (!loop && phase === last)) return;
    const t = setTimeout(() => setPhase((p) => (p >= last ? 0 : p + 1)), durations[phase]);
    return () => clearTimeout(t);
  }, [active, reduce, loop, phase, last, durations]);

  return reduce ? last : phase;
}

/** Media query match. Reports `false` while prerendering and hydrating, then the real value. */
export function useMediaQuery(query: string): boolean {
  const subscribe = useCallback(
    (onChange: () => void) => {
      const m = window.matchMedia(query);
      m.addEventListener("change", onChange);
      return () => m.removeEventListener("change", onChange);
    },
    [query],
  );
  return useSyncExternalStore(
    subscribe,
    () => window.matchMedia(query).matches,
    () => false,
  );
}
