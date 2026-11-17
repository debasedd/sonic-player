import { useCallback, useEffect, useRef, useState } from "react";
import { motion } from "framer-motion";
import { PillPlayer, ExpandedPlayer } from "../Player/Player";
import { PocketPanel } from "./PocketPanel";
import { usePlayer } from "../../store/playerStore";
import type { WindowMode } from "../../types/spotify";

/**
 * The sliding stage: player and Pocket as two pages inside one window.
 *
 * ## Why two pages and not a swap
 *
 * Pocket and the player used to be swapped by `AnimatePresence`, which crossfaded
 * them with a scale from 0.985 to 1. That is what read as "the island shrinks and
 * grows again" instead of "the island slides": a crossfade has no direction and
 * cannot be dragged or reversed mid-gesture.
 *
 * Two pages side by side in a translating track can do all three. The track
 * follows the pointer continuously, and letting go past the threshold commits to
 * whichever page the track ended up nearer.
 *
 * The player is mounted exactly once, so this costs what it always cost — one
 * GSMTC poller, not two.
 *
 * ## Direction
 *
 * Scroll down, or drag left, slides toward Pocket. Scroll up, or drag right,
 * slides back. The sign of the gesture is the whole navigation model, which is
 * what makes it reversible: the gesture that opened Pocket closes it.
 *
 * ## Why the window never resizes
 *
 * Pocket occupies the pill's own rect (416x108), so opening it changes nothing
 * about the window. That removes the old fight between the hover watcher, the
 * `window.blur` handler and the panel over who owns the window's height.
 */

/** Fraction of the travel that commits the gesture on release. */
const COMMIT_RATIO = 0.3;

/** Pixels a wheel has to travel before it counts as a page turn. */
const WHEEL_THRESHOLD = 12;

/** Minimum interval between two wheel-driven page turns. */
const WHEEL_COOLDOWN_MS = 400;

/** Targets that own their own pointer gesture and must not start a page drag. */
const DRAG_EXCLUDE =
  "button, input, textarea, [role=tab], [draggable=true], .pocket-row, .scroll-slim";

export function PocketStage({ mode }: { mode: WindowMode }) {
  const panel = usePlayer((s) => s.panel);
  const openPocket = usePlayer((s) => s.openPocket);
  const closePanel = usePlayer((s) => s.closePanel);
  const dropActive = usePlayer((s) => s.dropActive);
  const setPointerClaimed = usePlayer((s) => s.setPointerClaimed);

  const stageRef = useRef<HTMLDivElement | null>(null);
  const [width, setWidth] = useState(0);

  // Live offset in px while a drag is in progress. `null` means the track is
  // parked on a page and `animate` owns its position.
  const [dragOffset, setDragOffset] = useState<number | null>(null);

  // Where the current drag started, in client px, and what the track's offset
  // was at that moment. A ref because it changes during the gesture without
  // wanting a re-render.
  const dragStart = useRef({ x: 0, offset: 0 });

  const onPocket = panel === "pocket";

  // Measure the stage: the track is twice its width and the drag offset is in
  // px, so the conversion has to know the real window width.
  useEffect(() => {
    const el = stageRef.current;
    if (!el) return;

    const measure = () => setWidth(el.clientWidth);
    measure();

    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  // Wheel turns the page. Bound to the stage rather than to `window`, so
  // Settings — which replaces the stage entirely — is unaffected.
  useEffect(() => {
    const el = stageRef.current;
    if (!el) return;

    let last = 0;

    const onWheel = (event: WheelEvent) => {
      // A live drop target owns the pointer; scrolling there means nothing.
      if (dropActive) return;

      // The file and clipboard lists scroll their own content. Turning the page
      // while the user is halfway down a long list would yank it away, so a
      // wheel over a scrollable region is left to it.
      const target = event.target as HTMLElement | null;
      if (target?.closest(".scroll-slim")) return;

      const now = Date.now();
      if (now - last < WHEEL_COOLDOWN_MS) return;

      // Prefer the horizontal axis (a trackpad swipe), fall back to the vertical
      // wheel a mouse has.
      const delta =
        Math.abs(event.deltaX) > Math.abs(event.deltaY) ? event.deltaX : event.deltaY;
      if (Math.abs(delta) < WHEEL_THRESHOLD) return;

      event.preventDefault();
      event.stopPropagation();
      last = now;

      // Positive delta = scroll down / swipe left = go to Pocket.
      // Negative delta = scroll up / swipe right = go back to the player.
      if (delta > 0) {
        if (!onPocket) void openPocket();
      } else if (onPocket) {
        void closePanel();
      }
    };

    // `passive: false` is required, otherwise `preventDefault` is ignored and
    // the event also scrolls whatever is behind the window.
    el.addEventListener("wheel", onWheel, { passive: false, capture: true });
    return () => el.removeEventListener("wheel", onWheel, { capture: true });
  }, [dropActive, onPocket, openPocket, closePanel]);

  const beginDrag = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (event.button !== 0) return;
      // Never start a page drag from a control, a text field, a tab, or a shelf
      // row. The last one matters most: shelf rows start a native OLE drag, and
      // capturing the pointer here would swallow that gesture.
      const target = event.target as HTMLElement | null;
      if (target?.closest(DRAG_EXCLUDE)) return;

      dragStart.current = { x: event.clientX, offset: onPocket ? -width : 0 };
      setDragOffset(onPocket ? -width : 0);
      void setPointerClaimed(true);
      event.currentTarget.setPointerCapture(event.pointerId);
    },
    [onPocket, width, setPointerClaimed],
  );

  const moveDrag = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      setDragOffset((current) => {
        if (current === null) return null;

        const raw = dragStart.current.offset + (event.clientX - dragStart.current.x);
        const min = -width;
        const max = 0;

        // Rubber-band past each end so over-dragging feels elastic.
        if (raw > max) return max + (raw - max) * 0.22;
        if (raw < min) return min + (raw - min) * 0.22;
        return raw;
      });
    },
    [width],
  );

  const endDrag = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (event.currentTarget.hasPointerCapture(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }
      void setPointerClaimed(false);

      // Commit to whichever page the track ended up nearer.
      //
      // The threshold is measured from the page the gesture started on, not from
      // zero. Using zero in both directions meant a drag back from Pocket had to
      // travel 70% of the width before it registered, so a normal pull to the
      // right did nothing — which is exactly what "it only shrinks and grows
      // again" felt like.
      setDragOffset((landed) => {
        if (landed === null) return null;

        if (onPocket) {
          // Returning: the track has to come back past the 30% line measured
          // from the Pocket position, i.e. to the right of -70% width.
          if (landed > -width * (1 - COMMIT_RATIO)) {
            void closePanel();
          }
        } else if (landed <= -width * COMMIT_RATIO) {
          void openPocket();
        }
        return null;
      });
    },
    [onPocket, width, openPocket, closePanel, setPointerClaimed],
  );

  const cancelDrag = useCallback(() => {
    setDragOffset(null);
    void setPointerClaimed(false);
  }, [setPointerClaimed]);

  // Parked on Pocket -> one width left. Dragging -> wherever the finger is.
  const trackX = dragOffset !== null ? dragOffset : onPocket ? -width : 0;

  return (
    <div
      ref={stageRef}
      id="sonic-pocket-stage"
      className="relative h-full w-full touch-none overflow-hidden select-none"
      onPointerDown={beginDrag}
      onPointerMove={moveDrag}
      onPointerUp={endDrag}
      onPointerCancel={cancelDrag}
    >
      <motion.div
        className="flex h-full will-change-transform"
        style={{ width: width > 0 ? width * 2 : "200%" }}
        animate={{ x: trackX }}
        transition={
          dragOffset !== null
            ? { duration: 0 }
            : { type: "spring", stiffness: 440, damping: 34, mass: 0.85 }
        }
      >
        {/* Page 1: the player, unchanged. */}
        <div className="h-full shrink-0" style={{ width: width > 0 ? width : "50%" }}>
          {mode === "expanded" ? <ExpandedPlayer /> : <PillPlayer mode={mode} />}
        </div>

        {/* Page 2: Pocket, in the same rect. */}
        <div className="h-full shrink-0" style={{ width: width > 0 ? width : "50%" }}>
          <PocketPanel />
        </div>
      </motion.div>
    </div>
  );
}