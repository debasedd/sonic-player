import { create } from "zustand";
import { api } from "../services/spotify/api";
import { EVENTS } from "../types/spotify";
import type {
  AppError,
  ClipboardEntry,
  Device,
  LocalControls,
  Playback,
  ShelfEntry,
  Transport,
  UiSettings,
  UiState,
  WindowMode,
} from "../types/spotify";

/**
 * Player state.
 *
 * The backend is authoritative: it pushes a complete state snapshot after every
 * poll and command, omitting unchanged local cover art by revision. Local state
 * exists only for brief optimistic flips and drag previews, corrected by the next
 * snapshot.
 */

export type Panel = "none" | "settings" | "pocket";

/** Which Pocket section is showing. */
export type PocketTab = "files" | "clipboard";

interface PlayerStore {
  // --- snapshot from Rust ---------------------------------------------
  state: UiState | null;
  connected: boolean;
  authenticating: boolean;
  playback: Playback | null;
  devices: Device[];
  error: AppError | null;
  settings: UiSettings | null;
  ready: boolean;
  localMediaAvailable: boolean;
  localMediaPlaying: boolean;
  localMediaTitle: string | null;
  localMediaArtist: string | null;
  localMediaArtworkDataUrl: string | null;
  /** Local timeline, so the bar and time labels work without Spotify. */
  localMediaPositionMs: number | null;
  localMediaDurationMs: number | null;
  localMediaShuffle: boolean | null;
  localMediaRepeat: string | null;
  localMediaControls: LocalControls;
  /** Transient hover-expansion flag from Rust (true = drawing pill on hover). */
  hoverExpand?: boolean;
  hoverMini?: boolean;
  /**
   * The mode the UI must draw. Differs from `settings.mode` only while the
   * pointer is resting on the player.
   */
  effectiveMode: WindowMode;

  // --- local UI state --------------------------------------------------
  panel: Panel;
  /** True while the user drags the progress bar (pauses interpolation). */
  scrubbing: boolean;
  /** Local optimistic override so play/pause reacts in <16ms. */
  optimisticPlaying: boolean | null;
  /** Pending volume while the slider moves; sent on release. */
  pendingVolume: number | null;
  hydrated: boolean;
  /** Bumped whenever the settings panel should re-open (tray menu). */
  settingsRequest: number;

  // --- Pocket ----------------------------------------------------------
  /** Which section of Pocket is open. */
  pocketTab: PocketTab;
  /** Shelf contents, loaded lazily when Pocket opens. */
  shelf: ShelfEntry[];
  /** Vault contents, loaded lazily when the Clipboard tab opens. */
  vault: ClipboardEntry[];
  /** True while a Pocket request is in flight. */
  pocketBusy: boolean;
  /** Short confirmation shown after an import, e.g. "2 files added". */
  pocketToast: string | null;
  /** Bumped after a capture so the Clipboard tab re-reads the vault. */
  vaultRevision: number;
  /** True while a native file drag hovers the player. */
  dropActive: boolean;

  // --- actions ---------------------------------------------------------
  hydrate: () => Promise<void>;
  applyState: (next: UiState) => void;
  setPanel: (panel: Panel) => void;
  openSettings: () => void;
  closePanel: () => void;

  setPointerClaimed: (claimed: boolean) => Promise<void>;

  setScrubbing: (value: boolean) => void;
  setOptimisticPlaying: (value: boolean | null) => void;
  setPendingVolume: (value: number | null) => void;

  transport: (command: Transport) => Promise<void>;
  seek: (positionMs: number) => Promise<void>;
  commitVolume: (volume: number) => Promise<void>;
  toggleShuffle: () => Promise<void>;
  cycleRepeat: () => Promise<void>;
  selectDevice: (deviceId: string) => Promise<void>;

  login: () => Promise<void>;
  cancelLogin: () => Promise<void>;
  logout: () => Promise<void>;

  saveSettings: (patch: Record<string, unknown>) => Promise<void>;
  setMode: (mode: WindowMode) => Promise<void>;
  setAlwaysOnTop: (enabled: boolean) => Promise<void>;
  hide: () => Promise<void>;
  quit: () => Promise<void>;
  openInSpotify: () => Promise<void>;

  reportError: (error: AppError) => void;
  clearError: () => void;

  // --- Pocket actions ---------------------------------------------------
  openPocket: (tab?: PocketTab) => void;
  setPocketTab: (tab: PocketTab) => void;
  refreshShelf: () => Promise<void>;
  refreshVault: () => Promise<void>;
  /** Imports dropped absolute paths, then reports what happened. */
  importFiles: (paths: string[]) => Promise<void>;
  toggleShelfPinned: (id: string, pinned: boolean) => Promise<void>;
  removeShelfItem: (id: string) => Promise<void>;
  clearShelf: () => Promise<void>;
  openShelfItem: (path: string) => Promise<void>;
  revealShelfItem: (path: string) => Promise<void>;
  dragShelfOut: (paths: string[], ghost?: string) => Promise<void>;
  restoreClipboardItem: (id: string) => Promise<void>;
  toggleVaultPinned: (id: string, pinned: boolean) => Promise<void>;
  removeVaultItem: (id: string) => Promise<void>;
  clearVault: () => Promise<void>;
  captureClipboardNow: () => Promise<void>;
  setDropActive: (active: boolean) => void;
  showPocketToast: (message: string | null) => void;
}

export const usePlayer = create<PlayerStore>((set, get) => ({
  state: null,
  connected: false,
  authenticating: false,
  playback: null,
  devices: [],
  error: null,
  settings: null,
  ready: false,
  localMediaAvailable: false,
  localMediaPlaying: false,
  localMediaTitle: null,
  localMediaArtist: null,
  localMediaArtworkDataUrl: null,
  localMediaPositionMs: null,
  localMediaDurationMs: null,
  localMediaShuffle: null,
  localMediaRepeat: null,
  localMediaControls: {
    canPlay: false,
    canPause: false,
    canNext: false,
    canPrevious: false,
    canSeek: false,
    canShuffle: false,
    canRepeat: false,
  },
  hoverMini: false,
  hoverExpand: false,
  effectiveMode: "mini",

  panel: "none",
  scrubbing: false,
  optimisticPlaying: null,
  pendingVolume: null,
  hydrated: false,
  settingsRequest: 0,
  pocketTab: "files",
  shelf: [],
  vault: [],
  pocketBusy: false,
  pocketToast: null,
  vaultRevision: 0,
  dropActive: false,

  /**
   * Adopt a backend snapshot wholesale.
   *
   * The backend is authoritative for geometry: `effectiveMode` is what Rust has
   * already sized the window to, so it is adopted rather than derived. Deriving it
   * here from `settings.mode` would draw a pill inside a window that just shrank.
   */

  /** First load: the backend knows everything, so just ask it. */
  hydrate: async () => {
    try {
      // `get_bootstrap` returns both the state and the platform facts.
      const bootstrap = await api.bootstrap();
      get().applyState(bootstrap.state);
      if (bootstrap.state.localMediaArtworkRevision !== undefined) {
        void api
          .ackLocalMediaArtworkRevision(bootstrap.state.localMediaArtworkRevision)
          .catch(() => {});
      }
      set({ hydrated: true });
    } catch (err) {
      // A failure here means the webview lost its bridge, which is fatal for a
      // Tauri app; surface it rather than pretending to be disconnected.
      set({
        hydrated: true,
        error: {
          code: "local",
          message:
            err instanceof Error
              ? `Could not reach the Sonic backend: ${err.message}`
              : "Could not reach the Sonic backend.",
          retryAfterSeconds: null,
        },
      });
    }
  },

  /** Adopt a backend snapshot, retaining cover art omitted from unchanged events. */
  applyState: (next) => {
    const current = get();
    const previousRevision = current.state?.localMediaArtworkRevision ?? -1;
    const incomingRevision = next.localMediaArtworkRevision ?? previousRevision;
    const staleArtworkEvent = incomingRevision < previousRevision;
    const artworkRevision = staleArtworkEvent
      ? previousRevision
      : incomingRevision;
    const artwork = staleArtworkEvent
      ? current.localMediaArtworkDataUrl
      : next.localMediaArtworkDataUrl !== undefined
        ? next.localMediaArtworkDataUrl
        : incomingRevision !== previousRevision
          ? null
          : current.localMediaArtworkDataUrl;
    const snapshot: UiState = {
      ...next,
      localMediaArtworkDataUrl: artwork,
      localMediaArtworkRevision: Math.max(0, artworkRevision),
    };
    set({
      state: snapshot,
      connected: next.connected,
      authenticating: next.authenticating,
      playback: next.playback,
      devices: next.devices,
      error: next.error,
      settings: next.settings,
      ready: next.ready,
      localMediaAvailable: next.localMediaAvailable ?? false,
      localMediaPlaying: next.localMediaPlaying ?? false,
      localMediaTitle: next.localMediaTitle ?? null,
      localMediaArtist: next.localMediaArtist ?? null,
      localMediaArtworkDataUrl: artwork,
      localMediaPositionMs: next.localMediaPositionMs ?? null,
      localMediaDurationMs: next.localMediaDurationMs ?? null,
      localMediaShuffle: next.localMediaShuffle ?? null,
      localMediaRepeat: next.localMediaRepeat ?? null,
      localMediaControls: next.localMediaControls ?? {
        canPlay: false,
        canPause: false,
        canNext: false,
        canPrevious: false,
        canSeek: false,
        canShuffle: false,
        canRepeat: false,
      },
      hoverMini: next.hoverMini ?? false,
      hoverExpand: next.hoverExpand ?? false,
      effectiveMode: next.effectiveMode,
      // Rust owns the drop-target flag now (it drives the window geometry). The
      // webview adopts it rather than deciding for itself, so the target can
      // never be drawn over a window that has not grown to fit it.
      dropActive: next.settings?.dropTarget ?? false,
      // Any fresh snapshot resolves a pending optimistic flip.
      optimisticPlaying: null,
      pendingVolume: null,
    });
  },

  /**
   * Claims the pointer for a drag or a popover.
   *
   * The Rust watcher polls the OS cursor at 60 Hz; this is how it learns that the
   * pointer is busy. Skipping it is what makes a drag collapse the window to mini
   * mid-gesture and lose its grip.
   */
  setPointerClaimed: async (claimed) => {
    try {
      await api.setPointerClaimed(claimed);
    } catch {
      // The watcher's own geometry checks are the fallback; a failed claim only
      // means hover may be evaluated during a drag, never a crash.
    }
  },

  setPanel: (panel) => set({ panel }),
  openSettings: () =>
    set((s) => ({ panel: "settings", settingsRequest: s.settingsRequest + 1 })),
  closePanel: () => set({ panel: "none", dropActive: false }),

  setScrubbing: (scrubbing) => set({ scrubbing }),
  setOptimisticPlaying: (optimisticPlaying) => set({ optimisticPlaying }),
  setPendingVolume: (pendingVolume) => set({ pendingVolume }),

  transport: async (command) => {
    if (command === "toggle" || command === "play" || command === "pause") {
      const current = get().localMediaAvailable
        ? get().localMediaPlaying
        : get().playback?.isPlaying ?? false;
      const next = command === "play" ? true : command === "pause" ? false : !current;
      get().setOptimisticPlaying(next);
    }

    try {
      await api.transport(command);
    } catch (err) {
      get().setOptimisticPlaying(null);
      get().reportError(toAppError(err));
    }
  },

  seek: async (positionMs) => {
    // Show the target immediately; the snapshot will confirm it.
    const local = get().localMediaAvailable;
    if (local) {
      set({ localMediaPositionMs: Math.max(0, Math.round(positionMs)) });
    } else {
      const current = get().playback;
      if (current) {
        set({ playback: { ...current, progressMs: positionMs } });
      }
    }
    try {
      await api.seek(Math.max(0, Math.round(positionMs)));
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  commitVolume: async (volume) => {
    const clamped = Math.max(0, Math.min(100, Math.round(volume)));
    const current = get().playback;
    if (current) {
      set({
        playback: {
          ...current,
          device: { ...current.device, volumePercent: clamped },
        },
      });
    }
    set({ pendingVolume: clamped });
    try {
      await api.setVolume(clamped);
    } catch (err) {
      get().reportError(toAppError(err));
    } finally {
      set({ pendingVolume: null });
    }
  },

  toggleShuffle: async () => {
    const local = get().localMediaAvailable;
    const next = !(local
      ? get().localMediaShuffle ?? false
      : get().playback?.shuffleState ?? false);
    if (local) {
      set({ localMediaShuffle: next });
    } else {
      const current = get().playback;
      if (current) {
        set({ playback: { ...current, shuffleState: next } });
      }
    }
    try {
      await api.setShuffle(next);
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  cycleRepeat: async () => {
    const order = ["off", "context", "track"] as const;
    const local = get().localMediaAvailable;
    const current = (local
      ? get().localMediaRepeat ?? "off"
      : get().playback?.repeatState ?? "off") as (typeof order)[number];
    const next = order[(order.indexOf(current) + 1) % order.length];
    if (local) {
      set({ localMediaRepeat: next });
    } else {
      const playback = get().playback;
      if (playback) {
        set({ playback: { ...playback, repeatState: next } });
      }
    }
    try {
      await api.setRepeat(next);
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  selectDevice: async (deviceId) => {
    try {
      await api.selectDevice(deviceId, true);
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  login: async () => {
    set({ error: null });
    try {
      await api.startLogin();
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  cancelLogin: async () => {
    try {
      await api.cancelLogin();
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  logout: async () => {
    try {
      await api.logout();
      set({ playback: null, devices: [] });
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  saveSettings: async (patch) => {
    try {
      await api.updateSettings(patch);
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  setMode: async (mode) => {
    try {
      await api.setWindowMode(mode);
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  setAlwaysOnTop: async (enabled) => {
    try {
      await api.setAlwaysOnTop(enabled);
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  hide: async () => {
    try {
      await api.hideWindow();
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  quit: async () => {
    try {
      await api.quit();
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  openInSpotify: async () => {
    try {
      await api.openSpotify();
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  reportError: (error) => set({ error }),
  clearError: () => set({ error: null }),

  // --- Pocket -----------------------------------------------------------

  openPocket: (tab) =>
    set((s) => ({
      panel: "pocket",
      pocketTab: tab ?? s.pocketTab,
      // Claim the pointer: the window grows to the Pocket height, and the hover
      // watcher must not collapse it while the user is reading the list.
      pocketBusy: false,
    })),

  setPocketTab: (pocketTab) => set({ pocketTab }),

  /** Re-reads the shelf index. Cheap: one JSON read, no file bytes. */
  refreshShelf: async () => {
    try {
      set({ shelf: await api.shelfList() });
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  /** Re-reads the vault index. Cheap: one JSON read, images stay on disk. */
  refreshVault: async () => {
    try {
      const vault = await api.clipboardList();
      set((s) => ({ vault, vaultRevision: s.vaultRevision + 1 }));
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  /**
   * Imports dropped paths into the shelf.
   *
   * The native drop gives absolute paths, so this is a straight copy on the Rust
   * side. The toast text is built from the report rather than assumed: a partial
   * import ("2 added, 1 skipped") must not claim success for the skipped file.
   */
  importFiles: async (paths) => {
    if (paths.length === 0) return;
    set({ pocketBusy: true, dropActive: false });
    try {
      const report = await api.shelfImport(paths);
      await get().refreshShelf();

      const added = report.added.length;
      const skipped = report.skipped.length;
      let message: string;
      if (added === 0 && skipped === 0) {
        message = "Nothing to add";
      } else if (skipped === 0) {
        message = added === 1 ? "1 file added" : `${added} files added`;
      } else if (added === 0) {
        // Show the first reason; it is the most likely thing the user can act on.
        message = `Could not add ${skipped === 1 ? "that file" : `those ${skipped} files`}: ${
          report.skipped[0].reason
        }`;
      } else {
        message = `${added} added, ${skipped} skipped`;
      }
      get().showPocketToast(message);
    } catch (err) {
      get().showPocketToast(null);
      get().reportError(toAppError(err));
    } finally {
      set({ pocketBusy: false, dropActive: false });
    }
  },

  toggleShelfPinned: async (id, pinned) => {
    try {
      await api.shelfSetPinned(id, pinned);
      await get().refreshShelf();
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  removeShelfItem: async (id) => {
    try {
      await api.shelfRemove(id);
      await get().refreshShelf();
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  clearShelf: async () => {
    try {
      const removed = await api.shelfClear();
      await get().refreshShelf();
      get().showPocketToast(
        removed === 0 ? "Nothing to clear" : `${removed} ${removed === 1 ? "file" : "files"} removed`,
      );
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  openShelfItem: async (path) => {
    try {
      await api.shelfOpen(path);
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  revealShelfItem: async (path) => {
    try {
      await api.shelfReveal(path);
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  /**
   * Starts a native drag of stored copies.
   *
   * The promise resolves only when the Windows modal loop ends (drop or cancel),
   * so the caller must not block the UI on it. A failed drag is reported, not
   * thrown, because the shelf is still intact either way.
   */
  dragShelfOut: async (paths, ghost) => {
    if (paths.length === 0) return;
    try {
      await api.shelfDragOut(paths, ghost);
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  restoreClipboardItem: async (id) => {
    try {
      await api.clipboardRestore(id);
      get().showPocketToast("Copied to clipboard");
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  toggleVaultPinned: async (id, pinned) => {
    try {
      await api.clipboardSetPinned(id, pinned);
      await get().refreshVault();
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  removeVaultItem: async (id) => {
    try {
      await api.clipboardRemove(id);
      await get().refreshVault();
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  clearVault: async () => {
    try {
      const removed = await api.clipboardClear();
      await get().refreshVault();
      get().showPocketToast(
        removed === 0
          ? "Nothing to clear"
          : `${removed} ${removed === 1 ? "entry" : "entries"} removed`,
      );
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  captureClipboardNow: async () => {
    try {
      const captured = await api.clipboardCaptureNow();
      if (captured) {
        await get().refreshVault();
      } else {
        get().showPocketToast("Nothing new on the clipboard");
      }
    } catch (err) {
      get().reportError(toAppError(err));
    }
  },

  setDropActive: (dropActive) => set({ dropActive }),

  showPocketToast: (pocketToast) => {
    set({ pocketToast });
    if (!pocketToast) return;
    // Auto-dismiss: a toast that stays forever is a notification the user has to
    // clean up. 2.4 s is long enough to read "2 files added" and short enough to
    // be gone before it becomes furniture.
    window.setTimeout(() => {
      if (get().pocketToast === pocketToast) set({ pocketToast: null });
    }, 2400);
  },
}));

/** Normalises whatever the bridge threw into the backend's error shape. */
function toAppError(err: unknown): AppError {
  if (typeof err === "string") {
    // Tauri serialises a Rust error as a plain string; try to parse the shape
    // our `AppError` serialiser produces, otherwise show it verbatim.
    try {
      const parsed = JSON.parse(err) as Partial<AppError>;
      if (parsed && typeof parsed.message === "string") {
        return {
          code: (parsed.code ?? "local") as AppError["code"],
          message: parsed.message,
          retryAfterSeconds: parsed.retryAfterSeconds ?? null,
        };
      }
    } catch {
      // not JSON
    }
    return { code: "local", message: err, retryAfterSeconds: null };
  }
  if (err instanceof Error) {
    return { code: "local", message: err.message, retryAfterSeconds: null };
  }
  return { code: "local", message: "Something went wrong.", retryAfterSeconds: null };
}

export { EVENTS };
