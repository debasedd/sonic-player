import { useCallback, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { AnimatePresence, motion } from "framer-motion";
import { Volume1, Volume2, VolumeX, MonitorSpeaker } from "lucide-react";
import { cn, deviceKindLabel } from "../../lib/format";
import { usePlayer } from "../../store/playerStore";
import { useDismissable } from "../../hooks/usePlaybackProgress";

interface VolumePopoverProps {
  /** Which edge the popover is anchored to. */
  align?: "left" | "right";
  className?: string;
}

/**
 * Volume popover.
 *
 * Volume is only settable on devices that report `supportsVolume`; Spotify
 * returns 403 otherwise. Rather than failing on click, we disable the slider and
 * explain why.
 */
export function VolumePopover({ align = "right", className }: VolumePopoverProps) {
  const [open, setOpen] = useState(false);
  const playback = usePlayer((s) => s.playback);
  const commitVolume = usePlayer((s) => s.commitVolume);
  const pendingVolume = usePlayer((s) => s.pendingVolume);

  const close = useCallback(() => setOpen(false), []);
  const ref = useDismissable(open, close);

  // The popover is portalled to <body> and positioned with `fixed` coordinates.
  //
  // It used to be `absolute` inside the player shell, which worked only because the
  // shell was `overflow: visible`. The shell now clips its content to the rounded
  // radius (otherwise the expanded panel's bottom row paints a square band into the
  // bottom corners), so an in-flow popover would be cut off. Portalling keeps the
  // glass unclipped while the shell stays properly rounded.
  const anchorRef = useRef<HTMLButtonElement | null>(null);
  const [anchor, setAnchor] = useState<{ left: number; bottom: number } | null>(null);

  useLayoutEffect(() => {
    if (!open) {
      setAnchor(null);
      return;
    }
    const node = anchorRef.current;
    if (!node) return;
    const rect = node.getBoundingClientRect();
    const width = 212;
    const preferredLeft = rect.right - width;
    // Keep it on screen: the window is only 416 wide.
    const left = Math.max(8, Math.min(window.innerWidth - width - 8, preferredLeft));
    setAnchor({ left, bottom: window.innerHeight - rect.top + 10 });
  }, [open]);

  const device = playback?.device;
  const supportsVolume = device?.supportsVolume ?? false;
  const volume = pendingVolume ?? device?.volumePercent ?? 0;
  const muted = volume === 0;

  const VolumeIcon = muted ? VolumeX : volume < 45 ? Volume1 : Volume2;

  return (
    <div ref={ref} className={cn("relative", className)}>
      <button
        ref={anchorRef}
        type="button"
        aria-label="Volume"
        aria-expanded={open}
        aria-haspopup="dialog"
        title={device ? `Volume · ${device.name}` : "Volume"}
        onClick={() => setOpen((v) => !v)}
        className={cn("ctl h-[30px] w-[30px]", open && "text-white")}
      >
        <VolumeIcon size={15} strokeWidth={2} />
      </button>

      {createPortal(
        <AnimatePresence>
          {open && anchor && (
            <motion.div
              role="dialog"
              aria-label="Volume"
              initial={{ opacity: 0, y: 6, scale: 0.96 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={{ opacity: 0, y: 4, scale: 0.97 }}
              transition={{ duration: 0.18, ease: [0.32, 0.72, 0, 1] }}
              className={cn(
                "glass-raised fixed z-[9999] w-[212px]",
                "rounded-[16px] p-3",
              )}
              style={{
                left: anchor.left,
                bottom: anchor.bottom,
                transformOrigin: align === "right" ? "bottom right" : "bottom left",
              }}
            >
            <div className="mb-2 flex items-center justify-between">
              <span className="text-2xs font-semibold uppercase tracking-[0.08em] text-ink-faint">
                Volume
              </span>
              <span className="text-2xs font-medium text-ink-muted text-mono-num">
                {Math.round(volume)}%
              </span>
            </div>

            {device ? (
              <>
                <input
                  type="range"
                  min={0}
                  max={100}
                  step={1}
                  value={volume}
                  disabled={!supportsVolume}
                  aria-label="Volume level"
                  aria-valuetext={`${Math.round(volume)} percent`}
                  onChange={(e) => {
                    // Local echo only; the commit happens on release so we do not
                    // flood the API while dragging.
                    const next = Number(e.target.value);
                    usePlayer.setState((s) => ({
                      playback: s.playback
                        ? {
                            ...s.playback,
                            device: { ...s.playback.device, volumePercent: next },
                          }
                        : s.playback,
                    }));
                  }}
                  onPointerUp={(e) => {
                    if (!supportsVolume) return;
                    void commitVolume(Number((e.target as HTMLInputElement).value));
                  }}
                  onKeyUp={(e) => {
                    if (!supportsVolume) return;
                    void commitVolume(Number((e.currentTarget as HTMLInputElement).value));
                  }}
                  className={cn("range", !supportsVolume && "cursor-default opacity-40")}
                />

                {!supportsVolume && (
                  <p className="mt-2 text-2xs leading-snug text-ink-faint">
                    This device does not accept volume changes. Use the device&apos;s own
                    controls.
                  </p>
                )}

                <div className="mt-2.5 flex items-center gap-1.5 border-t border-white/[0.07] pt-2 text-2xs text-ink-faint">
                  <MonitorSpeaker size={11} strokeWidth={2} className="shrink-0" />
                  <span className="clamp-1">{device.name}</span>
                  <span className="ml-auto shrink-0 rounded-full bg-white/[0.07] px-1.5 py-[1px] text-[9px] uppercase tracking-wide text-ink-faint">
                    {deviceKindLabel(device.type)}
                  </span>
                </div>
              </>
            ) : (
              <p className="text-2xs leading-snug text-ink-faint">
                Start playback in Spotify and pick a device to control its volume.
              </p>
            )}
            </motion.div>
          )}
        </AnimatePresence>,
        document.body,
      )}
    </div>
  );
}