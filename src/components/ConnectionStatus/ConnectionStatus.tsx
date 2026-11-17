import { motion } from "framer-motion";
import {
  AlertCircle,
  Loader2,
  MonitorOff,
  RefreshCw,
  Unplug,
  WifiOff,
} from "lucide-react";
import { cn } from "../../lib/format";
import { usePlayer } from "../../store/playerStore";
import { SpotifyMark } from "../icons";

/**
 * Every reason Sonic might not be playing right now, each with an action.
 *
 * There is deliberately no raw error text: each state gets a sentence written for
 * a person who does not know what an API is.
 */
export function ConnectionStatus({
  className,
  variant = "stacked",
}: {
  className?: string;
  /** `inline` is a single row for the 108px pill; `stacked` for expanded mode. */
  variant?: "inline" | "stacked";
}) {
  const connected = usePlayer((s) => s.connected);
  const authenticating = usePlayer((s) => s.authenticating);
  const error = usePlayer((s) => s.error);
  const ready = usePlayer((s) => s.ready);
  const devices = usePlayer((s) => s.devices);
  const login = usePlayer((s) => s.login);
  const cancelLogin = usePlayer((s) => s.cancelLogin);
  const clearError = usePlayer((s) => s.clearError);
  const setPanel = usePlayer((s) => s.setPanel);

  // --- authenticating ------------------------------------------------------
  if (authenticating) {
    if (variant === "inline") {
      return (
        <div className={cn("flex min-w-0 items-center gap-2", className)}>
          <Loader2 size={15} className="shrink-0 animate-spin text-spotify" strokeWidth={2.4} />
          <div className="min-w-0 flex-1 leading-tight">
            <p className="clamp-1 text-[12px] font-semibold text-white">Waiting for Spotify</p>
            <p className="clamp-1 text-2xs text-ink-muted">Approve access in the browser tab</p>
          </div>
          <button
            type="button"
            onClick={() => void cancelLogin()}
            className="shrink-0 rounded-full px-2 py-0.5 text-2xs font-medium text-ink-muted transition-colors hover:bg-white/[0.08] hover:text-white"
          >
            Cancel
          </button>
        </div>
      );
    }
    return (
      <Shell className={className}>
        <Loader2 size={22} className="animate-spin text-spotify" strokeWidth={2.2} />
        <div className="text-center">
          <p className="text-[13px] font-semibold text-white">Waiting for Spotify</p>
          <p className="mt-1 max-w-[240px] text-2xs leading-relaxed text-ink-muted">
            Finish signing in in the browser tab that just opened. This window updates
            on its own.
          </p>
        </div>
        <button
          type="button"
          onClick={() => void cancelLogin()}
          className="rounded-full px-3 py-1 text-2xs font-medium text-ink-muted transition-colors hover:bg-white/[0.08] hover:text-white"
        >
          Cancel
        </button>
      </Shell>
    );
  }

  // --- not connected -------------------------------------------------------
  if (!connected) {
    if (variant === "inline") {
      return (
        <div className={cn("flex min-w-0 items-center gap-2", className)}>
          <span className="grid h-8 w-8 shrink-0 place-items-center rounded-full bg-white/[0.07] text-white/85">
            <Unplug size={15} strokeWidth={2} />
          </span>
          <div className="min-w-0 flex-1 leading-tight">
            <p className="clamp-1 text-[12px] font-semibold text-white">Not connected</p>
            <p className="clamp-1 text-2xs text-ink-muted">Connect Spotify to control playback</p>
          </div>
          <button
            type="button"
            onClick={() => void login()}
            className="inline-flex shrink-0 items-center gap-1.5 rounded-full bg-white px-2.5 py-1 text-2xs font-semibold text-[#0a0a0c] transition-transform duration-150 ease-glass hover:scale-[1.03] active:scale-[0.98]"
          >
            <SpotifyMark className="h-2.5 w-2.5" />
            Connect
          </button>
        </div>
      );
    }
    return (
      <Shell className={className}>
        <span className="grid h-10 w-10 place-items-center rounded-full bg-white/[0.06] text-white/80">
          <Unplug size={18} strokeWidth={1.9} />
        </span>
        <div className="text-center">
          <p className="text-[13px] font-semibold text-white">Sonic is not connected</p>
          <p className="mt-1 max-w-[250px] text-2xs leading-relaxed text-ink-muted">
            Connect your Spotify account to control playback. Sonic never stores your
            password, and the audio keeps playing in Spotify.
          </p>
        </div>
        <button
          type="button"
          onClick={() => void login()}
          className="inline-flex items-center gap-1.5 rounded-full bg-white px-3.5 py-1.5 text-2xs font-semibold text-[#0a0a0c] shadow-[0_4px_14px_-6px_rgba(0,0,0,0.8)] transition-transform duration-150 ease-glass hover:scale-[1.03] active:scale-[0.98]"
        >
          <SpotifyMark className="h-3 w-3" />
          Connect Spotify
        </button>
        <button
          type="button"
          onClick={() => setPanel("settings")}
          className="text-2xs font-medium text-ink-faint transition-colors hover:text-ink-muted"
        >
          Set up client ID
        </button>
      </Shell>
    );
  }

  // --- error states --------------------------------------------------------
  if (error) {
    const { code } = error;

    if (code === "rate_limited") {
      return (
        <Shell className={className}>
          <RefreshCw size={20} className="text-amber-300/90" strokeWidth={2} />
          <div className="text-center">
            <p className="text-[13px] font-semibold text-white">Spotify is busy</p>
            <p className="mt-1 max-w-[240px] text-2xs leading-relaxed text-ink-muted">
              {error.message}
            </p>
          </div>
        </Shell>
      );
    }

    if (code === "network") {
      return (
        <Shell className={className}>
          <WifiOff size={20} className="text-ink-muted" strokeWidth={2} />
          <div className="text-center">
            <p className="text-[13px] font-semibold text-white">No connection</p>
            <p className="mt-1 max-w-[240px] text-2xs leading-relaxed text-ink-muted">
              {error.message}
            </p>
          </div>
          <button
            type="button"
            onClick={clearError}
            className="rounded-full px-3 py-1 text-2xs font-medium text-ink-muted transition-colors hover:bg-white/[0.08] hover:text-white"
          >
            Dismiss
          </button>
        </Shell>
      );
    }

    // Everything else is a generic, honest failure.
    return (
      <Shell className={className}>
        <AlertCircle size={20} className="text-ink-muted" strokeWidth={2} />
        <div className="text-center">
          <p className="text-[13px] font-semibold text-white">Something went wrong</p>
          <p className="mt-1 max-w-[250px] text-2xs leading-relaxed text-ink-muted">
            {error.message}
          </p>
        </div>
        <button
          type="button"
          onClick={clearError}
          className="rounded-full px-3 py-1 text-2xs font-medium text-ink-muted transition-colors hover:bg-white/[0.08] hover:text-white"
        >
          Dismiss
        </button>
      </Shell>
    );
  }

  // --- connected but no device --------------------------------------------
  if (ready && devices.length === 0) {
    if (variant === "inline") {
      return (
        <div className={cn("flex min-w-0 items-center gap-2", className)}>
          <MonitorOff size={15} className="shrink-0 text-ink-muted" strokeWidth={2} />
          <div className="min-w-0 flex-1 leading-tight">
            <p className="clamp-1 text-[12px] font-semibold text-white">No device found</p>
            <p className="clamp-1 text-2xs text-ink-muted">Start playback in Spotify first</p>
          </div>
        </div>
      );
    }
    return (
      <Shell className={className}>
        <MonitorOff size={20} className="text-ink-muted" strokeWidth={1.9} />
        <div className="text-center">
          <p className="text-[13px] font-semibold text-white">No device found</p>
          <p className="mt-1 max-w-[245px] text-2xs leading-relaxed text-ink-muted">
            Open Spotify on this computer or your phone and start playing something.
            Sonic will pick it up automatically.
          </p>
        </div>
      </Shell>
    );
  }

  // --- first load ----------------------------------------------------------
  if (variant === "inline") {
    return (
      <div className={cn("flex min-w-0 items-center gap-2", className)}>
        <Loader2 size={14} className="shrink-0 animate-spin text-ink-faint" strokeWidth={2.2} />
        <p className="clamp-1 text-2xs font-medium text-ink-faint">Checking Spotify…</p>
      </div>
    );
  }
  return (
    <Shell className={className}>
      <Loader2 size={20} className="animate-spin text-ink-muted" strokeWidth={2} />
      <p className="text-2xs font-medium text-ink-faint">Checking Spotify…</p>
    </Shell>
  );
}

function Shell({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <motion.div
      initial={{ opacity: 0, y: 4 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.26, ease: [0.32, 0.72, 0, 1] }}
      className={cn(
        "flex flex-col items-center justify-center gap-2.5 px-4 py-3",
        className,
      )}
    >
      {children}
    </motion.div>
  );
}