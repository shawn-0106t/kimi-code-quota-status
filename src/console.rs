// Windows process/console helpers: detached refresh child process spawning (SPEC §4.4) +
// CONOUT$ terminal width query (SPEC §7.5).

use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Console::{CONSOLE_SCREEN_BUFFER_INFO, GetConsoleScreenBufferInfo};
use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, DETACHED_PROCESS};

/// Spawns a detached --refresh child process (SPEC §4.4):
/// `Command::new(<own absolute path>).arg("--refresh")` + CREATE_NO_WINDOW |
/// DETACHED_PROCESS + stdio DEVNULL on all three; returns immediately after spawn, the parent
/// does not wait and does not read any of its output. It exits far earlier than the host's
/// 300ms timeout, and the host's `taskkill /T` does not reach detached child processes.
pub fn spawn_detached_refresh() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    Command::new(exe)
        .arg("--refresh")
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

/// Width normalization (SPEC §7.5): query failure or abnormal value (0) falls back to 120.
pub fn normalize_width(raw: Option<u32>) -> u32 {
    raw.filter(|w| *w > 0).unwrap_or(120)
}

/// Terminal width: although stdout is a pipe, the process is still attached to the host console --
/// query via CreateFileW("CONOUT$") + GetConsoleScreenBufferInfo().dwSize.X;
/// headless/query failure falls back to 120 (via normalize_width).
pub fn console_width() -> u32 {
    let raw = query_console_width();
    normalize_width(raw)
}

fn query_console_width() -> Option<u32> {
    let conout: Vec<u16> = "CONOUT$".encode_utf16().chain(std::iter::once(0)).collect();
    let handle = unsafe {
        CreateFileW(
            conout.as_ptr(),
            // GENERIC_READ (0x8000_0000): GetConsoleScreenBufferInfo requires the handle to
            // carry read access; access=0 yields ERROR_ACCESS_DENIED and width query always fails
            0x8000_0000,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return None;
    }
    let mut info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetConsoleScreenBufferInfo(handle, &mut info) };
    unsafe { CloseHandle(handle) };
    if ok != 0 {
        // dwSize.X is i16: a theoretical negative value goes through try_from to None -> fallback 120
        u32::try_from(info.dwSize.X).ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::Console::GetConsoleProcessList;

    /// Width-unavailable branch (PLAN P3 unit test list): None/0 -> 120 fallback, normal values pass through.
    #[test]
    fn width_normalize_fallback() {
        assert_eq!(normalize_width(None), 120);
        assert_eq!(normalize_width(Some(0)), 120);
        assert_eq!(normalize_width(Some(80)), 80);
        assert_eq!(normalize_width(Some(u32::MAX)), u32::MAX);
    }

    /// Real query smoke test: returns a positive width when the test process has a console, 120 when headless.
    /// Does not assume a minimum width (an 80-column console is equally valid).
    #[test]
    fn console_width_sane() {
        let w = console_width();
        assert!(w > 0, "width = {w}");
    }

    /// CONOUT$ handle permission regression (the old access=0 implementation always failed the
    /// query, returning None): when the process is attached to a console, query_console_width
    /// must return Some; headless (no console) skips the assertion.
    #[test]
    fn query_width_succeeds_when_console_attached() {
        let attached = unsafe {
            let mut pid = 0u32;
            GetConsoleProcessList(&mut pid, 1) > 0
        };
        if attached {
            assert!(
                query_console_width().is_some(),
                "process attached to console but width query failed (CONOUT$ handle permission issue?)"
            );
        }
    }
}
