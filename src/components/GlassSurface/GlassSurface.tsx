import type { ReactNode } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { usePointerGlow } from "../../hooks/usePointerGlow";
import { cn } from "../../lib/format";
import { T_TRACK } from "../../lib/motion";

/**
 * The interactive glass surface.
 *
 * This is the component that makes the material respond. It renders three
 * decorative layers inside a single clipping wrapper, and publishes the pointer
 * position as CSS variables (see `usePointerGlow`):
 *
 *   .glass-ambient  album-derived colour, very low intensity
 *   .glass-sheen    a specular band that leans toward the pointer
 *   .glass-light    the cursor's radial reflection
 *
 * Why the layers live in their own wrapper: the shell itself must not get
 * `overflow: hidden`, because the volume and device popovers are positioned
 * outside its box and would be clipped. The wrapper is the only element allowed
 * to clip, and it sits at `z-index: -1` inside the shell's isolated stacking
 * context — above the tint, below the content.
 *
 * Pointer tracking is written straight to CSS variables rather than React state:
 * the light has to follow the cursor at display refresh rate, and re-rendering the
 * player subtree on every mouse move for a purely visual effect would be a
 * measurable waste.
 */
export function GlassSurface({
  children,
  className,
  interactive = true,
  artwork,
  compact = false,
  style,
}: {
  children: ReactNode;
  className?: string;
  /** Set false for surfaces that should stay inert (e.g. static panels). */
  interactive?: boolean;
  /** Optional album art used as a blurred, tinted material layer. */
  artwork?: string;
  /** Compact surfaces (the mini pill) get a denser tint and no clipped wash. */
  compact?: boolean;
  style?: React.CSSProperties;
}) {
  const ref = usePointerGlow<HTMLDivElement>(interactive);

  return (
    <div
      ref={ref}
      className={cn("glass-shell", className)}
      style={style}
      data-compact={compact ? "true" : undefined}
    >
      <div className={cn("glass-fx", artwork && "glass-fx-artwork")} aria-hidden="true">
        {artwork && (
          <div className="glass-art-wash">
            <AnimatePresence initial={false}>
              <motion.div
                key={artwork}
                className="glass-art-wash-image"
                style={{ backgroundImage: `url("${artwork}")` }}
                initial={{ opacity: 0, scale: 1.035 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 1.01 }}
                transition={{ ...T_TRACK, duration: 0.48 }}
              />
            </AnimatePresence>
          </div>
        )}
        <div className="glass-ambient" />
        <div className="glass-sheen" />
        {interactive && <div className="glass-light" />}
      </div>

      {children}
    </div>
  );
}