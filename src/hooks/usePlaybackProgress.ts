import { useEffect, useRef, useState } from "react";
import { usePlayer } from "../store/playerStore";

const FRAME_MS = 250;

export function usePlaybackProgress(
  progressMs: number | undefined,
  isPlaying: boolean,
  durationMs: number | undefined,
): number {
  const [display, setDisplay] = useState(progressMs ?? 0);

  // Anchor: the value we last received, and the wall clock at that moment.
  const anchorRef = useRef({ value: progressMs ?? 0, at: Date.now() });

  useEffect(() => {
    anchorRef.current = { value: progressMs ?? 0, at: Date.now() };
    setDisplay(progressMs ?? 0);
  }, [progressMs]);

  useEffect(() => {
    if (!isPlaying) {
      // Snap to the last authoritative value when paused.
      setDisplay(anchorRef.current.value);
      return;
    }

    let frame = 0;
    let lastPaint = performance.now();
    let hidden = document.visibilityState === "hidden";

    const tick = (now: number) => {
      if (hidden) return;
      if (now - lastPaint >= FRAME_MS) {
        const delta = now - lastPaint;
        lastPaint = now;
        setDisplay((prev) => {
          const next = prev + delta;
          if (durationMs && next >= durationMs) return durationMs;
          return next;
        });
      }
      frame = window.setTimeout(() => tick(performance.now()), FRAME_MS);
    };

    const onVisibility = () => {
      window.clearTimeout(frame);
      if (document.visibilityState === "hidden") {
        hidden = true;
        return;
      }
      hidden = false;
      lastPaint = performance.now();
      frame = window.setTimeout(() => tick(performance.now()), FRAME_MS);
    };

    document.addEventListener("visibilitychange", onVisibility);
    if (!hidden) frame = window.setTimeout(() => tick(performance.now()), FRAME_MS);
    return () => {
      window.clearTimeout(frame);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [isPlaying, durationMs]);

  return display;
}

/** Closes a popover on outside click or Escape. */
export function useDismissable(open: boolean, onClose: () => void) {
  const ref = useRef<HTMLDivElement | null>(null);
  const setPointerClaimed = usePlayer((s) => s.setPointerClaimed);

  // A popover is anchored to a button inside the pill. If the pointer rests on the
  // pill while one is open, the hover watcher would shrink the window to mini and
  // the popover would hang off a pill that no longer exists. Claiming the pointer
  // for the popover's lifetime keeps the window still.
  useEffect(() => {
    if (!open) return;
    void setPointerClaimed(true);
    return () => {
      void setPointerClaimed(false);
    };
  }, [open, setPointerClaimed]);

  useEffect(() => {
    if (!open) return;

    const onPointerDown = (event: MouseEvent) => {
      if (ref.current && !ref.current.contains(event.target as Node)) {
        onClose();
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        onClose();
      }
    };

    window.addEventListener("mousedown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("mousedown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open, onClose]);

  return ref;
}