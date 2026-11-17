//! Thin Win32 clipboard access for the Clipboard Vault.
//!
//! Kept separate from `clipboard.rs` so the platform calls are isolated from the
//! storage/index logic — the storage half is pure Rust and testable, this half is
//! `unsafe` and Windows-only.
//!
//! ## Why sequence-number polling
//!
//! `GetClipboardSequenceNumber` returns a counter that increments on every
//! clipboard change. Polling it is one `user32` call, needs no window
//! procedure, and is free when the clipboard is idle. A `WM_CLIPBOARDUPDATE`
//! listener would require a message-only window with correct lifetime management
//! across Tauri's event loop, which is a lot of machinery for a counter read.

#![cfg(windows)]

use crate::clipboard::ClipboardVault;
use crate::config::Settings;
use crate::state::AppState;
use std::ffi::c_void;

const CF_UNICODETEXT: u32 = 13;
const CF_DIB: u32 = 8;
const GMEM_MOVEABLE: u32 = 0x0002;

/// Current clipboard sequence number, or `None` if the clipboard is locked.
pub fn sequence_number() -> Option<u32> {
    let value = unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() };
    if value == 0 {
        None
    } else {
        Some(value)
    }
}

/// Reads the clipboard as Unicode text.
///
/// Returns `Ok(None)` when the clipboard holds no text, which is the normal case
/// for an image copy — not an error.
pub fn read_text() -> Option<String> {
    unsafe {
        if windows_sys::Win32::System::DataExchange::OpenClipboard(std::ptr::null_mut()) == 0 {
            return None;
        }

        let result = read_text_locked();
        windows_sys::Win32::System::DataExchange::CloseClipboard();
        result
    }
}

unsafe fn read_text_locked() -> Option<String> {
    let handle = windows_sys::Win32::System::DataExchange::GetClipboardData(CF_UNICODETEXT);
    if handle.is_null() {
        return None;
    }

    let pointer = windows_sys::Win32::System::Memory::GlobalLock(handle) as *const u16;
    if pointer.is_null() {
        return None;
    }

    let mut length = 0usize;
    while *pointer.add(length) != 0 {
        length += 1;
    }

    let slice = std::slice::from_raw_parts(pointer, length);
    let text = String::from_utf16_lossy(slice);

    windows_sys::Win32::System::Memory::GlobalUnlock(handle);
    Some(text)
}

/// Width and height of a CF_DIB payload, parsed from its BITMAPINFOHEADER.
///
/// `CF_DIB` is a packed `BITMAPINFOHEADER` followed by the pixel array, rows
/// bottom-up for positive heights.
struct DibHeader {
    width: i32,
    height: i32,
    bit_count: u16,
    compression: u32,
}

unsafe fn read_dib_header(handle: *mut c_void) -> Option<DibHeader> {
    let pointer = windows_sys::Win32::System::Memory::GlobalLock(handle) as *const u8;
    if pointer.is_null() {
        return None;
    }

    // BITMAPINFOHEADER: biSize(4) biWidth(4) biHeight(4) biPlanes(2) biBitCount(2)
    // biCompression(4) ...
    let header = DibHeader {
        width: i32::from_ne_bytes([
            *pointer.add(4),
            *pointer.add(5),
            *pointer.add(6),
            *pointer.add(7),
        ]),
        height: i32::from_ne_bytes([
            *pointer.add(8),
            *pointer.add(9),
            *pointer.add(10),
            *pointer.add(11),
        ]),
        bit_count: u16::from_ne_bytes([*pointer.add(12), *pointer.add(13)]),
        compression: u32::from_ne_bytes([
            *pointer.add(16),
            *pointer.add(17),
            *pointer.add(18),
            *pointer.add(19),
        ]),
    };

    windows_sys::Win32::System::Memory::GlobalUnlock(handle);

    if header.width <= 0 || header.height == 0 || header.bit_count != 32 {
        return None;
    }
    // BI_RGB only: BI_BITFIELDS and RLE would need per-format decoding.
    if header.compression != 0 {
        return None;
    }
    Some(header)
}

/// Reads the clipboard as an RGBA image.
///
/// Only 32-bit BI_RGB DIBs are handled. That covers screenshots from Snipping
/// Tool, browser image copies, and every Chromium/Firefox "copy image", which is
/// the realistic set; anything else is skipped rather than guessed at.
pub fn read_image_rgba() -> Option<(Vec<u8>, u32, u32)> {
    unsafe {
        if windows_sys::Win32::System::DataExchange::OpenClipboard(std::ptr::null_mut()) == 0 {
            return None;
        }

        let result = read_image_locked();
        windows_sys::Win32::System::DataExchange::CloseClipboard();
        result
    }
}

unsafe fn read_image_locked() -> Option<(Vec<u8>, u32, u32)> {
    let handle = windows_sys::Win32::System::DataExchange::GetClipboardData(CF_DIB);
    if handle.is_null() {
        return None;
    }

    let header = read_dib_header(handle)?;

    let width = header.width as u32;
    // Negative height = top-down rows; positive = bottom-up.
    let top_down = header.height < 0;
    let height = header.height.unsigned_abs();

    if width > 4096 || height > 4096 {
        return None;
    }

    let pointer = windows_sys::Win32::System::Memory::GlobalLock(handle) as *const u8;
    if pointer.is_null() {
        return None;
    }

    let pixel_count = width as usize * height as usize;
    let mut bgra = vec![0u8; pixel_count * 4];
    std::ptr::copy_nonoverlapping(pointer.add(40), bgra.as_mut_ptr(), bgra.len());
    windows_sys::Win32::System::Memory::GlobalUnlock(handle);

    // BGRA -> RGBA.
    let mut rgba = Vec::with_capacity(bgra.len());
    for pixel in bgra.chunks_exact(4) {
        rgba.push(pixel[2]);
        rgba.push(pixel[1]);
        rgba.push(pixel[0]);
        rgba.push(pixel[3]);
    }

    // Bottom-up DIBs arrive flipped; normalise to top-down so the PNG is upright.
    if !top_down {
        let stride = width as usize * 4;
        let mut flipped = vec![0u8; rgba.len()];
        for row in 0..height as usize {
            let source = (height as usize - 1 - row) * stride;
            flipped[row * stride..(row + 1) * stride]
                .copy_from_slice(&rgba[source..source + stride]);
        }
        rgba = flipped;
    }

    Some((rgba, width, height))
}

/// Name of the process that currently owns the foreground window.
///
/// Used only for the user's exclusion list. Returns an empty string when the
/// process cannot be read (elevated window, protected process), which is treated
/// as "not excluded" — a failure to identify an app must not silently drop
/// clipboard content the user asked to keep.
pub fn foreground_process_name() -> String {
    unsafe {
        let hwnd = windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow();
        if hwnd.is_null() {
            return String::new();
        }

        let mut pid: u32 = 0;
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return String::new();
        }

        // PROCESS_QUERY_LIMITED_INFORMATION is enough for QueryFullProcessImageName
        // and works across integrity levels without elevation.
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
        let process = windows_sys::Win32::System::Threading::OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            pid,
        );
        if process.is_null() {
            return String::new();
        }

        let mut buffer = [0u16; 512];
        let mut size = buffer.len() as u32;
        let ok = windows_sys::Win32::System::Threading::QueryFullProcessImageNameW(
            process,
            0,
            buffer.as_mut_ptr(),
            &mut size,
        );
        windows_sys::Win32::Foundation::CloseHandle(process);

        if ok == 0 {
            return String::new();
        }

        let full = String::from_utf16_lossy(&buffer[..size as usize]);
        full.rsplit(['\\', '/']).next().unwrap_or("").to_string()
    }
}

/// Copies text onto the real Windows clipboard.
pub fn set_text(text: &str) -> crate::error::AppResult<()> {
    unsafe {
        // UTF-16 plus a NUL terminator.
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        wide.push(0);
        let bytes = wide.len() * std::mem::size_of::<u16>();

        let memory = windows_sys::Win32::System::Memory::GlobalAlloc(GMEM_MOVEABLE, bytes);
        if memory.is_null() {
            return Err(crate::error::AppError::Local(
                "could not allocate clipboard memory".into(),
            ));
        }

        let destination = windows_sys::Win32::System::Memory::GlobalLock(memory) as *mut u16;
        if destination.is_null() {
            windows_sys::Win32::Foundation::CloseHandle(memory);
            return Err(crate::error::AppError::Local(
                "could not lock clipboard memory".into(),
            ));
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), destination, wide.len());
        windows_sys::Win32::System::Memory::GlobalUnlock(memory);

        if windows_sys::Win32::System::DataExchange::OpenClipboard(std::ptr::null_mut()) == 0 {
            windows_sys::Win32::Foundation::CloseHandle(memory);
            return Err(crate::error::AppError::Local(
                "another app is holding the clipboard".into(),
            ));
        }

        windows_sys::Win32::System::DataExchange::EmptyClipboard();
        let result =
            windows_sys::Win32::System::DataExchange::SetClipboardData(CF_UNICODETEXT, memory);
        windows_sys::Win32::System::DataExchange::CloseClipboard();

        if result.is_null() {
            windows_sys::Win32::Foundation::CloseHandle(memory);
            return Err(crate::error::AppError::Local(
                "Windows refused the clipboard update".into(),
            ));
        }
        Ok(())
    }
}

/// Puts a stored PNG back on the clipboard as a DIB.
///
/// Re-encoding to DIB rather than passing the PNG through keeps the paste target
/// working in applications that only understand CF_DIB (older Office, some Java
/// apps). The reverse conversion happens through the `image` crate, which is
/// already a dependency.
pub fn set_image_from_png(path: &std::path::Path) -> crate::error::AppResult<()> {
    let rgba = image::open(path)
        .map_err(|e| {
            crate::error::AppError::Local(format!("could not read the stored image: {e}"))
        })?
        .to_rgba8();

    let (width, height) = rgba.dimensions();
    let width_i = width as i32;
    let height_i = height as i32;
    let row_bytes = width as usize * 4;
    let image_bytes = row_bytes * height as usize;
    // BITMAPINFOHEADER (40 bytes) followed by a BITMAPFILEHEADER placeholder is
    // not needed for CF_DIB: the clipboard format is the info header + pixels.
    let total = 40 + image_bytes;

    unsafe {
        let memory = windows_sys::Win32::System::Memory::GlobalAlloc(GMEM_MOVEABLE, total);
        if memory.is_null() {
            return Err(crate::error::AppError::Local(
                "could not allocate clipboard memory".into(),
            ));
        }

        let destination = windows_sys::Win32::System::Memory::GlobalLock(memory) as *mut u8;
        if destination.is_null() {
            windows_sys::Win32::Foundation::CloseHandle(memory);
            return Err(crate::error::AppError::Local(
                "could not lock clipboard memory".into(),
            ));
        }

        let mut header = [0u8; 40];
        header[0..4].copy_from_slice(&40u32.to_ne_bytes()); // biSize
        header[4..8].copy_from_slice(&width_i.to_ne_bytes());
        // Positive height = bottom-up rows, which is the DIB convention.
        header[8..12].copy_from_slice(&height_i.to_ne_bytes());
        header[12..14].copy_from_slice(&1u16.to_ne_bytes()); // biPlanes
        header[14..16].copy_from_slice(&32u16.to_ne_bytes()); // biBitCount
        header[16..20].copy_from_slice(&0u32.to_ne_bytes()); // BI_RGB
        header[20..24].copy_from_slice(&(image_bytes as u32).to_ne_bytes());

        std::ptr::copy_nonoverlapping(header.as_ptr(), destination, 40);

        // BGRA, bottom-up.
        let pixels = rgba.as_raw();
        let height = height as usize;
        let width = width as usize;
        for row in 0..height {
            let source_row = height - 1 - row;
            for column in 0..width {
                let source = (source_row * width + column) * 4;
                let target = 40 + (row * width + column) * 4;
                *destination.add(target) = pixels[source + 2];
                *destination.add(target + 1) = pixels[source + 1];
                *destination.add(target + 2) = pixels[source];
                *destination.add(target + 3) = pixels[source + 3];
            }
        }

        windows_sys::Win32::System::Memory::GlobalUnlock(memory);

        if windows_sys::Win32::System::DataExchange::OpenClipboard(std::ptr::null_mut()) == 0 {
            windows_sys::Win32::Foundation::CloseHandle(memory);
            return Err(crate::error::AppError::Local(
                "another app is holding the clipboard".into(),
            ));
        }

        windows_sys::Win32::System::DataExchange::EmptyClipboard();
        let result = windows_sys::Win32::System::DataExchange::SetClipboardData(CF_DIB, memory);
        windows_sys::Win32::System::DataExchange::CloseClipboard();

        if result.is_null() {
            windows_sys::Win32::Foundation::CloseHandle(memory);
            return Err(crate::error::AppError::Local(
                "Windows refused the clipboard image".into(),
            ));
        }
        Ok(())
    }
}

/// One capture pass: reads the clipboard if it changed and stores the result.
///
/// Called from the background watcher. The settings gate lives here rather than in
/// the caller so there is exactly one place where capture can be suppressed.
pub fn capture_once(state: &AppState) -> Option<crate::clipboard::ClipboardEntry> {
    let settings: Settings = state.settings();
    if !settings.clipboard_enabled || settings.clipboard_private {
        return None;
    }

    // Exclusion list: checked before reading anything, so a password manager's
    // clipboard contents never enter Sonic's memory.
    let process = foreground_process_name();
    if !process.is_empty() {
        let name = process.to_ascii_lowercase();
        let stem = name.strip_suffix(".exe").unwrap_or(&name);
        if settings
            .clipboard_excluded_apps
            .iter()
            .any(|excluded| excluded.trim().eq_ignore_ascii_case(stem))
        {
            return None;
        }
    }

    let mut vault = ClipboardVault::load();

    // Text first: it is the common case and the cheaper read.
    if let Some(text) = read_text() {
        if text.trim().is_empty() {
            return None;
        }
        if crate::clipboard::looks_sensitive(&text) {
            return None;
        }

        let (stored, _truncated) = crate::clipboard::clamp_text(&text);
        let entry = vault
            .push(crate::clipboard::ClipboardKind::Text, Some(stored), None)
            .ok()?;
        let id = entry.id.clone();
        vault.set_source(&id, process);
        let _ = vault.save_for_capture();
        state.set_clipboard_dirty(true);
        return Some(entry);
    }

    // Image second.
    if let Some((rgba, width, height)) = read_image_rgba() {
        let stored = vault.store_image(&rgba, width, height).ok()?;
        let entry = vault
            .push(crate::clipboard::ClipboardKind::Image, None, Some(stored))
            .ok()?;
        let id = entry.id.clone();
        vault.set_source(&id, process);
        let _ = vault.save_for_capture();
        state.set_clipboard_dirty(true);
        return Some(entry);
    }

    None
}

/// Swaps a stored image back onto the clipboard.
pub fn restore_entry(id: &str) -> crate::error::AppResult<()> {
    let vault = ClipboardVault::load();
    vault.restore_to_clipboard(id)
}
