import { motion } from "framer-motion";
import {
  Pause,
  Play,
  SkipBack,
  SkipForward,
  Loader2,
} from "lucide-react";
import { cn } from "../../lib/format";
import { usePlayer } from "../../store/playerStore";
import type { Transport } from "../../types/spotify";

interface PlaybackControlsProps {
  size?: "sm" | "md" | "lg";
  className?: string;
  /** Hide the skip buttons (mini mode only has play/pause). */
  showSkip?: boolean;
}

const SIZES = {
  sm: { button: 26, icon: 13, primary: 30, primaryIcon: 14, gap: "gap-0.5" },
  md: { button: 30, icon: 15, primary: 38, primaryIcon: 17, gap: "gap-1" },
  lg: { button: 38, icon: 19, primary: 52, primaryIcon: 22, gap: "gap-2" },
} as const;

/**
 * Transport controls.
 *
 * The play/pause glyph is a single icon that rotates and crossfades between the
 * two states inside a fixed-size box, so the layout never shifts when playback
 * toggles.
 */
export function PlaybackControls({
  size = "md",
  className,
  showSkip = true,
}: PlaybackControlsProps) {
  const playback = usePlayer((s) => s.playback);
  const optimisticPlaying = usePlayer((s) => s.optimisticPlaying);
  const localMediaAvailable = usePlayer((s) => s.localMediaAvailable);
  const localMediaPlaying = usePlayer((s) => s.localMediaPlaying);
  const authenticating = usePlayer((s) => s.authenticating);
  const connected = usePlayer((s) => s.connected);
  const transport = usePlayer((s) => s.transport);
  const localControls = usePlayer((s) => s.localMediaControls);
  const localTitle = usePlayer((s) => s.localMediaTitle);

  const dims = SIZES[size];

  // Prefer session state supplied by GSMTC; Spotify snapshot remains metadata fallback.
  const isPlaying =
    optimisticPlaying ??
    (localMediaAvailable ? localMediaPlaying : playback?.isPlaying ?? false);
  const hasTrack = Boolean(playback?.item) || Boolean(localTitle);
  const deviceRestricted = playback?.device?.isRestricted ?? false;
  // Playback works on any Windows media session without a Spotify login; skip
  // buttons need either a local session that supports them or Spotify.
  const transportDisabled = authenticating && !localMediaAvailable;
  const skipDisabled = localMediaAvailable
    ? authenticating || !localControls.canNext
    : authenticating || !connected || !hasTrack || deviceRestricted;
  const previousDisabled = localMediaAvailable
    ? authenticating || !localControls.canPrevious
    : skipDisabled;

  const run = (command: Transport) => {
    if (command === "previous" || command === "next") {
      if (skipDisabled) return;
    } else if (transportDisabled) {
      return;
    }
    void transport(command);
  };

  return (
    <div className={cn("flex items-center", dims.gap, className)}>
      {showSkip && (
        <button
          type="button"
          aria-label="Previous track"
          title="Previous track"
          disabled={previousDisabled}
          onClick={() => run("previous")}
          className={cn(
            "ctl",
            previousDisabled && "cursor-default opacity-35 hover:text-white/74",
          )}
          style={{ width: dims.button, height: dims.button }}
        >
          <SkipBack size={dims.icon} strokeWidth={2.1} fill="currentColor" />
        </button>
      )}

      <button
        type="button"
        aria-label={isPlaying ? "Pause" : "Play"}
        title={isPlaying ? "Pause" : "Play"}
        disabled={transportDisabled}
        onClick={() => run("toggle")}
        className={cn(
          "ctl ctl-primary relative",
          transportDisabled && "cursor-default opacity-60",
        )}
        style={{ width: dims.primary, height: dims.primary }}
      >
        {authenticating ? (
          <Loader2 size={dims.primaryIcon} className="animate-spin" strokeWidth={2.4} />
        ) : (
          // Both glyphs are stacked in the same grid cell so toggling never
          // changes the button's size.
          <span className="relative grid place-items-center" style={{ width: dims.primaryIcon, height: dims.primaryIcon }}>
            <motion.span
              className="col-start-1 row-start-1 grid place-items-center"
              initial={false}
              animate={{
                opacity: isPlaying ? 0 : 1,
                scale: isPlaying ? 0.6 : 1,
                rotate: isPlaying ? -35 : 0,
              }}
              transition={{ duration: 0.16, ease: [0.32, 0.72, 0, 1] }}
            >
              <Play
                size={dims.primaryIcon}
                strokeWidth={0}
                fill="currentColor"
                style={{ marginLeft: 1 }}
              />
            </motion.span>
            <motion.span
              className="col-start-1 row-start-1 grid place-items-center"
              initial={false}
              animate={{
                opacity: isPlaying ? 1 : 0,
                scale: isPlaying ? 1 : 0.6,
                rotate: isPlaying ? 0 : 35,
              }}
              transition={{ duration: 0.16, ease: [0.32, 0.72, 0, 1] }}
            >
              <Pause size={dims.primaryIcon} strokeWidth={0} fill="currentColor" />
            </motion.span>
          </span>
        )}
      </button>

      {showSkip && (
        <button
          type="button"
          aria-label="Next track"
          title="Next track"
          disabled={skipDisabled}
          onClick={() => run("next")}
          className={cn(
            "ctl",
            skipDisabled && "cursor-default opacity-35 hover:text-white/74",
          )}
          style={{ width: dims.button, height: dims.button }}
        >
          <SkipForward size={dims.icon} strokeWidth={2.1} fill="currentColor" />
        </button>
      )}
    </div>
  );
}