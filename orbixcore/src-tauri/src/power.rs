// ─────────────────────────────────────────────
//  Power - keep the machine awake while Orbix runs
//
//  Orbix hosts standing agents and scheduled workflows; a Mac that
//  idle-sleeps takes them all down with it. A single IOKit power
//  assertion (the same mechanism `caffeinate -i` uses) held for the
//  process lifetime prevents idle SYSTEM sleep while still letting the
//  display sleep, so the battery cost is minimal. The kernel releases
//  the assertion automatically when the process exits - crash included -
//  so there is nothing to clean up. Closing the lid still sleeps the
//  machine; the user's explicit intent always wins.
// ─────────────────────────────────────────────

#[cfg(target_os = "macos")]
pub fn keep_awake() {
    use core_foundation::base::TCFType;
    use core_foundation::string::{CFString, CFStringRef};

    type IOPMAssertionID = u32;
    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOPMAssertionCreateWithName(
            assertion_type: CFStringRef,
            assertion_level: u32,
            assertion_name: CFStringRef,
            assertion_id: *mut IOPMAssertionID,
        ) -> i32;
    }
    const LEVEL_ON: u32 = 255;

    let assertion_type = CFString::new("PreventUserIdleSystemSleep");
    let reason = CFString::new("Orbix Core keeps agents and workflows running");
    let mut id: IOPMAssertionID = 0;
    let ret = unsafe {
        IOPMAssertionCreateWithName(
            assertion_type.as_concrete_TypeRef(),
            LEVEL_ON,
            reason.as_concrete_TypeRef(),
            &mut id,
        )
    };
    if ret == 0 {
        eprintln!("[power] sleep assertion active (id {}) - system stays awake while Orbix runs", id);
    } else {
        eprintln!("[power] sleep assertion failed (IOReturn {:#x}) - system may idle-sleep", ret);
    }
}

/// Windows analogue: SetThreadExecutionState with ES_CONTINUOUS holds the
/// same "no idle system sleep, display may sleep" contract for the process
/// lifetime, and the kernel clears it automatically on exit - crash
/// included. Closing the lid still sleeps the machine.
#[cfg(target_os = "windows")]
pub fn keep_awake() {
    use windows::Win32::System::Power::{
        SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED,
    };
    let prev = unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) };
    if prev.0 != 0 {
        eprintln!("[power] execution state held - system stays awake while Orbix runs");
    } else {
        eprintln!("[power] SetThreadExecutionState failed - system may idle-sleep");
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn keep_awake() {}
