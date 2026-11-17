import { useEffect, useState } from "react";
import { motion } from "framer-motion";
import {
  Check,
  ChevronLeft,
  Copy,
  Inbox,
  LogOut,
  MonitorDown,
  RotateCcw,
  ShieldCheck,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";
import { cn } from "../../lib/format";
import { usePlayer } from "../../store/playerStore";
import { SpotifyMark } from "../icons";
import { DevicePicker } from "../DevicePicker/DevicePicker";

/**
 * Settings panel.
 *
 * Rendered inside the expanded window rather than a separate window: Sonic is a
 * single surface, and a second window would break the "floating object" feel.
 */
export function SettingsPanel() {
  const closePanel = usePlayer((s) => s.closePanel);
  const state = usePlayer((s) => s.state);
  const settings = usePlayer((s) => s.settings);
  const saveSettings = usePlayer((s) => s.saveSettings);
  const login = usePlayer((s) => s.login);
  const logout = usePlayer((s) => s.logout);
  const quit = usePlayer((s) => s.quit);
  const setAlwaysOnTop = usePlayer((s) => s.setAlwaysOnTop);
  const setMode = usePlayer((s) => s.setMode);
  const error = usePlayer((s) => s.error);

  const [clientId, setClientId] = useState("");
  const [savedFlash, setSavedFlash] = useState(false);

  // Seed the client-id field from the backend's "is it configured" flag. The
  // actual value is never sent to the frontend, so this is a password-style
  // field that only ever holds what the user types.
  useEffect(() => {
    setClientId("");
  }, []);

  if (!settings) return null;

  const connected = state?.connected ?? false;
  const accountLabel = state?.accountLabel ?? "";
  const redirectUri = settings.redirectUri;

  const flash = () => {
    setSavedFlash(true);
    window.setTimeout(() => setSavedFlash(false), 1400);
  };

  const saveClientId = async () => {
    const trimmed = clientId.trim();
    if (!trimmed) return;
    await saveSettings({ clientId: trimmed });
    setClientId("");
    flash();
  };

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.2, ease: [0.42, 0, 0.58, 1] }}
      className="flex h-full min-h-0 w-full flex-col"
    >
      {/* Header */}
      <div className="flex items-center justify-between px-4 pb-1 pt-3">
        <div className="flex items-center gap-1.5">
          <button
            type="button"
            onClick={closePanel}
            aria-label="Back"
            title="Back"
            className="ctl -ml-1 h-7 w-7"
          >
            <ChevronLeft size={16} strokeWidth={2.3} />
          </button>
          <h2 className="text-[12.5px] font-semibold tracking-[-0.01em] text-white">
            Settings
          </h2>
        </div>
        <button
          type="button"
          onClick={closePanel}
          aria-label="Close settings"
          className="ctl h-7 w-7"
        >
          <X size={14} strokeWidth={2.3} />
        </button>
      </div>

      <div className="scroll-slim min-h-0 flex-1 overflow-y-auto overscroll-contain px-4 pb-3">
        {/* ---------------- Spotify account ---------------- */}
        <Section
          title="Spotify"
          icon={<SpotifyMark className="h-3 w-3" />}
        >
          {connected ? (
            <div className="space-y-2.5">
              <div className="flex items-center gap-2 rounded-[12px] bg-white/[0.05] px-2.5 py-2">
                <span
                  className={cn(
                    "h-1.5 w-1.5 shrink-0 rounded-full bg-spotify",
                    "shadow-[0_0_6px_rgba(30,215,96,0.85)]",
                  )}
                />
                <span className="clamp-1 flex-1 text-[11.5px] font-medium text-white">
                  {accountLabel || "Connected"}
                </span>
                <span className="shrink-0 text-2xs uppercase tracking-[0.06em] text-ink-faint">
                  {state?.isPremium ? "Premium" : "Free"}
                </span>
              </div>

              <div className="flex items-center justify-between">
                <span className="text-[11.5px] text-ink-muted">Playback device</span>
                <DevicePicker variant="row" />
              </div>

              <div className="flex gap-1.5 pt-0.5">
                <button
                  type="button"
                  onClick={() => void login()}
                  className="flex flex-1 items-center justify-center gap-1.5 rounded-full bg-white/[0.08] py-1.5 text-2xs font-semibold text-white transition-colors hover:bg-white/[0.13]"
                >
                  <RotateCcw size={11} strokeWidth={2.4} />
                  Reconnect
                </button>
                <button
                  type="button"
                  onClick={() => void logout()}
                  className="flex flex-1 items-center justify-center gap-1.5 rounded-full bg-white/[0.04] py-1.5 text-2xs font-semibold text-ink-muted transition-colors hover:bg-white/[0.08] hover:text-white"
                >
                  <LogOut size={11} strokeWidth={2.4} />
                  Disconnect
                </button>
              </div>
            </div>
          ) : (
            <div className="space-y-2.5">
              <p className="text-2xs leading-relaxed text-ink-muted">
                Sonic uses the official Spotify Web API to control playback on your
                existing account. It never plays audio itself.
              </p>

              <label className="block">
                <span className="mb-1 block text-2xs font-semibold uppercase tracking-[0.08em] text-ink-faint">
                  Client ID
                </span>
                <div className="flex gap-1.5">
                  <input
                    type="password"
                    value={clientId}
                    onChange={(e) => setClientId(e.target.value)}
                    placeholder={settings.hasClientId ? "••••••••••••••••" : "Paste your client ID"}
                    autoComplete="off"
                    spellCheck={false}
                    className={cn(
                      "min-w-0 flex-1 rounded-[10px] bg-black/30 px-2.5 py-1.5",
                      "text-[11px] text-white placeholder:text-ink-faint",
                      "ring-1 ring-inset ring-white/[0.09] outline-none",
                      "transition-shadow focus:ring-white/25",
                    )}
                  />
                  <button
                    type="button"
                    onClick={() => void saveClientId()}
                    disabled={!clientId.trim()}
                    className={cn(
                      "shrink-0 rounded-[10px] bg-white px-2.5 text-2xs font-semibold text-[#0a0a0c]",
                      "transition-opacity",
                      clientId.trim() ? "opacity-100" : "opacity-40",
                    )}
                  >
                    Save
                  </button>
                </div>
              </label>

              <div className="rounded-[10px] bg-black/25 px-2.5 py-2">
                <p className="mb-1 text-2xs font-semibold uppercase tracking-[0.08em] text-ink-faint">
                  Redirect URI
                </p>
                <div className="flex items-center gap-1.5">
                  <code className="clamp-1 flex-1 text-[10.5px] text-ink-muted">
                    {redirectUri}
                  </code>
                  <button
                    type="button"
                    onClick={() => {
                      void navigator.clipboard?.writeText(redirectUri);
                      flash();
                    }}
                    aria-label="Copy redirect URI"
                    title="Copy"
                    className="ctl h-6 w-6 shrink-0"
                  >
                    {savedFlash ? (
                      <Check size={11} strokeWidth={2.6} className="text-spotify" />
                    ) : (
                      <Copy size={11} strokeWidth={2.2} />
                    )}
                  </button>
                </div>
              </div>

              <button
                type="button"
                onClick={() => void login()}
                className={cn(
                  "flex w-full items-center justify-center gap-1.5 rounded-full py-2",
                  "bg-white text-2xs font-semibold text-[#0a0a0c]",
                  "transition-transform duration-150 ease-glass hover:scale-[1.015] active:scale-[0.99]",
                )}
              >
                <SpotifyMark className="h-3 w-3" />
                Connect Spotify
              </button>

              <p className="text-[9.5px] leading-relaxed text-ink-faint">
                Need a client ID? Create a free app at developer.spotify.com and add the
                redirect URI above. No client secret is required.
              </p>
            </div>
          )}

          {error?.code === "unauthorized" && (
            <p className="mt-2 text-2xs leading-snug text-amber-300/90">
              {error.message}
            </p>
          )}
        </Section>

        {/* ---------------- Appearance ---------------- */}
        <Section title="Appearance" icon={<Sparkles size={11} strokeWidth={2.2} />}>
          <ToggleRow
            label="Solid surface"
            hint="Flat black pill like the iPhone Dynamic Island — no blur or colour"
            checked={settings.solidSurface ?? false}
            onChange={(v) => void saveSettings({ solidSurface: v })}
          />
          <p className="mt-1 mb-1.5 text-[9.5px] leading-snug text-ink-faint/80">
            {settings.solidSurface
              ? "Solid is on. Transparency and blur below are stored but not applied."
              : "Liquid Glass is on. Turn on Solid for the flat Dynamic Island look."}
          </p>
          <SliderRow
            label="Transparency"
            value={settings.transparency}
            onChange={(v) => void saveSettings({ transparency: v })}
            hint="How much of the desktop shows through the glass"
          />
          <SliderRow
            label="Blur"
            value={settings.blur}
            onChange={(v) => void saveSettings({ blur: v })}
            hint="Strength of the frosted backdrop"
          />
          <div className="mt-1 flex items-center justify-between">
            <span className="text-[11.5px] text-ink-muted">Start in</span>
            <div className="flex gap-1 rounded-full bg-black/25 p-0.5">
              {(["pill", "expanded", "mini"] as const).map((option) => (
                <button
                  key={option}
                  type="button"
                  // Goes through the player store, not the settings mirror: the
                  // store's `setMode` clears any hover collapse and lets Rust
                  // animate the window. The mirror only patches values.
                  onClick={() => void setMode(option)}
                  aria-pressed={settings.mode === option}
                  className={cn(
                    "rounded-full px-2 py-[3px] text-2xs font-medium capitalize transition-colors",
                    settings.mode === option
                      ? "bg-white/[0.14] text-white"
                      : "text-ink-faint hover:text-ink-muted",
                  )}
                >
                  {option}
                </button>
              ))}
            </div>
          </div>
        </Section>

        {/* ---------------- Behaviour ---------------- */}
        <Section title="Behaviour" icon={<MonitorDown size={11} strokeWidth={2.2} />}>
          <ToggleRow
            label="Always on top"
            hint="Keep Sonic above other windows"
            checked={settings.alwaysOnTop}
            onChange={(v) => void setAlwaysOnTop(v)}
          />
          <ToggleRow
            label="Expand on hover"
            hint="Mini expands into pill while pointer rests on it"
            checked={settings.hoverExpand ?? true}
            onChange={(v) => void saveSettings({ hoverExpand: v, hoverMini: v })}
          />
          <ToggleRow
            label="Start with Windows"
            hint="Launch Sonic when you sign in"
            checked={settings.startWithWindows}
            onChange={(v) => void saveSettings({ startWithWindows: v })}
          />
          <ToggleRow
            label="Tray icon"
            hint="Keep Sonic reachable when hidden"
            checked={settings.showTray}
            onChange={(v) => void saveSettings({ showTray: v })}
          />
        </Section>

        {/* ---------------- Pocket ---------------- */}
        <Section title="Pocket" icon={<Inbox size={11} strokeWidth={2.2} />}>
          <p className="text-2xs leading-relaxed text-ink-muted">
            Pocket keeps files you drop on Sonic and an optional clipboard history.
            Everything stays on this PC — nothing is uploaded, and nothing is
            logged.
          </p>

          <ToggleRow
            label="File Shelf"
            hint="Accept files dropped from Explorer"
            checked={settings.shelfEnabled}
            onChange={(v) => void saveSettings({ shelfEnabled: v })}
          />

          {settings.shelfEnabled && (
            <div className="flex items-center justify-between gap-2 pl-0.5">
              <span className="min-w-0">
                <span className="block truncate text-[11.5px] text-ink-muted">
                  Max file size
                </span>
                <span className="block truncate text-[9.5px] leading-snug text-ink-faint/80">
                  {settings.shelfMaxMib === 0 ? "No limit" : `${settings.shelfMaxMib} MiB`}
                </span>
              </span>
              <div className="flex gap-1 rounded-full bg-black/25 p-0.5">
                {[0, 25, 100, 500].map((mib) => (
                    <button
                      key={mib}
                      type="button"
                      onClick={() => void saveSettings({ shelfMaxMib: mib })}
                      aria-pressed={settings.shelfMaxMib === mib}
                      className={cn(
                        "rounded-full px-1.5 py-[3px] text-[9.5px] font-medium transition-colors",
                        settings.shelfMaxMib === mib
                          ? "bg-white/[0.14] text-white"
                          : "text-ink-faint hover:text-ink-muted",
                      )}
                    >
                      {mib === 0 ? "Any" : `${mib}M`}
                    </button>
                  ))}
              </div>
            </div>
          )}

          <div className="my-1 border-t border-white/[0.06]" />

          <ToggleRow
            label="Clipboard history"
            hint="Remember what you copy, on this PC only"
            checked={settings.clipboardEnabled}
            onChange={(v) => void saveSettings({ clipboardEnabled: v })}
          />

          {settings.clipboardEnabled && (
            <>
              <ToggleRow
                label="Private mode"
                hint="Stop capturing; existing entries are kept"
                checked={settings.clipboardPrivate}
                onChange={(v) => void saveSettings({ clipboardPrivate: v })}
              />
              <div className="flex items-center justify-between gap-2 pl-0.5">
                <span className="min-w-0">
                  <span className="block truncate text-[11.5px] text-ink-muted">Keep</span>
                  <span className="block truncate text-[9.5px] leading-snug text-ink-faint/80">
                    {settings.clipboardRetentionDays} days
                  </span>
                </span>
                <div className="flex gap-1 rounded-full bg-black/25 p-0.5">
                  {[1, 7, 30].map((days) => (
                    <button
                      key={days}
                      type="button"
                      onClick={() => void saveSettings({ clipboardRetentionDays: days })}
                      aria-pressed={settings.clipboardRetentionDays === days}
                      className={cn(
                        "rounded-full px-1.5 py-[3px] text-[9.5px] font-medium transition-colors",
                        settings.clipboardRetentionDays === days
                          ? "bg-white/[0.14] text-white"
                          : "text-ink-faint hover:text-ink-muted",
                      )}
                    >
                      {days}d
                    </button>
                  ))}
                </div>
              </div>
              <p className="text-[9.5px] leading-snug text-ink-faint/80">
                Pin an entry to keep it past the retention window. Passwords,
                card numbers and one-time codes are never captured.
              </p>
            </>
          )}
        </Section>

        {/* ---------------- About ---------------- */}
        <Section title="About" icon={<ShieldCheck size={11} strokeWidth={2.2} />}>
          <p className="text-2xs leading-relaxed text-ink-muted">
            Sonic controls playback on your own Spotify device through the official Web
            API. Audio is never streamed, stored, or modified. Your token is kept in
            Windows Credential Manager and never leaves this app.
          </p>
          <div className="mt-2 flex gap-1.5">
            <button
              type="button"
              onClick={() => void quit()}
              className={cn(
                "flex flex-1 items-center justify-center gap-1.5 rounded-full py-1.5",
                "bg-white/[0.04] text-2xs font-semibold text-ink-muted",
                "transition-colors hover:bg-red-500/15 hover:text-red-300",
              )}
            >
              <Trash2 size={11} strokeWidth={2.3} />
              Quit Sonic
            </button>
          </div>
        </Section>

        <p className="pb-1 pt-2 text-center text-[9.5px] text-ink-faint/70">
          Sonic 0.1.0 · Not affiliated with Spotify
        </p>
      </div>
    </motion.div>
  );
}

function Section({
  title,
  icon,
  children,
}: {
  title: string;
  icon?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section className="mt-2.5 border-t border-white/[0.06] pt-2.5 first:border-0 first:pt-0">
      <h3 className="mb-1.5 flex items-center gap-1.5 text-2xs font-semibold uppercase tracking-[0.09em] text-ink-faint">
        {icon}
        {title}
      </h3>
      <div className="space-y-1.5">{children}</div>
    </section>
  );
}

function SliderRow({
  label,
  value,
  onChange,
  hint,
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
  hint?: string;
}) {
  return (
    <div>
      <div className="mb-0.5 flex items-center justify-between">
        <span className="text-[11.5px] text-ink-muted">{label}</span>
        <span className="text-2xs font-medium text-ink-faint text-mono-num">{value}</span>
      </div>
      <input
        type="range"
        min={0}
        max={100}
        step={1}
        value={value}
        aria-label={label}
        aria-valuetext={`${value} percent`}
        onChange={(e) => onChange(Number(e.target.value))}
        className="range"
      />
      {hint && <p className="mt-0.5 text-[9.5px] leading-snug text-ink-faint/80">{hint}</p>}
    </div>
  );
}

function ToggleRow({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint?: string;
  checked: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between gap-2">
      <span className="min-w-0">
        <span className="block truncate text-[11.5px] text-ink-muted">{label}</span>
        {hint && (
          <span className="block truncate text-[9.5px] leading-snug text-ink-faint/80">
            {hint}
          </span>
        )}
      </span>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        onClick={() => onChange(!checked)}
        className={cn(
          "relative h-[17px] w-[30px] shrink-0 rounded-full transition-colors duration-200 ease-glass",
          checked ? "bg-spotify/85" : "bg-white/[0.14]",
        )}
      >
        <span
          className={cn(
            "absolute top-[2px] h-[13px] w-[13px] rounded-full bg-white shadow-[0_1px_3px_rgba(0,0,0,0.5)]",
            "transition-[left] duration-200 ease-glass",
            checked ? "left-[15px]" : "left-[2px]",
          )}
        />
      </button>
    </div>
  );
}