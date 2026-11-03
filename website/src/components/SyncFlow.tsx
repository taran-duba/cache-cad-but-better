import { useRef } from "react";
import { motion, useScroll, useSpring, useTransform, type MotionValue } from "motion/react";
import { Cloud, Database, HardDrive } from "lucide-react";

const NODES = [
  { icon: Cloud, title: "Google Drive", body: "Your team's shared drive stays the single source of truth." },
  { icon: Database, title: "Permafrost index", body: "A SQLite index and sync journal on each PC. Survives crashes." },
  { icon: HardDrive, title: "Your CAD folder", body: "Real files SolidWorks opens. Read-only until you check out." },
];
const LINKS = ["Change feed", "Atomic pull & push"];

/** Drive → index → disk, with the connecting lines drawn in as you scroll. */
export function SyncFlow() {
  const ref = useRef<HTMLDivElement>(null);
  const { scrollYProgress } = useScroll({ target: ref, offset: ["start 0.85", "center 0.5"] });
  const p = useSpring(scrollYProgress, { stiffness: 120, damping: 28 });

  return (
    <section className="flow" aria-labelledby="flow-title">
      <div className="container">
        <header className="section-head center">
          <p className="eyebrow">Under the ice</p>
          <h2 id="flow-title" className="section-title">
            Drive in the cloud. Speed on your desk.
          </h2>
          <p className="section-lede">
            Permafrost sits between your shared drive and SolidWorks, so the slow part (the network) never sits between
            you and your files.
          </p>
        </header>

        <div className="flow-row" ref={ref}>
          {NODES.map((n, i) => (
            <FlowPart key={n.title} node={n} index={i} progress={p} />
          ))}
        </div>
      </div>
    </section>
  );
}

function FlowPart({
  node,
  index,
  progress,
}: {
  node: (typeof NODES)[number];
  index: number;
  progress: MotionValue<number>;
}) {
  const start = index * 0.36;
  const lit = useTransform(progress, [start, start + 0.18], [0.35, 1]);
  const lift = useTransform(progress, [start, start + 0.18], [16, 0]);
  const draw = useTransform(progress, [start + 0.14, start + 0.38], [0, 1]);
  const Icon = node.icon;
  return (
    <>
      <motion.div className="flow-node" style={{ opacity: lit, y: lift }}>
        <span className="flow-icon">
          <Icon size={22} strokeWidth={1.75} />
        </span>
        <h3>{node.title}</h3>
        <p>{node.body}</p>
      </motion.div>
      {index < LINKS.length && (
        <div className="flow-link" aria-hidden>
          <div className="flow-track">
            <motion.div className="flow-fill" style={{ "--p": draw } as never} />
            <span className="flow-packet" />
            <span className="flow-packet late" />
          </div>
          <span className="flow-label mono">{LINKS[index]}</span>
        </div>
      )}
    </>
  );
}
