import { useRef, useState } from "react";
import {
  AnimatePresence,
  motion,
  useMotionTemplate,
  useMotionValue,
  useSpring,
  useTransform,
} from "framer-motion";
import { MusicNote } from "../icons";
import { cn } from "../../lib/format";
import { SPRING_MAGNETIC, T_TRACK } from "../../lib/motion";
import { useReducedMotion } from "../../hooks/useReducedMotion";

interface AlbumArtworkProps {
  src?: string;
  alt: string;
  size: number;
  className?: string;
  /** Rounded corner radius in px; defaults to a 14px squircle look. */
  radius?: number;
  /** Enables the pointer-driven 3D tilt. Off for tiny sizes where it reads as noise. */
  tilt?: boolean;
  /** Ambient glow colour, `r g b`. Derived from the artwork by the caller. */
  glow?: string;
  /** Slowly breathe the album-colored spill while media plays. */
  playing?: boolean;
  /**
   * Flat material mode: no coloured spill, no glare, no tilt.
   *
   * Mirrors the `solidSurface` preference so the cover behaves like a printed
   * thumbnail on a black cutout rather than a lit piece of glass.
   */
  solid?: boolean;
}

/**
 * Album artwork with a crossfade when the track changes and an optional 3D tilt.
 *
 * The tilt is a few degrees at most. It exists to catch the specular highlight and
 * give the cover a sense of thickness — a large rotation would make a 64px
 * thumbnail look like a wobbling card, which is the opposite of premium.
 *
 * The previous frame is kept underneath for the length of the fade, so a track
 * change never flashes an empty box.
 */
export function AlbumArtwork({
  src,
  alt,
  size,
  className,
  radius = 14,
  tilt = false,
  glow,
  playing = false,
  solid = false,
}: AlbumArtworkProps) {
  const [loadedSrc, setLoadedSrc] = useState<string | null>(null);
  const [failedSrc, setFailedSrc] = useState<string | null>(null);
  const reduced = useReducedMotion();
  const ref = useRef<HTMLDivElement | null>(null);
  const latestSrc = useRef(src);
  latestSrc.current = src;

  const showImage = Boolean(src) && failedSrc !== src;
  const visibleLoadedSrc = src ? loadedSrc : null;
  const hasPreviousImage = Boolean(visibleLoadedSrc);
  const canTilt = tilt && !reduced && !solid && size >= 56;

  // Raw pointer offsets, normalised around the centre.
  const rawRotateX = useMotionValue(0);
  const rawRotateY = useMotionValue(0);

  const rotateX = useSpring(rawRotateX, SPRING_MAGNETIC);
  const rotateY = useSpring(rawRotateY, SPRING_MAGNETIC);

  // A highlight that slides across the cover as it tilts, so the glare and the
  // rotation read as one physical event rather than two effects.
  const glareX = useTransform(rotateY, [-5, 5], [12, 88]);
  // Framer's MotionStyle rejects custom properties, so the gradient is composed
  // from a motion template instead: the glare position is interpolated straight
  // into the background string.
  const glare = useMotionTemplate`radial-gradient(110px circle at ${useTransform(
    glareX,
    (v) => `${v}%`,
  )} 8%, rgba(255,255,255,0.2), transparent 64%)`;
  const glareOpacity = useTransform([rotateX, rotateY], ([rx, ry]: number[]) =>
    Math.min(1, Math.hypot(rx, ry) / 5) * 0.45,
  );

  const onPointerMove = (event: React.PointerEvent) => {
    if (!canTilt || event.pointerType === "touch") return;
    const node = ref.current;
    if (!node) return;

    const rect = node.getBoundingClientRect();
    const px = (event.clientX - rect.left) / rect.width - 0.5;
    const py = (event.clientY - rect.top) / rect.height - 0.5;

    // 5 degrees is the ceiling: enough to catch light, not enough to distort.
    rawRotateY.set(px * 10);
    rawRotateX.set(-py * 10);
  };

  const reset = () => {
    rawRotateX.set(0);
    rawRotateY.set(0);
  };

  return (
    <motion.div
      ref={ref}
      className={cn("relative shrink-0", className)}
      style={{
        width: size,
        height: size,
        borderRadius: radius,
        // The tilt needs perspective on the parent to read as depth.
        perspective: 600,
      }}
      onPointerMove={onPointerMove}
      onPointerLeave={reset}
      onPointerCancel={reset}
    >
      {/* Ambient glow derived from the artwork. Sits behind the cover so it reads
          as light spill rather than a border.

          Suppressed in solid mode: on a flat black island a coloured halo behind
          the cover is the single strongest "this is glass" cue, and removing it
          is most of what makes the surface read as an iPhone cutout. */}
      {glow && !solid && (
        <motion.div
          aria-hidden="true"
          className="pointer-events-none absolute -inset-2 -z-10 rounded-[inherit] blur-xl"
          animate={
            playing && !reduced
              ? { opacity: [0.34, 0.62, 0.34], scale: [0.97, 1.035, 0.97] }
              : { opacity: 0.3, scale: 1 }
          }
          transition={
            playing && !reduced
              ? { duration: 4.8, ease: "easeInOut", repeat: Infinity }
              : { duration: 0.45 }
          }
          style={{ background: `rgb(${glow} / 0.4)` }}
        />
      )}

      <motion.div
        className="relative h-full w-full overflow-hidden bg-white/[0.04] shadow-art"
        style={{
          borderRadius: radius,
          rotateX: canTilt ? rotateX : 0,
          rotateY: canTilt ? rotateY : 0,
          transformStyle: "preserve-3d",
        }}
      >
        {/* Fallback: a soft green-tinted gradient with a note glyph. */}
        <div
          className="artwork-fallback absolute inset-0 flex items-center justify-center transition-opacity duration-300"
          style={{ borderRadius: radius, opacity: hasPreviousImage ? 0 : 1 }}
          aria-hidden="true"
        >
          <MusicNote
            style={{ width: size * 0.36, height: size * 0.36 }}
            className="text-white/45"
            strokeWidth={1.6}
          />
        </div>

        <AnimatePresence initial={false}>
          {visibleLoadedSrc && (
            <motion.img
              key={visibleLoadedSrc}
              src={visibleLoadedSrc}
              alt={visibleLoadedSrc === src ? alt : ""}
              width={size}
              height={size}
              draggable={false}
              initial={{ opacity: 0, scale: 1.015 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.995 }}
              transition={{ ...T_TRACK, duration: reduced ? 0 : 0.32 }}
              className="absolute inset-0 h-full w-full object-cover"
              style={{ borderRadius: radius }}
            />
          )}
        </AnimatePresence>

        {showImage && src && src !== loadedSrc && (
          <img
            key={`preload-${src}`}
            src={src}
            alt=""
            width={size}
            height={size}
            draggable={false}
            onLoad={() => {
              if (latestSrc.current === src) setLoadedSrc(src);
            }}
            onError={() => {
              if (latestSrc.current === src) setFailedSrc(src);
            }}
            className="pointer-events-none absolute inset-0 h-full w-full opacity-0"
            style={{ borderRadius: radius }}
          />
        )}

        {/* Specular glare that tracks the tilt. */}
        {canTilt && (
          <motion.div
            aria-hidden="true"
            className="pointer-events-none absolute inset-0"
            style={{
              borderRadius: radius,
              background: glare,
              opacity: glareOpacity,
            }}
          />
        )}

        {/* Inner rim so the art sits inside the glass rather than on it. */}
        <div
          className="pointer-events-none absolute inset-0 ring-1 ring-inset ring-white/10"
          style={{ borderRadius: radius }}
        />
      </motion.div>
    </motion.div>
  );
}