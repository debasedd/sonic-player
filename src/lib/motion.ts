import type { Transition } from "framer-motion";

/**
 * Shared motion tokens.
 *
 * Every duration in Sonic comes from here so the whole surface feels like one
 * material rather than a collection of unrelated animations. The easing curve is
 * the same one macOS uses for sheets and popovers: fast out of the gate, long
 * soft landing, no overshoot.
 */

/** The house easing curve. */
export const EASE_GLASS = [0.42, 0, 0.58, 1] as const;

/** Hover / small state changes: 150-200ms. */
export const T_HOVER: Transition = { duration: 0.18, ease: EASE_GLASS };

/** Icon and button feedback: 120-180ms. */
export const T_TAP: Transition = { duration: 0.14, ease: EASE_GLASS };

/** Popovers and small panels: 180-250ms. */
export const T_POPOVER: Transition = { duration: 0.22, ease: EASE_GLASS };

/** Track change: 350-500ms. */
export const T_TRACK: Transition = { duration: 0.42, ease: EASE_GLASS };

/**
 * Time-normalized underdamped spring from the reference island.
 * The Hermite correction lands position and velocity at 400ms, matching Rust.
 */
export function islandSpring(progress: number): number {
  const t = Math.max(0, Math.min(1, progress));
  const duration = 0.4;
  const omega = Math.sqrt(520);
  const decay = 24.2 / 2;
  const frequency = Math.sqrt(omega * omega - decay * decay);

  const response = (time: number): [number, number] => {
    const a = -1;
    const b = (decay * a) / frequency;
    const cosine = Math.cos(frequency * time);
    const sine = Math.sin(frequency * time);
    const envelope = Math.exp(-decay * time);
    const value = a * cosine + b * sine;
    return [
      envelope * value,
      envelope * (-decay * value + frequency * (-a * sine + b * cosine)),
    ];
  };

  const [position] = response(t * duration);
  const [tailPosition, tailVelocity] = response(duration);
  const smooth = 3 * t * t - 2 * t * t * t;
  const velocityCorrection = t * t * t - t * t;
  return Math.max(
    0,
    Math.min(
      1.08,
      1 +
        position -
        tailPosition * smooth -
        tailVelocity * duration * velocityCorrection,
    ),
  );
}

/** Island morph: 400ms damped spring, synchronized with native window resize. */
export const T_LAYOUT: Transition = { duration: 0.4, ease: islandSpring };

/**
 * Spring for anything the pointer drives directly.
 *
 * Medium stiffness with high damping: it tracks the cursor closely enough to feel
 * attached, but never jitters or overshoots. A bouncier spring reads as "toy";
 * this reads as "weighted glass".
 */
export const SPRING_POINTER = {
  type: "spring",
  stiffness: 320,
  damping: 34,
  mass: 0.7,
} as const;

/** Slightly softer spring for magnetic pull and artwork tilt. */
export const SPRING_MAGNETIC = {
  type: "spring",
  stiffness: 220,
  damping: 26,
  mass: 0.8,
} as const;

/** Slow settle for things returning to rest. */
export const SPRING_SETTLE = {
  type: "spring",
  stiffness: 170,
  damping: 28,
  mass: 0.9,
} as const;