import { motion } from "framer-motion";
import { FilePlus2 } from "lucide-react";
import { GlassSurface } from "../GlassSurface/GlassSurface";
import { T_LAYOUT } from "../../lib/motion";

/**
 * The surface shown while a native file drag hovers the player.
 *
 * ## Why it replaces the player
 *
 * The window is 176 px tall while this is up — shorter than the pill's own 108 px
 * plus its breathing room, and much shorter than expanded. Drawing the player
 * underneath would mean two glass shells in one rect, which reads as a rendering
 * fault rather than a state change.
 *
 * ## Why the copy is imperative
 *
 * The user is mid-gesture with a file under the cursor. "Drop files here" and
 * "Release to add to Sonic" answer the only two questions they have: can I drop
 * this, and what happens if I do. A friendlier sentence would be a longer read at
 * exactly the moment nobody is reading.
 */
export function DropTarget() {
  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.16 }}
      className="h-full w-full p-1.5"
    >
      <GlassSurface
        interactive={false}
        artwork={undefined}
        className="flex h-full w-full flex-col items-center justify-center gap-2 rounded-[22px]"
        style={{
          borderRadius: 22,
          // A dashed inner ring is the one affordance that reads as "drop here"
          // without a filled panel: the material stays intact and the target
          // still has an edge.
          boxShadow:
            "inset 0 0 0 1px rgba(255,255,255,0.22), inset 0 0 0 4px rgba(255,255,255,0.05)",
        }}
      >
        <motion.span
          initial={{ scale: 0.9, opacity: 0 }}
          animate={{ scale: 1, opacity: 1 }}
          transition={{ ...T_LAYOUT, delay: 0.04 }}
          className="grid h-9 w-9 place-items-center rounded-full bg-white/[0.09] text-white/80"
        >
          <FilePlus2 size={17} strokeWidth={1.9} />
        </motion.span>

        <div className="text-center">
          <p className="text-[13px] font-semibold leading-tight text-white">Drop files here</p>
          <p className="mt-0.5 text-[11px] font-medium leading-tight text-ink-muted">
            Release to add to Sonic
          </p>
        </div>
      </GlassSurface>
    </motion.div>
  );
}