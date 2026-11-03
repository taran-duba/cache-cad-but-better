import { AnimatePresence, motion } from "motion/react";
import { Search } from "lucide-react";
import { AppWindow } from "./AppWindow";
import { CheckBox, Crumbs, FileName, HeadRow, LockPill, MinePill, Row, Spinner, StatusPill } from "./mockParts";
import { usePhases } from "../lib/phases";

const ease = [0.16, 1, 0.3, 1] as const;

/* ---------------- hero: the vault catching up with Drive ---------------- */

const HERO_T = [1700, 99999];

export function HeroScene() {
  const p = usePhases(HERO_T, true, false);
  const synced = p >= 1;
  return (
    <AppWindow
      tab="vault"
      syncing={!synced}
      label="The Permafrost vault: the team's robot files with their sync status and who has each one checked out."
    >
      <div className="mock-toolbar">
        <Crumbs path={["Shared CAD", "2026 Robot"]} />
        <span className="mock-search">
          <Search size={12} /> Filter
        </span>
      </div>
      <div className="mock-table">
        <HeadRow />
        <Row name="Drivetrain" />
        <Row name="Arm" />
        <Row
          name="Robot.SLDASM"
          flash={synced}
          status={<StatusPill status={synced ? "synced" : "outdated"} />}
          action={<span className="mock-btn mock-btn-small">Check out</span>}
        />
        <Row
          name="Robot.SLDDRW"
          status={<StatusPill status="synced" />}
          action={<span className="mock-btn mock-btn-small">Check out</span>}
        />
        <Row
          name="Bumper Mount.SLDPRT"
          status={<StatusPill status="synced" />}
          lock={<LockPill who="Jordan Lee" ago="2h" />}
          action={<span className="mock-btn mock-btn-small">Need it?</span>}
        />
        <Row name="Electronics Plate.SLDPRT" get={false} />
      </div>
    </AppWindow>
  );
}

/* ---------------- 1. browse from the local index ---------------- */

const QUERY = "swerve";
const BROWSE_T = [1300, ...Array.from(QUERY, () => 130), 2600];
const DRIVETRAIN = [
  "Wiring",
  "Drivetrain.SLDASM",
  "Swerve Module.SLDASM",
  "Swerve Module.SLDDRW",
  "Frame Rail.SLDPRT",
  "Belly Pan.SLDPRT",
];

export function BrowseScene({ active }: { active: boolean }) {
  const p = usePhases(BROWSE_T, active);
  const typed = QUERY.slice(0, Math.min(p, QUERY.length));
  const shown = DRIVETRAIN.filter((n) => n.toLowerCase().includes(typed));
  return (
    <AppWindow tab="vault" label="Filtering the Drivetrain folder for swerve: results appear instantly from the local index.">
      <div className="mock-toolbar">
        <Crumbs path={["Shared CAD", "2026 Robot", "Drivetrain"]} />
        <span className={`mock-search ${p > 0 ? "focus" : ""}`}>
          <Search size={12} />
          {typed || (p > 0 ? "" : "Filter")}
          {p > 0 && <span className="mock-caret" />}
        </span>
      </div>
      <div className="mock-table">
        <HeadRow />
        <AnimatePresence initial={false}>
          {shown.map((n) => (
            <Row
              key={n}
              name={n}
              status={n.includes(".") ? <StatusPill status="synced" /> : undefined}
              action={n.includes(".") ? <span className="mock-btn mock-btn-small">Check out</span> : undefined}
            />
          ))}
        </AnimatePresence>
      </div>
      <p className="mock-note">
        <span className="mock-dot-live" /> Read from this PC's index. No waiting on Drive.
      </p>
    </AppWindow>
  );
}

/* ---------------- 2. check out = lock ---------------- */

const CHECKOUT_T = [1400, 420, 2000, 2800];

export function CheckoutScene({ active }: { active: boolean }) {
  const p = usePhases(CHECKOUT_T, active);
  const mine = p >= 2;
  return (
    <AppWindow
      tab="vault"
      toast={p === 2 || p === 3 ? "Checked out Arm Tube.SLDPRT" : undefined}
      label="Checking out Arm Tube: it gets locked to you, while a teammate's three-day-old lock on Spacer is flagged as possibly forgotten."
    >
      <div className="mock-toolbar">
        <Crumbs path={["Shared CAD", "2026 Robot", "Arm"]} />
      </div>
      <div className="mock-table">
        <HeadRow />
        <Row name="Arm.SLDASM" status={<StatusPill status="synced" />} action={<span className="mock-btn mock-btn-small">Check out</span>} />
        <Row
          name="Arm Tube.SLDPRT"
          flash={p === 1 || p === 2}
          status={<StatusPill status={p >= 3 ? "modified" : "synced"} />}
          lock={mine ? <MinePill /> : undefined}
          action={
            <span className={`mock-btn mock-btn-small ${p === 1 ? "pressed" : ""}`}>
              {mine ? "Undo check-out" : "Check out"}
            </span>
          }
        />
        <Row name="Pivot Plate.SLDPRT" status={<StatusPill status="synced" />} action={<span className="mock-btn mock-btn-small">Check out</span>} />
        <Row
          name="Spacer.SLDPRT"
          status={<StatusPill status="synced" />}
          lock={<LockPill who="Jordan Lee" ago="3d" stale />}
          action={<span className="mock-btn mock-btn-small">Need it?</span>}
        />
      </div>
    </AppWindow>
  );
}

/* ---------------- 3. atomic, checked check-in ---------------- */

const CHECKIN_T = [1500, 1300, 2400, 1500, 1400, 2800];
const CHANGED = ["Arm.SLDASM", "Arm Tube.SLDPRT", "Pivot Plate.SLDPRT"];

export function CheckinScene({ active }: { active: boolean }) {
  const p = usePhases(CHECKIN_T, active);
  const status = p >= 5 ? "synced" : p === 4 ? "pending" : "modified";
  return (
    <AppWindow
      tab="changes"
      syncing={p === 4}
      toast={p === 5 ? "Checked in 3 files" : undefined}
      label="Checking in an assembly with two parts: a missing reference blocks it, then all three files upload together."
    >
      <h4 className="mock-h">My changes</h4>
      <div className="mock-panel">
        <div className="mock-small mock-muted mock-label">Checked out by you</div>
        <ul className="mock-list">
          {CHANGED.map((n) => (
            <li key={n}>
              <CheckBox />
              <FileName name={n} />
              <span className="mock-push">
                <StatusPill status={status} />
              </span>
            </li>
          ))}
        </ul>
        <div className="mock-input">Longer arm tube, new pivot holes</div>

        <AnimatePresence mode="wait" initial={false}>
          {p === 1 && (
            <motion.div key="ask" className="mock-working" {...fade}>
              <Spinner /> Asking SolidWorks for references…
            </motion.div>
          )}
          {p === 2 && (
            <motion.div key="bad" className="mock-problem" {...fade}>
              <strong>Missing file.</strong> Arm.SLDASM uses <code>Bearing Block.SLDPRT</code>, which isn't on Drive
              or this PC.
              <div className="mock-muted mock-small">Open the assembly in SolidWorks and replace the component.</div>
            </motion.div>
          )}
          {p >= 3 && (
            <motion.div key="ok" className="mock-problem fixed" {...fade}>
              <strong>References check out.</strong> These 3 files go up together, or not at all.
            </motion.div>
          )}
        </AnimatePresence>

        <div className="mock-row-actions">
          <span className={`mock-btn mock-btn-primary ${p === 2 ? "is-disabled" : ""} ${p === 3 ? "pressed" : ""}`}>
            Check in 3 files
          </span>
          {p === 4 && (
            <div className="mock-progress" style={{ flex: 1 }}>
              <motion.div initial={{ scaleX: 0 }} animate={{ scaleX: 1 }} transition={{ duration: 1.3, ease }} />
            </div>
          )}
        </div>
      </div>
    </AppWindow>
  );
}

const fade = {
  initial: { opacity: 0, y: 6 },
  animate: { opacity: 1, y: 0, transition: { duration: 0.28, ease } },
  exit: { opacity: 0, y: -4, transition: { duration: 0.15 } },
};

/* ---------------- 4. names Windows will accept ---------------- */

const NAMES: [string, string, string][] = [
  ["Intake/Shooter.SLDASM", "Intake_Shooter.SLDASM", "slash"],
  ["CON.SLDPRT", "_CON.SLDPRT", "reserved name"],
  ["Bracket.", "Bracket_", "trailing dot"],
  ["arm.sldprt", "arm (2).sldprt", "clashes with Arm.SLDPRT"],
  ["🤖 Robot.SLDASM", "🤖 Robot.SLDASM", "kept as is"],
];
const NAMES_T = [900, ...NAMES.map(() => 520), 3200];

export function NamesScene({ active }: { active: boolean }) {
  const p = usePhases(NAMES_T, active);
  return (
    <AppWindow tab="vault" label="Drive file names mapped to names Windows accepts, such as Intake/Shooter becoming Intake_Shooter.">
      <div className="mock-toolbar">
        <Crumbs path={["Shared CAD", "Imported"]} />
      </div>
      <div className="mock-table">
        <div className="mock-row names head">
          <span>On Drive</span>
          <span />
          <span>On this PC</span>
        </div>
        {NAMES.map(([drive, local, why], i) => {
          const shown = p > i;
          return (
            <div key={drive} className="mock-row names">
              <span className="mock-cell">
                <FileName name={drive} kind={/\.sldasm$/i.test(drive) ? "assembly" : "part"} />
              </span>
              <motion.span className="mock-arrow" animate={{ opacity: shown ? 1 : 0.2, x: shown ? 0 : -4 }}>
                →
              </motion.span>
              <span className="mock-cell mock-local">
                <AnimatePresence>
                  {shown && (
                    <motion.span
                      className="mock-local-inner"
                      initial={{ opacity: 0, x: -8, filter: "blur(4px)" }}
                      animate={{ opacity: 1, x: 0, filter: "blur(0px)" }}
                      transition={{ duration: 0.35, ease }}
                    >
                      <span className={`mock-name ${local === drive ? "" : "changed"}`}>{local}</span>
                      <span className="pill pill-dim hide-xs">{why}</span>
                    </motion.span>
                  )}
                </AnimatePresence>
              </span>
            </div>
          );
        })}
      </div>
      <p className="mock-note">Nothing on Drive is renamed. Every PC works out the same local paths.</p>
    </AppWindow>
  );
}
