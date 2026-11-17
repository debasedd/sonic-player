import { useCallback, useRef, useState } from "react";
import { cn, formatTime } from "../../lib/format";
import { usePlayer } from "../../store/playerStore";
import { usePlaybackProgress } from "../../hooks/usePlaybackProgress";

interface ProgressBarProps {
  /** Compact variant hides the timestamps and shrinks the hit area. */
  variant?: "full" | "compact";
  className?: string;
}

/**
 * Thin scrubber.
 *
 * Progress is interpolated locally (see `usePlaybackProgress`) so the bar moves
 * continuously between the backend's 1 Hz snapshots. Pointer-down captures the
 * pointer, so a drag that leaves the element keeps tracking until release.
 */
export function ProgressBar({ variant = "full", className }: ProgressBarProps) {
  const playback = usePlayer((s) => s.playback);
  const scrubbing = usePlayer((s) => s.scrubbing);
  const setScrubbing = usePlayer((s) => s.setScrubbing);
  const seek = usePlayer((s) => s.seek);
  const localAvailable = usePlayer((s) => s.localMediaAvailable);
  const localPosition = usePlayer((s) => s.localMediaPositionMs);
  const localDuration = usePlayer((s) => s.localMediaDurationMs);
  const localPlaying = usePlayer((s) => s.localMediaPlaying);
  const localControls = usePlayer((s) => s.localMediaControls);

  const trackRef = useRef<HTMLDivElement | null>(null);
  const [dragValue, setDragValue] = useState<number | null>(null);
  /** Pointer x while hovering, for the time tooltip. */
  const [hoverRatio, setHoverRatio] = useState<number | null>(null);

  // Local session wins for the bar; Spotify snapshot is the fallback.
  const durationMs = localAvailable
    ? localDuration ?? playback?.item?.durationMs ?? 0
    : playback?.item?.durationMs ?? 0;
  const isPlaying = localAvailable ? localPlaying : playback?.isPlaying ?? false;

  const interpolated = usePlaybackProgress(
    localAvailable ? localPosition ?? undefined : playback?.progressMs,
    isPlaying && !scrubbing,
    durationMs,
  );

  const position = dragValue ?? interpolated;
  const percent = durationMs > 0 ? Math.min(100, (position / durationMs) * 100) : 0;

  /** Maps a clientX to a position in ms, clamped to the track. */
  const positionFromEvent = useCallback(
    (clientX: number) => {
      const node = trackRef.current;
      if (!node || durationMs <= 0) return 0;
      const rect = node.getBoundingClientRect();
      const ratio = rect.width > 0 ? (clientX - rect.left) / rect.width : 0;
      return Math.round(Math.min(1, Math.max(0, ratio)) * durationMs);
    },
    [durationMs],
  );

  const handlePointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if (durationMs <= 0) return;
    event.preventDefault();
    event.stopPropagation();

    // Stop the window drag from hijacking the scrub.
    event.currentTarget.setPointerCapture?.(event.pointerId);
    setScrubbing(true);
    setDragValue(positionFromEvent(event.clientX));
  };

  const handlePointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const node = trackRef.current;
    if (node && node.getBoundingClientRect) {
      const rect = node.getBoundingClientRect();
      setHoverRatio(rect.width > 0 ? (event.clientX - rect.left) / rect.width : null);
    }
    if (!scrubbing) return;
    setDragValue(positionFromEvent(event.clientX));
  };

  const clearHover = () => setHoverRatio(null);

  const commit = (event: React.PointerEvent<HTMLDivElement>) => {
    if (!scrubbing) return;
    const target = dragValue ?? positionFromEvent(event.clientX);
    setScrubbing(false);
    setDragValue(null);
    void seek(target);
  };

  const cancel = () => {
    if (!scrubbing) return;
    setScrubbing(false);
    setDragValue(null);
  };

  const disabled = durationMs <= 0 || (localAvailable && !localControls.canSeek);
  const isCompact = variant === "compact";

  return (
    <div className={cn("w-full", className)}>
      <div
        ref={trackRef}
        role="slider"
        tabIndex={disabled ? -1 : 0}
        aria-label="Playback position"
        aria-valuemin={0}
        aria-valuemax={Math.round(durationMs / 1000)}
        aria-valuenow={Math.round(position / 1000)}
        aria-valuetext={`${formatTime(position)} of ${formatTime(durationMs)}`}
        aria-disabled={disabled}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={commit}
        onPointerCancel={cancel}
        onPointerLeave={clearHover}
        onKeyDown={(event) => {
          if (disabled) return;
          const step = event.shiftKey ? 30_000 : 5_000;
          if (event.key === "ArrowRight") {
            event.preventDefault();
            event.stopPropagation();
            void seek(Math.min(durationMs, position + step));
          } else if (event.key === "ArrowLeft") {
            event.preventDefault();
            event.stopPropagation();
            void seek(Math.max(0, position - step));
          }
        }}
        className={cn(
          "group relative flex w-full touch-none items-center",
          isCompact ? "h-3" : "h-4",
          disabled ? "cursor-default" : "cursor-pointer",
        )}
        style={{ WebkitTapHighlightColor: "transparent" }}
      >
        {/* Rail */}
        <div
          className={cn(
            "relative w-full overflow-hidden rounded-full bg-white/[0.13]",
            "transition-[height] duration-200 ease-glass",
            isCompact ? "h-px" : "h-[3px]",
            !disabled && "group-hover:h-[5px]",
          )}
        >
          {/* Elapsed */}
          <div
            className={cn(
              "absolute inset-y-0 left-0 rounded-full",
              "bg-white/85 transition-[width] duration-300 ease-linear",
              !disabled && "group-hover:bg-white",
            )}
            style={{ width: `${percent}%` }}
          />
          {/* Buffer-ish highlight: a soft leading edge on the fill. */}
          <div
            className="absolute inset-y-0 w-6 -translate-x-full rounded-full bg-white/25 blur-[3px] transition-[width] duration-300 ease-linear"
            style={{ left: `${percent}%`, opacity: percent > 0 ? 1 : 0 }}
          />
        </div>

        {/* Hover preview: a faint segment from the playhead to the cursor, so a
            click-to-seek reads as "this is where it will go". */}
        {!isCompact && hoverRatio !== null && !disabled && !scrubbing && (
          <div
            aria-hidden="true"
            className="pointer-events-none absolute inset-y-0 left-0 rounded-full bg-white/[0.16]"
            style={{ width: `${Math.min(100, Math.max(0, hoverRatio * 100))}%` }}
          />
        )}

        {/* Thumb: appears on hover / while scrubbing. */}
        {!isCompact && (
          <div
            className={cn(
              "pointer-events-none absolute top-1/2 z-10 h-[9px] w-[9px] -translate-x-1/2 -translate-y-1/2",
              "rounded-full bg-white shadow-[0_1px_4px_rgba(0,0,0,0.6)]",
              "transition-opacity duration-150",
              scrubbing ? "opacity-100" : "opacity-0 group-hover:opacity-100",
            )}
            style={{ left: `${percent}%` }}
          />
        )}
      </div>

      {/* Time tooltip while seeking. */}
      {!isCompact && scrubbing && durationMs > 0 && (
        <div
          className="pointer-events-none relative -mt-0.5 h-0"
          aria-hidden="true"
        >
          <div
            className="absolute -translate-x-1/2 -translate-y-full whitespace-nowrap rounded-md bg-black/70 px-1.5 py-[2px] text-[9.5px] font-medium text-white/90 backdrop-blur-sm"
            style={{ left: `${percent}%`, marginBottom: 2 }}
          >
            <span className="text-mono-num">{formatTime(position)}</span>
          </div>
        </div>
      )}

      {!isCompact && (
        <div className="mt-0.5 flex items-center justify-between text-2xs font-medium text-ink-faint">
          <span className="text-mono-num">{formatTime(position)}</span>
          <span className="text-mono-num">-{formatTime(Math.max(0, durationMs - position))}</span>
        </div>
      )}
    </div>
  );
}