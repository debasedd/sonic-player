import { useEffect, useRef } from "react";

/**
 * Tracks the pointer inside an element and publishes its position as CSS custom
 * properties, throttled to one write per animation frame.
 *
 * Why CSS variables instead of React state: the glass highlight has to follow the
 * cursor at display refresh rate. Pushing that through `setState` would re-render
 * the player subtree on every mouse move — dozens of times per second, for a
 * purely visual effect. Writing two custom properties on the element is free by
 * comparison, and the browser composites it without touching React at all.
 *
 * Properties written:
 *   --gx, --gy  pointer position in px, relative to the element
 *   --gi        presence, 0 when the pointer is outside, 1 when inside
 *
 * The presence value is what lets CSS fade the highlight in and out instead of
 * snapping, which is the difference between "a light turned on" and "light moved".
 */
export function usePointerGlow<T extends HTMLElement>(enabled = true) {
  const ref = useRef<T | null>(null);

  useEffect(() => {
    const node = ref.current;
    if (!node || !enabled) return;

    // Respect the OS reduced-motion setting: the highlight is decorative, so the
    // correct behaviour is to leave the glass at its resting appearance.
    if (typeof window.matchMedia === "function") {
      const query = window.matchMedia("(prefers-reduced-motion: reduce)");
      if (query.matches) return;
    }

    // Start the light at the centre so the first paint is already plausible.
    const rect0 = node.getBoundingClientRect();
    node.style.setProperty("--gx", `${rect0.width / 2}px`);
    node.style.setProperty("--gy", `${rect0.height / 2}px`);
    node.style.setProperty("--gi", "0");

    let frame = 0;
    let nextX = 0;
    let nextY = 0;
    let writtenX = Number.NaN;
    let writtenY = Number.NaN;

    const flush = () => {
      frame = 0;
      if (nextX !== writtenX) {
        node.style.setProperty("--gx", `${nextX}px`);
        writtenX = nextX;
      }
      if (nextY !== writtenY) {
        node.style.setProperty("--gy", `${nextY}px`);
        writtenY = nextY;
      }
    };

    const schedule = () => {
      if (frame === 0) frame = requestAnimationFrame(flush);
    };

    const onPointerMove = (event: PointerEvent) => {
      // Touch and pen have no hover; skip them so a tap does not leave a light on.
      if (event.pointerType === "touch") return;
      const rect = node.getBoundingClientRect();
      nextX = event.clientX - rect.left;
      nextY = event.clientY - rect.top;
      node.style.setProperty("--gi", "1");
      schedule();
    };

    const onPointerEnter = (event: PointerEvent) => {
      if (event.pointerType === "touch") return;
      node.style.setProperty("--gi", "1");
    };

    const onPointerLeave = () => {
      // Fade out via CSS; the last position is kept so the light retreats in place
      // rather than jumping back to the centre.
      node.style.setProperty("--gi", "0");
    };

    node.addEventListener("pointermove", onPointerMove, { passive: true });
    node.addEventListener("pointerenter", onPointerEnter, { passive: true });
    node.addEventListener("pointerleave", onPointerLeave, { passive: true });

    return () => {
      if (frame) cancelAnimationFrame(frame);
      node.removeEventListener("pointermove", onPointerMove);
      node.removeEventListener("pointerenter", onPointerEnter);
      node.removeEventListener("pointerleave", onPointerLeave);
    };
  }, [enabled]);

  return ref;
}