//! Spawn helpers that suppress the console-window flash on Windows.
//!
//! When a Windows GUI app (`windows_subsystem = "windows"`) spawns a
//! console-subsystem child like `powershell.exe`, `tar.exe`, or
//! `llama-server.exe`, the child opens its own console window unless
//! `CREATE_NO_WINDOW` is passed at creation time. That flash is what shows
//! up as "a terminal pops up when I prompt Bob" - the frontend triggers
//! TTS, TTS spawns powershell with the SAPI script, the brief console
//! flickers on screen.
//!
//! Use these helpers everywhere we'd otherwise call `Command::new(...)`.
//! On macOS they're a thin pass-through.
use std::ffi::OsStr;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// `tokio::process::Command::new` with `CREATE_NO_WINDOW` on Windows.
pub fn tokio_command<S: AsRef<OsStr>>(program: S) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(program);
    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// `std::process::Command::new` with `CREATE_NO_WINDOW` on Windows.
#[allow(dead_code)]
pub fn std_command<S: AsRef<OsStr>>(program: S) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}
