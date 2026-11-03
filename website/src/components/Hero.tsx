import { useRef } from "react";
import { motion, useScroll, useTransform, useReducedMotion } from "motion/react";
import { ArrowDown } from "lucide-react";
import { HeroScene } from "./scenes";
import { DownloadButtons } from "./DownloadButtons";

const ease = [0.16, 1, 0.3, 1] as const;
const rise = (delay: number) => ({
  initial: { opacity: 0, y: 14 },
  animate: { opacity: 1, y: 0 },
  transition: { duration: 0.8, ease, delay },
});

export function Hero() {
  const stage = useRef<HTMLDivElement>(null);
  const reduce = useReducedMotion();
  // The window starts tilted back like a slab of ice and settles flat as you scroll to it.
  const { scrollYProgress } = useScroll({ target: stage, offset: ["start end", "start 0.25"] });
  const rotateX = useTransform(scrollYProgress, [0, 1], reduce ? [0, 0] : [24, 0]);
  const scale = useTransform(scrollYProgress, [0, 1], reduce ? [1, 1] : [0.9, 1]);
  const y = useTransform(scrollYProgress, [0, 1], reduce ? [0, 0] : [40, 0]);

  const glow = useTransform(scrollYProgress, [0, 1], [0.35, 1]);

  return (
    <section className="hero" aria-labelledby="hero-title">
      <div className="hero-frost" aria-hidden />
      <div className="container hero-copy">
        <motion.p className="eyebrow" {...rise(0)}>
          SolidWorks + Google Drive, for FRC teams
        </motion.p>
        <motion.h1 id="hero-title" className="hero-title" {...rise(0.08)}>
          Shared CAD that <span className="accent-text">stays solid</span>.
        </motion.h1>
        <motion.p className="hero-lede" {...rise(0.16)}>
          Permafrost keeps your team's SolidWorks files in sync through Google Drive. Check a file out, edit it, check it
          in. No overwrites, no broken mates, no staring at a spinner while the network catches up.
        </motion.p>
        <motion.div {...rise(0.24)}>
          <DownloadButtons />
        </motion.div>
        <motion.a href="#how" className="hero-scroll" {...rise(0.4)}>
          See it work <ArrowDown size={15} />
        </motion.a>
      </div>

      <div className="container hero-stage" ref={stage}>
        <motion.div className="hero-glow" style={{ opacity: glow }} aria-hidden />
        <motion.div
          className="hero-window"
          style={{ rotateX, scale, y, transformPerspective: 1400 }}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ duration: 1, ease, delay: 0.3 }}
        >
          <HeroScene />
        </motion.div>
      </div>
    </section>
  );
}
