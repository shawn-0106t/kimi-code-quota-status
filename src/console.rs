// Windows 进程/控制台辅助：detached 刷新子进程派生（SPEC §4.4）+
// CONOUT$ 终端宽度查询（SPEC §7.5）。

use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Console::{CONSOLE_SCREEN_BUFFER_INFO, GetConsoleScreenBufferInfo};
use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, DETACHED_PROCESS};

/// 派生 detached --refresh 子进程（SPEC §4.4）：
/// `Command::new(<自身绝对路径>).arg("--refresh")` + CREATE_NO_WINDOW |
/// DETACHED_PROCESS + stdio 三路 DEVNULL；spawn 后立即返回，父进程
/// 不等待、不读取其任何输出。远早于宿主 300ms 超时退出，宿主的
/// `taskkill /T` 不波及 detached 子进程。
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

/// 宽度归一（SPEC §7.5）：查询失败或异常值（0）按 120 兜底。
pub fn normalize_width(raw: Option<u32>) -> u32 {
    raw.filter(|w| *w > 0).unwrap_or(120)
}

/// 终端宽度：stdout 虽为 pipe 但进程仍附着宿主 console —— 用
/// CreateFileW("CONOUT$") + GetConsoleScreenBufferInfo().dwSize.X 查询；
/// headless/查询失败按 120（经 normalize_width）。
pub fn console_width() -> u32 {
    let raw = query_console_width();
    normalize_width(raw)
}

fn query_console_width() -> Option<u32> {
    let conout: Vec<u16> = "CONOUT$".encode_utf16().chain(std::iter::once(0)).collect();
    let handle = unsafe {
        CreateFileW(
            conout.as_ptr(),
            // GENERIC_READ (0x8000_0000)：GetConsoleScreenBufferInfo 要求句柄
            // 带读权限；access=0 会 ERROR_ACCESS_DENIED，宽度查询恒失败
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
        // dwSize.X 为 i16：理论负值经 try_from 落 None -> 兜底 120
        u32::try_from(info.dwSize.X).ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::Console::GetConsoleProcessList;

    /// 宽度不可得分支（PLAN P3 单测清单）：None/0 -> 120 兜底，正常值透传。
    #[test]
    fn width_normalize_fallback() {
        assert_eq!(normalize_width(None), 120);
        assert_eq!(normalize_width(Some(0)), 120);
        assert_eq!(normalize_width(Some(80)), 80);
        assert_eq!(normalize_width(Some(u32::MAX)), u32::MAX);
    }

    /// 真实查询烟测：测试进程有 console 时返回正宽度，headless 落 120。
    /// 不假定最小宽度（80 列 console 同样合法）。
    #[test]
    fn console_width_sane() {
        let w = console_width();
        assert!(w > 0, "width = {w}");
    }

    /// CONOUT$ 句柄权限回归（access=0 的旧实现查询恒失败落 None）：进程
    /// 附着 console 时 query_console_width 必须返回 Some；headless（无
    /// console）跳过断言。
    #[test]
    fn query_width_succeeds_when_console_attached() {
        let attached = unsafe {
            let mut pid = 0u32;
            GetConsoleProcessList(&mut pid, 1) > 0
        };
        if attached {
            assert!(
                query_console_width().is_some(),
                "进程附着 console 但宽度查询失败（CONOUT$ 句柄权限问题？）"
            );
        }
    }
}
