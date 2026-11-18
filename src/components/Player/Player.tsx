import { useEffect } from "react";
import { AnimatePresence, motion } from "framer-motion";
import {
  ChevronDown,
  ChevronUp,
  Minimize2,
  Pocket,
  Settings2,
  Shuffle,
  Repeat,
  Repeat1,
  ExternalLink,
} from "lucide-react";
import { cn, joinArtists } from "../../lib/format";
import { usePlayer } from "../../store/playerStore";
import type { WindowMode } from "../../types/spotify";
import { AlbumArtwork } from "../AlbumArtwork/AlbumArtwork";
import { PlaybackControls } from "../PlaybackControls/PlaybackControls";
import { ProgressBar } from "../ProgressBar/ProgressBar";
import { VolumePopover } from "../VolumePopover/VolumePopover";
import { DevicePicker } from "../DevicePicker/DevicePicker";
import { ConnectionStatus } from "../ConnectionStatus/ConnectionStatus";
import { Equalizer } from "../Equalizer/Equalizer";
import { GlassSurface } from "../GlassSurface/GlassSurface";
import { useAlbumColors } from "../../hooks/useAlbumColors";
import { useReducedMotion } from "../../hooks/useReducedMotion";
import { EASE_TRACK, T_LAYOUT } from "../../lib/motion";

interface PlayerProps {
  mode: WindowMode;
}

/**
 * True when the player has nothing real to show.
 *
 * Kept in one place so the pill, mini and expanded layouts cannot drift apart on
 * what counts as an empty state.
 */
function useIsEmpty() {
  const connected = usePlayer((s) => s.connected);
  const localMediaTitle = usePlayer((s) => s.localMediaTitle);
  const authenticating = usePlayer((s) => s.authenticating);
  const error = usePlayer((s) => s.error);
  const ready = usePlayer((s) => s.ready);
  const devices = usePlayer((s) => s.devices);

  return (
    authenticating ||
    (!localMediaTitle &&
      (!connected || Boolean(error) || (ready && devices.length === 0)))
  );
}

/**
 * The album-derived ambient colour, written onto the shell as CSS variables.
 *
 * The artwork is the only thing in the UI with real colour, so letting it tint the
 * glass a little is what makes the player feel connected to what is playing. The
 * extraction and the muting both live in `useAlbumColors`.
 */
function useAmbientGlow(artworkUrl: string | undefined) {
  const palette = useAlbumColors(artworkUrl);

  useEffect(() => {
    const root = document.documentElement;
    if (palette) {
      root.style.setProperty("--art-base", palette.base);
      root.style.setProperty("--art-lift", palette.lift);
    } else {
      root.style.removeProperty("--art-base");
      root.style.removeProperty("--art-lift");
    }
  }, [palette]);

  return palette;
}

/** Crossfades only the changed label while keeping its line box steady. */
function AnimatedTrackText({
  text,
  className,
}: {
  text: string;
  className?: string;
}) {
  const reduced = useReducedMotion();
  return (
    <span className={cn("relative grid min-w-0 grid-cols-[minmax(0,1fr)]", className)}>
      <AnimatePresence initial={false} mode="popLayout">
        <motion.span
          key={text}
          initial={reduced ? false : { opacity: 0, y: 3 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: reduced ? 0 : -2 }}
          transition={{ duration: reduced ? 0 : 0.34, ease: EASE_TRACK }}
          className="clamp-1 min-w-0"
          style={{ gridArea: "1 / 1" }}
        >
          {text}
        </motion.span>
      </AnimatePresence>
    </span>
  );
}

export function PillPlayer({ mode }: PlayerProps) {
  return mode === "mini" ? <MiniPlayer /> : <FullPill />;
}

function FullPill() {
  const playback = usePlayer((s) => s.playback);
  const solid = usePlayer((s) => s.settings?.solidSurface ?? false);
  const localMediaArtworkDataUrl = usePlayer((s) => s.localMediaArtworkDataUrl);
  const localMediaTitle = usePlayer((s) => s.localMediaTitle);
  const localMediaArtist = usePlayer((s) => s.localMediaArtist);
  const localMediaPlaying = usePlayer((s) => s.localMediaPlaying);
  const setMode = usePlayer((s) => s.setMode);
  const openSettings = usePlayer((s) => s.openSettings);
  const openPocket = usePlayer((s) => s.openPocket);
  const isEmpty = useIsEmpty();

  const item = playback?.item;
  const playing = playback?.isPlaying ?? localMediaPlaying;
  const currentArtwork = item?.artworkUrl || localMediaArtworkDataUrl || undefined;
  // Solid mode has no coloured spill, so the palette is not computed at all:
  // extracting it means decoding the cover into a canvas and scanning pixels,
  // which is pure waste when the result is never painted.
  const palette = useAmbientGlow(solid ? undefined : currentArtwork);

  return (
    <div
      className="h-full w-full"
      style={{ borderRadius: "var(--shell-radius, 28px)" }}
    >
      <GlassSurface
        className="drag-region glass-shell-hover flex h-full w-full items-center gap-3 px-3.5"
        artwork={currentArtwork}
        style={{ borderRadius: "var(--shell-radius, 28px)" }}
      >
        <span data-tauri-drag-region className="contents" />

        {isEmpty ? (
          // The pill is only 108px tall, so the empty state has to be a single
          // row. The stacked variant lives in expanded mode.
          <div
            data-tauri-drag-region
            className="flex h-full w-full items-center justify-center"
          >
            <ConnectionStatus variant="inline" />
          </div>
        ) : (
          <>
            {/* Artwork is deliberately NOT a button. Making a 64px control that
                opens a 548px panel means a passing cursor or a slightly misplaced
                click expands the player, which feels like the window is
                misbehaving. Expansion is an explicit action instead: double-click
                anywhere on the pill, or the chevron button in the transport. */}
            <div
              className="-m-1 shrink-0 rounded-[14px] p-1"
              onDoubleClick={() => void setMode("expanded")}
              title="Double-click to expand"
            >
              <AlbumArtwork
                src={currentArtwork}
                alt={item?.album ? `${item.album} artwork` : "Album artwork"}
                size={64}
                radius={14}
                tilt
                glow={palette?.base}
                playing={playing}
                solid={solid}
              />
            </div>

            {/* Identity + progress */}
            <div
              data-tauri-drag-region
              className="flex min-w-0 flex-1 flex-col justify-center gap-1 self-stretch py-3"
            >
              <div className="flex items-center gap-1.5">
                <motion.p
                  layout="position"
                  layoutId="track-title"
                  className="clamp-1 text-[12px] font-semibold leading-tight tracking-[-0.01em] text-white"
                >
                  <AnimatedTrackText text={item?.name ?? localMediaTitle ?? "Nothing playing"} />
                </motion.p>
                {/* Decorative only: Spotify exposes no frequency data. It reflects
                    playback status, nothing more. */}
                <Equalizer
                  playing={playing}
                  className="shrink-0 text-spotify/90"
                />
              </div>

              <motion.p
                layout="position"
                layoutId="track-artist"
                className="clamp-1 text-[10px] font-medium leading-tight text-ink-muted"
              >
                <AnimatedTrackText text={item ? joinArtists(item.artists) : localMediaArtist ?? "Pick something in Spotify"} />
              </motion.p>

              <div className="mt-auto">
                <ProgressBar variant="full" />
              </div>
            </div>

            {/* Transport */}
            <div className="flex shrink-0 items-center gap-0.5" data-no-drag>
              <div className="mr-0.5 hidden sm:block">
                <VolumePopover />
              </div>
              <PlaybackControls size="md" showSkip />
              <button
                type="button"
                onClick={() => void setMode("expanded")}
                aria-label="Expand player"
                title="Expand"
                className="ctl ml-0.5 h-[30px] w-[30px]"
              >
                <ChevronUp size={15} strokeWidth={2.2} />
              </button>
              <button
                type="button"
                onClick={() => openPocket("files")}
                aria-label="Pocket"
                title="Pocket — files and clipboard"
                className="ctl h-[30px] w-[30px]"
              >
                <Pocket size={14.5} strokeWidth={2} />
              </button>
              <button
                type="button"
                onClick={openSettings}
                aria-label="Settings"
                title="Settings"
                className="ctl h-[30px] w-[30px]"
              >
                <Settings2 size={14.5} strokeWidth={2} />
              </button>
            </div>
          </>
        )}
      </GlassSurface>
    </div>
  );
}

/**
 * Mini mode: artwork, title, and a hairline progress indicator.
 *
 * Deliberately no transport beyond play/pause — the point is a glanceable strip.
 */
function MiniPlayer() {
  const playback = usePlayer((s) => s.playback);
  const solid = usePlayer((s) => s.settings?.solidSurface ?? false);
  const localMediaArtworkDataUrl = usePlayer((s) => s.localMediaArtworkDataUrl);
  const localMediaTitle = usePlayer((s) => s.localMediaTitle);
  const localMediaArtist = usePlayer((s) => s.localMediaArtist);
  const localMediaPlaying = usePlayer((s) => s.localMediaPlaying);
  const setMode = usePlayer((s) => s.setMode);
  const isEmpty = useIsEmpty();
  const item = playback?.item;
  const playing = playback?.isPlaying ?? localMediaPlaying;
  const currentArtwork = item?.artworkUrl || localMediaArtworkDataUrl || undefined;
  // Solid mode has no coloured spill, so the palette is not computed at all:
  // extracting it means decoding the cover into a canvas and scanning pixels,
  // which is pure waste when the result is never painted.
  const palette = useAmbientGlow(solid ? undefined : currentArtwork);

  return (
    <GlassSurface
      className="drag-region flex h-full w-full items-center gap-2.5 overflow-hidden px-3"
      artwork={currentArtwork}
      compact
      style={{ borderRadius: "var(--shell-radius, 30px)" }}
    >
      <span data-tauri-drag-region className="contents" />

      {isEmpty ? (
        <div className="flex min-w-0 items-center gap-1 px-1 text-2xs font-medium text-ink-muted">
          <span className="relative flex h-1.5 w-1.5 shrink-0">
            <span className="absolute inline-flex h-full w-full rounded-full bg-amber-400/70" />
          </span>
          <button
            type="button"
            data-no-drag
            onClick={() => void setMode("pill")}
            title="Expand to connect Spotify"
            className="clamp-1 transition-colors hover:text-white"
          >
            Not connected · expand
          </button>
        </div>
      ) : (
        <>
          <button
            type="button"
            data-no-drag
            onClick={() => void setMode("pill")}
            aria-label="Expand player"
            title="Expand"
            className="ctl shrink-0"
          >
            <AlbumArtwork
              src={currentArtwork}
              alt={item?.album ? `${item.album} artwork` : "Album artwork"}
              size={38}
              radius={10}
              glow={palette?.base}
              playing={playing}
              solid={solid}
            />
          </button>

          <div
            data-tauri-drag-region
            className="flex min-w-0 flex-1 flex-col justify-center"
          >
            <motion.p
              layout="position"
              layoutId="track-title"
              className="clamp-1 text-[12px] font-semibold leading-tight text-white"
            >
              <AnimatedTrackText text={item?.name ?? localMediaTitle ?? "Nothing playing"} />
            </motion.p>
            <motion.p
              layout="position"
              layoutId="track-artist"
              className="clamp-1 text-[10px] font-medium leading-tight text-ink-faint"
            >
              <AnimatedTrackText text={item ? joinArtists(item.artists) : localMediaArtist ?? "Spotify"} />
            </motion.p>
          </div>

          <div data-no-drag className="shrink-0">
            <PlaybackControls size="sm" showSkip={false} />
          </div>
        </>
      )}
    </GlassSurface>
  );
}

/**
 * Expanded mode: large artwork, album, full metadata, and the secondary controls.
 *
 * The layout keeps the pill's visual language — same glass, same rim light, same
 * type scale — just more of it.
 */
export function ExpandedPlayer() {
  const playback = usePlayer((s) => s.playback);
  const solid = usePlayer((s) => s.settings?.solidSurface ?? false);
  const localMediaArtworkDataUrl = usePlayer((s) => s.localMediaArtworkDataUrl);
  const localMediaTitle = usePlayer((s) => s.localMediaTitle);
  const localMediaArtist = usePlayer((s) => s.localMediaArtist);
  const localMediaPlaying = usePlayer((s) => s.localMediaPlaying);
  const setMode = usePlayer((s) => s.setMode);
  const openSettings = usePlayer((s) => s.openSettings);
  const openPocket = usePlayer((s) => s.openPocket);
  const toggleShuffle = usePlayer((s) => s.toggleShuffle);
  const cycleRepeat = usePlayer((s) => s.cycleRepeat);
  const openInSpotify = usePlayer((s) => s.openInSpotify);
  const isEmpty = useIsEmpty();
  const item = playback?.item;
  const playing = playback?.isPlaying ?? localMediaPlaying;
  const currentArtwork = item?.artworkUrl || localMediaArtworkDataUrl || undefined;
  // Solid mode has no coloured spill, so the palette is not computed at all:
  // extracting it means decoding the cover into a canvas and scanning pixels,
  // which is pure waste when the result is never painted.
  const palette = useAmbientGlow(solid ? undefined : currentArtwork);

  const RepeatIcon = playback?.repeatState === "track" ? Repeat1 : Repeat;
  const repeatActive = (playback?.repeatState ?? "off") !== "off";
  const shuffleActive = playback?.shuffleState ?? false;
  const canControl = Boolean(item);

  return (
    <GlassSurface
      className="drag-region flex h-full w-full flex-col"
      artwork={currentArtwork}
      style={{ borderRadius: "var(--shell-radius, 26px)" }}
    >
      <span data-tauri-drag-region className="contents" />

      {isEmpty ? (
        <div className="flex h-full w-full flex-col items-center justify-center gap-3 px-6">
          <ConnectionStatus />
          <button
            type="button"
            data-no-drag
            onClick={() => void setMode("pill")}
            className="ctl mt-1 h-7 gap-1 px-2.5 text-2xs font-medium"
          >
            <ChevronDown size={12} strokeWidth={2.4} />
            Back to player
          </button>
        </div>
      ) : (
        <>
          {/* Header */}
          <div className="flex items-start justify-between px-4 pb-1 pt-3.5">
            <div className="min-w-0">
              <p className="text-2xs font-semibold uppercase tracking-[0.1em] text-ink-faint">
                {item?.kind === "episode" ? "Podcast" : "Now playing"}
              </p>
            </div>
            <div className="flex items-center gap-0.5" data-no-drag>
              <DevicePicker />
              <VolumePopover />
              <button
                type="button"
                onClick={() => openPocket("files")}
                aria-label="Pocket"
                title="Pocket — files and clipboard"
                className="ctl h-[30px] w-[30px]"
              >
                <Pocket size={14.5} strokeWidth={2} />
              </button>
              <button
                type="button"
                onClick={openSettings}
                aria-label="Settings"
                title="Settings"
                className="ctl h-[30px] w-[30px]"
              >
                <Settings2 size={14.5} strokeWidth={2} />
              </button>
              <button
                type="button"
                onClick={() => void setMode("mini")}
                aria-label="Mini mode"
                title="Mini mode"
                className="ctl h-[30px] w-[30px]"
              >
                <Minimize2 size={14} strokeWidth={2} />
              </button>
            </div>
          </div>

          {/* Artwork — crossfades on track change, tilts under the pointer. */}
          <div className="flex justify-center px-4 pb-4 pt-1">
            <AlbumArtwork
              src={currentArtwork}
              alt={item?.album ? `${item.album} artwork` : "Album artwork"}
              size={252}
              radius={18}
              tilt
              glow={palette?.base}
              solid={solid}
            />
          </div>

          {/* Identity */}
          <div className="px-5">
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0">
                <h2 className="clamp-1 text-[15.5px] font-semibold leading-snug tracking-[-0.01em] text-white">
                  <AnimatedTrackText text={item?.name ?? localMediaTitle ?? "Nothing playing"} />
                </h2>
                <p className="clamp-1 mt-0.5 text-[11.5px] font-medium text-ink-muted">
                  <AnimatedTrackText text={item ? joinArtists(item.artists) : localMediaArtist ?? "Choose something in Spotify"} />
                </p>
              </div>
              <Equalizer
                playing={playing}
                bars={5}
                className="mt-2 shrink-0 text-spotify/90"
              />
            </div>

            {item?.album && (
              <div className="mt-2 flex items-center gap-1.5">
                <span className="text-2xs text-ink-faint">Album</span>
                <span className="clamp-1 text-2xs font-medium text-ink-muted">
                  {item.album}
                </span>
                {item.explicit && (
                  <span
                    className="grid h-[13px] w-[13px] shrink-0 place-items-center rounded-[3px] bg-white/[0.14] text-[8px] font-bold leading-none text-ink-muted"
                    title="Explicit"
                  >
                    E
                  </span>
                )}
              </div>
            )}
          </div>

          {/* Progress */}
          <div className="px-5 pt-3">
            <ProgressBar variant="full" />
          </div>

          {/* Transport */}
          <div
            className="flex items-center justify-center pb-3 pt-1.5"
            data-no-drag
          >
            <PlaybackControls size="lg" showSkip />
          </div>

          {/* Secondary controls */}
          <div
            className={cn(
              "mt-auto flex items-center justify-between border-t border-white/[0.07] px-4 py-2.5",
            )}
            data-no-drag
          >
            <div className="flex items-center gap-0.5">
              <button
                type="button"
                onClick={() => void toggleShuffle()}
                disabled={!canControl}
                aria-pressed={shuffleActive}
                aria-label="Shuffle"
                title="Shuffle"
                className={cn(
                  "ctl h-8 w-8",
                  shuffleActive && "ctl-active",
                  !canControl && "cursor-default opacity-35",
                )}
              >
                <Shuffle size={14.5} strokeWidth={2.1} />
              </button>
              <button
                type="button"
                onClick={() => void cycleRepeat()}
                disabled={!canControl}
                aria-label={`Repeat: ${playback?.repeatState ?? "off"}`}
                title={`Repeat: ${playback?.repeatState ?? "off"}`}
                className={cn(
                  "ctl h-8 w-8",
                  repeatActive && "ctl-active",
                  !canControl && "cursor-default opacity-35",
                )}
              >
                <RepeatIcon size={15} strokeWidth={2.1} />
              </button>
            </div>

            <div className="flex items-center gap-0.5">
              <button
                type="button"
                onClick={() => void openInSpotify()}
                disabled={!item}
                aria-label="Open in Spotify"
                title="Open in Spotify"
                className={cn(
                  "ctl h-8 w-8",
                  !item && "cursor-default opacity-35",
                )}
              >
                <ExternalLink size={14} strokeWidth={2.1} />
              </button>
              <button
                type="button"
                onClick={() => void setMode("pill")}
                aria-label="Collapse"
                title="Collapse"
                className="ctl h-8 w-8"
              >
                <ChevronDown size={15} strokeWidth={2.2} />
              </button>
            </div>
          </div>
        </>
      )}
    </GlassSurface>
  );
}

/** Motion preset re-exported so App can share the layout timing. */
export { T_LAYOUT };
