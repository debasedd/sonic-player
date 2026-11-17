import { useCallback, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { AnimatePresence, motion } from "framer-motion";
import {
  Check,
  Laptop,
  MonitorSpeaker,
  Radio,
  Smartphone,
  Speaker,
  Tablet,
  Tv,
  Car,
  Gamepad2,
} from "lucide-react";
import { cn, deviceKindLabel } from "../../lib/format";
import { usePlayer } from "../../store/playerStore";
import { useDismissable } from "../../hooks/usePlaybackProgress";
import type { Device } from "../../types/spotify";

/** Maps a Spotify device type to a line icon. */
function DeviceIcon({ type, className }: { type: string; className?: string }) {
  const t = type.toLowerCase();
  if (t === "computer") return <Laptop className={className} strokeWidth={1.9} />;
  if (t === "smartphone") return <Smartphone className={className} strokeWidth={1.9} />;
  if (t === "tablet") return <Tablet className={className} strokeWidth={1.9} />;
  if (t === "speaker") return <Speaker className={className} strokeWidth={1.9} />;
  if (t === "tv" || t === "tv_streaming_device")
    return <Tv className={className} strokeWidth={1.9} />;
  if (t === "automobile") return <Car className={className} strokeWidth={1.9} />;
  if (t === "game_console") return <Gamepad2 className={className} strokeWidth={1.9} />;
  return <MonitorSpeaker className={className} strokeWidth={1.9} />;
}

interface DevicePickerProps {
  /** Compact variant renders a bare icon button. */
  variant?: "icon" | "row";
  className?: string;
}

/**
 * Device picker.
 *
 * Spotify only allows one active device at a time, so selecting a row moves
 * playback there (Spotify's transfer endpoint) rather than merely remembering a
 * preference.
 */
export function DevicePicker({ variant = "icon", className }: DevicePickerProps) {
  const [open, setOpen] = useState(false);
  const devices = usePlayer((s) => s.devices);
  const playback = usePlayer((s) => s.playback);
  const selectDevice = usePlayer((s) => s.selectDevice);
  const connected = usePlayer((s) => s.connected);

  const close = useCallback(() => setOpen(false), []);
  const ref = useDismissable(open, close);

  // Portalled and `fixed`, for the same reason as the volume popover: the player
  // shell now clips to its rounded radius, so an in-flow popover would be cut off
  // at the pill's edge.
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
    const width = 236;
    const left = Math.max(8, Math.min(window.innerWidth - width - 8, rect.right - width));
    setAnchor({ left, bottom: window.innerHeight - rect.top + 10 });
  }, [open]);

  const activeId = playback?.device?.id ?? "";
  const canPick = connected && devices.length > 0;

  return (
    <div ref={ref} className={cn("relative", className)}>
      <button
        ref={anchorRef}
        type="button"
        aria-label="Playback device"
        aria-expanded={open}
        aria-haspopup="menu"
        disabled={!canPick}
        title={
          canPick
            ? `Playing on ${playback?.device?.name ?? "unknown device"}`
            : "No playback device available"
        }
        onClick={() => setOpen((v) => !v)}
        className={cn(
          "ctl",
          variant === "row" ? "h-8 w-8" : "h-[30px] w-[30px]",
          !canPick && "cursor-default opacity-35 hover:text-white/74",
        )}
      >
        <Radio size={variant === "row" ? 15 : 15} strokeWidth={2} />
        {canPick && (
          <span className="absolute -right-0 -top-0 h-1 w-1 rounded-full bg-spotify shadow-[0_0_5px_rgba(30,215,96,0.9)]" />
        )}
      </button>

      {createPortal(
        <AnimatePresence>
          {open && canPick && anchor && (
            <motion.div
              role="menu"
              aria-label="Playback devices"
              initial={{ opacity: 0, y: 6, scale: 0.96 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={{ opacity: 0, y: 4, scale: 0.97 }}
              transition={{ duration: 0.18, ease: [0.32, 0.72, 0, 1] }}
              className={cn(
                "glass-raised fixed z-[9999] w-[236px] overflow-hidden rounded-[16px] p-1",
              )}
              style={{
                left: anchor.left,
                bottom: anchor.bottom,
                transformOrigin: "bottom right",
              }}
            >
              <div className="px-2 pb-1 pt-1.5 text-2xs font-semibold uppercase tracking-[0.08em] text-ink-faint">
                Play on
              </div>
              <div className="scroll-slim max-h-[196px] overflow-y-auto">
                {devices.map((device) => (
                  <DeviceRow
                    key={device.id}
                    device={device}
                    active={device.id === activeId}
                    onSelect={() => {
                      void selectDevice(device.id);
                      setOpen(false);
                    }}
                  />
                ))}
              </div>
            </motion.div>
          )}
        </AnimatePresence>,
        document.body,
      )}
    </div>
  );
}

function DeviceRow({
  device,
  active,
  onSelect,
}: {
  device: Device;
  active: boolean;
  onSelect: () => void;
}) {
  const restricted = device.isRestricted;

  return (
    <button
      type="button"
      role="menuitem"
      onClick={onSelect}
      className={cn(
        "flex w-full items-center gap-2 rounded-[10px] px-2 py-1.5 text-left",
        "transition-colors duration-150 ease-glass",
        active ? "bg-white/[0.09]" : "hover:bg-white/[0.06]",
      )}
    >
      <DeviceIcon
        type={device.type}
        className={cn("shrink-0", active ? "text-spotify" : "text-ink-muted")}
      />
      <span className="min-w-0 flex-1">
        <span
          className={cn(
            "block truncate text-[11.5px] font-medium",
            active ? "text-white" : "text-white/85",
          )}
        >
          {device.name}
        </span>
        <span className="block truncate text-[9.5px] uppercase tracking-[0.06em] text-ink-faint">
          {deviceKindLabel(device.type)}
          {restricted ? " · restricted" : ""}
        </span>
      </span>
      {active && <Check size={12} strokeWidth={2.6} className="shrink-0 text-spotify" />}
    </button>
  );
}