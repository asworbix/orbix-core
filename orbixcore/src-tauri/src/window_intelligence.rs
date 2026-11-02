use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

// ── macOS Accessibility API FFI ──

#[cfg(target_os = "macos")]
mod ax {
    use core_foundation::base::{CFTypeRef, TCFType};
    use core_foundation::string::{CFString, CFStringRef};
    use std::ptr;

    type CFArrayRef = *const std::ffi::c_void;
    type CFIndex = isize;
    type CFTypeID = u64;

    extern "C" {
        fn CFArrayGetCount(arr: CFArrayRef) -> CFIndex;
        fn CFArrayGetValueAtIndex(arr: CFArrayRef, idx: CFIndex) -> *const std::ffi::c_void;
        fn CFRelease(cf: *const std::ffi::c_void);
        fn CFGetTypeID(cf: *const std::ffi::c_void) -> CFTypeID;
        fn CFStringGetTypeID() -> CFTypeID;
        fn CFArrayGetTypeID() -> CFTypeID;
        fn CFBooleanGetTypeID() -> CFTypeID;
    }

    fn is_cf_string(ptr: *const std::ffi::c_void) -> bool {
        if ptr.is_null() { return false; }
        unsafe { CFGetTypeID(ptr) == CFStringGetTypeID() }
    }

    fn is_cf_array(ptr: *const std::ffi::c_void) -> bool {
        if ptr.is_null() { return false; }
        unsafe { CFGetTypeID(ptr) == CFArrayGetTypeID() }
    }

    pub type AXUIElementRef = CFTypeRef;
    pub type AXError = i32;
    pub const AX_ERROR_SUCCESS: AXError = 0;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        pub fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
        pub fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        pub fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> AXError;
        pub fn AXUIElementCopyAttributeNames(
            element: AXUIElementRef,
            names: *mut CFTypeRef,
        ) -> AXError;
        pub fn AXUIElementSetAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: CFTypeRef,
        ) -> AXError;
        pub fn AXUIElementPerformAction(
            element: AXUIElementRef,
            action: CFStringRef,
        ) -> AXError;
        pub fn AXIsProcessTrusted() -> u8;
    }

    // AXIsProcessTrustedWithOptions - prompts the user if not trusted
    extern "C" {
        fn AXIsProcessTrustedWithOptions(options: CFTypeRef) -> u8;
    }

    pub fn is_trusted() -> bool {
        unsafe { AXIsProcessTrusted() != 0 }
    }

    pub fn request_access() -> bool {
        unsafe {
            // Pass kAXTrustedCheckOptionPrompt = true to show the system prompt
            
            use core_foundation::boolean::CFBoolean;

            let key = CFString::new("AXTrustedCheckOptionPrompt");
            let val = CFBoolean::true_value();

            let _keys = [key.as_CFTypeRef()];
            let _vals = [val.as_CFTypeRef()];

            let dict = core_foundation::dictionary::CFDictionary::from_CFType_pairs(&[
                (key.as_CFType(), val.as_CFType())
            ]);

            AXIsProcessTrustedWithOptions(dict.as_CFTypeRef()) != 0
        }
    }

    // Try an actual AX call to verify access works, regardless of what AXIsProcessTrusted says
    pub fn test_access() -> bool {
        unsafe {
            // Try to get the system-wide element and read an attribute
            let sys = AXUIElementCreateSystemWide();
            if sys.is_null() { return false; }
            // Try reading the focused application
            let cf_attr = CFString::new("AXFocusedApplication");
            let mut value: CFTypeRef = std::ptr::null();
            let err = AXUIElementCopyAttributeValue(
                sys,
                cf_attr.as_concrete_TypeRef(),
                &mut value,
            );
            err == AX_ERROR_SUCCESS && !value.is_null()
        }
    }

    pub fn get_app_element(pid: i32) -> Option<AXUIElementRef> {
        let el = unsafe { AXUIElementCreateApplication(pid) };
        if el.is_null() { return None; }
        // AXUIElementCreateApplication always returns a ref even for dead PIDs,
        // so verify we can actually read from it
        let role = get_attribute_string(el, "AXRole");
        if role.is_none() {
            // Try AXTitle as fallback - some apps don't set AXRole on the app element
            let title = get_attribute_string(el, "AXTitle");
            if title.is_none() {
                return None;
            }
        }
        Some(el)
    }

    pub fn get_attribute_string(element: AXUIElementRef, attr: &str) -> Option<String> {
        unsafe {
            if element.is_null() { return None; }
            let cf_attr = CFString::new(attr);
            let mut value: CFTypeRef = ptr::null();
            let err = AXUIElementCopyAttributeValue(
                element,
                cf_attr.as_concrete_TypeRef(),
                &mut value,
            );
            if err != AX_ERROR_SUCCESS || value.is_null() {
                return None;
            }
            // CRITICAL: verify the returned value is actually a CFString before casting
            if !is_cf_string(value) {
                CFRelease(value);
                return None;
            }
            let cf_str = CFString::wrap_under_create_rule(value as CFStringRef);
            Some(cf_str.to_string())
        }
    }

    pub fn get_attribute_value(element: AXUIElementRef, attr: &str) -> Option<CFTypeRef> {
        unsafe {
            if element.is_null() { return None; }
            let cf_attr = CFString::new(attr);
            let mut value: CFTypeRef = ptr::null();
            let err = AXUIElementCopyAttributeValue(
                element,
                cf_attr.as_concrete_TypeRef(),
                &mut value,
            );
            if err != AX_ERROR_SUCCESS || value.is_null() {
                None
            } else {
                Some(value)
            }
        }
    }

    pub fn get_attribute_names(element: AXUIElementRef) -> Vec<String> {
        unsafe {
            if element.is_null() { return vec![]; }
            let mut names_ref: CFTypeRef = ptr::null();
            let err = AXUIElementCopyAttributeNames(element, &mut names_ref);
            if err != AX_ERROR_SUCCESS || names_ref.is_null() {
                return vec![];
            }
            if !is_cf_array(names_ref) {
                CFRelease(names_ref);
                return vec![];
            }
            let arr = names_ref as CFArrayRef;
            let count = CFArrayGetCount(arr);
            if count < 0 || count > 10000 {
                CFRelease(names_ref);
                return vec![];
            }
            let mut result = Vec::new();
            for i in 0..count {
                let item_ptr = CFArrayGetValueAtIndex(arr, i);
                if !item_ptr.is_null() && is_cf_string(item_ptr) {
                    let cf_str = CFString::wrap_under_get_rule(item_ptr as CFStringRef);
                    result.push(cf_str.to_string());
                }
            }
            CFRelease(names_ref);
            result
        }
    }

    /// Returns (array_ref, children) - caller MUST keep array_ref alive while using children.
    /// Call release() on array_ref when done.
    /// Elements of a CFArray attribute value already fetched (AXWindows,
    /// AXChildren). The caller releases the returned owner.
    pub fn get_children_with_owner_of_list(list: CFTypeRef) -> (*const std::ffi::c_void, Vec<AXUIElementRef>) {
        if list.is_null() || !is_cf_array(list) {
            if !list.is_null() { unsafe { CFRelease(list) }; }
            return (ptr::null(), Vec::new());
        }
        let n = unsafe { CFArrayGetCount(list) };
        let mut out = Vec::with_capacity(n.max(0) as usize);
        for i in 0..n {
            let item = unsafe { CFArrayGetValueAtIndex(list, i) };
            if !item.is_null() { out.push(item as AXUIElementRef); }
        }
        (list, out)
    }

    pub fn get_children_with_owner(element: AXUIElementRef) -> (*const std::ffi::c_void, Vec<AXUIElementRef>) {
        unsafe {
            if element.is_null() { return (ptr::null(), vec![]); }
            let cf_attr = CFString::new("AXChildren");
            let mut value: CFTypeRef = ptr::null();
            let err = AXUIElementCopyAttributeValue(
                element,
                cf_attr.as_concrete_TypeRef(),
                &mut value,
            );
            if err != AX_ERROR_SUCCESS || value.is_null() {
                return (ptr::null(), vec![]);
            }
            // Verify it's actually a CFArray before treating it as one
            if !is_cf_array(value) {
                CFRelease(value);
                return (ptr::null(), vec![]);
            }
            let arr = value as CFArrayRef;
            let count = CFArrayGetCount(arr);
            // Sanity check: negative or absurdly large counts mean corrupt data
            if count < 0 || count > 10000 {
                CFRelease(value);
                return (ptr::null(), vec![]);
            }
            // Cap at 200 children per node to prevent runaway on huge apps
            let capped = if count > 200 { 200 } else { count };
            let mut result = Vec::new();
            for i in 0..capped {
                let child = CFArrayGetValueAtIndex(arr, i);
                if !child.is_null() {
                    result.push(child as AXUIElementRef);
                }
            }
            // Return array ref so caller keeps it alive
            (value, result)
        }
    }

    pub unsafe fn release(ptr: *const std::ffi::c_void) {
        if !ptr.is_null() {
            CFRelease(ptr);
        }
    }

    pub fn set_attribute_string(element: AXUIElementRef, attr: &str, val: &str) -> bool {
        unsafe {
            let cf_attr = CFString::new(attr);
            let cf_val = CFString::new(val);
            let err = AXUIElementSetAttributeValue(
                element,
                cf_attr.as_concrete_TypeRef(),
                cf_val.as_CFTypeRef(),
            );
            err == AX_ERROR_SUCCESS
        }
    }

    /// Read a boolean attribute (AXFocused, AXEnabled). None when unset.
    pub fn get_attribute_bool(element: AXUIElementRef, attr: &str) -> Option<bool> {
        use core_foundation::boolean::{CFBoolean, CFBooleanRef};
        let v = get_attribute_value(element, attr)?;
        unsafe {
            if CFGetTypeID(v) != CFBooleanGetTypeID() {
                CFRelease(v);
                return None;
            }
            let b = CFBoolean::wrap_under_create_rule(v as CFBooleanRef);
            Some(bool::from(b))
        }
    }

    /// Set a boolean attribute (AXFocused). Best effort.
    pub fn set_attribute_bool(element: AXUIElementRef, attr: &str, val: bool) -> bool {
        use core_foundation::boolean::CFBoolean;
        unsafe {
            if element.is_null() { return false; }
            let cf_attr = CFString::new(attr);
            let cf_val = if val { CFBoolean::true_value() } else { CFBoolean::false_value() };
            AXUIElementSetAttributeValue(element, cf_attr.as_concrete_TypeRef(), cf_val.as_CFTypeRef()) == AX_ERROR_SUCCESS
        }
    }

    pub fn perform_action(element: AXUIElementRef, action: &str) -> bool {
        unsafe {
            let cf_action = CFString::new(action);
            let err = AXUIElementPerformAction(element, cf_action.as_concrete_TypeRef());
            err == AX_ERROR_SUCCESS
        }
    }
}

// ── Windows UI Automation FFI ──
// Mirrors the surface of the `ax` module above so the public-facing methods
// (profile_window_direct, walk_*_tree_shallow, find_field_value, etc.) can be
// implemented with the same shape on both platforms. The underlying technology
// is different - Windows uses COM-based UIAutomation - but the public API
// behaves the same: walk a tree, classify elements, read/write values, invoke.
#[cfg(target_os = "windows")]
pub mod uia {
    use std::sync::OnceLock;
    use windows::core::{BSTR, VARIANT};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationInvokePattern,
        IUIAutomationValuePattern, TreeScope_Children, TreeScope_Descendants,
        UIA_AutomationIdPropertyId, UIA_ButtonControlTypeId, UIA_CheckBoxControlTypeId,
        UIA_ComboBoxControlTypeId, UIA_ControlTypePropertyId, UIA_DataGridControlTypeId,
        UIA_DocumentControlTypeId, UIA_EditControlTypeId, UIA_HelpTextPropertyId,
        UIA_HyperlinkControlTypeId, UIA_ImageControlTypeId, UIA_InvokePatternId,
        UIA_IsEnabledPropertyId, UIA_ListControlTypeId, UIA_MenuBarControlTypeId,
        UIA_MenuItemControlTypeId, UIA_NamePropertyId, UIA_PaneControlTypeId,
        UIA_ProcessIdPropertyId, UIA_SliderControlTypeId, UIA_TabControlTypeId,
        UIA_TabItemControlTypeId, UIA_TableControlTypeId, UIA_TextControlTypeId, UIA_TitleBarControlTypeId,
        UIA_ToolBarControlTypeId, UIA_TreeControlTypeId, UIA_ValuePatternId,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetForegroundWindow, GetWindowThreadProcessId, IsWindowVisible,
    };

    // We initialise COM once per process. UIA returns the same automation
    // object shape from any thread once COM is up.
    static AUTOMATION: OnceLock<SendIUIAutomation> = OnceLock::new();

    // SAFETY: IUIAutomation is COM apartment-threaded but free-threaded callers
    // receive a marshalled proxy. For our use case (synchronous reads on the
    // calling thread after CoInitializeEx with MULTITHREADED), it's safe to
    // share. We wrap to satisfy Send/Sync.
    struct SendIUIAutomation(IUIAutomation);
    unsafe impl Send for SendIUIAutomation {}
    unsafe impl Sync for SendIUIAutomation {}

    fn ensure_com() -> Result<(), String> {
        unsafe {
            // CoInitializeEx in this windows-crate version returns HRESULT
            // directly. S_OK (0) means initialised here, S_FALSE (1) means
            // already up on this thread, RPC_E_CHANGED_MODE (0x80010106) means
            // COM is initialised in a different apartment mode - also fine.
            let hr = CoInitializeEx(Some(std::ptr::null()), COINIT_MULTITHREADED);
            let code = hr.0 as u32;
            if code == 0 || code == 1 || code == 0x80010106 {
                Ok(())
            } else {
                Err(format!("CoInitializeEx failed: 0x{:08x}", code))
            }
        }
    }

    fn automation() -> Result<&'static IUIAutomation, String> {
        // Initialise COM on whatever thread is calling - required before any
        // IUIAutomation method, even for objects created on another thread.
        ensure_com()?;
        if let Some(a) = AUTOMATION.get() {
            return Ok(&a.0);
        }
        let auto: IUIAutomation = unsafe {
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("CoCreateInstance(CUIAutomation) failed: {}", e))?
        };
        // Race between threads is fine - both produce equivalent objects;
        // OnceLock keeps whichever lands first.
        let _ = AUTOMATION.set(SendIUIAutomation(auto));
        Ok(&AUTOMATION.get().unwrap().0)
    }

    pub fn is_trusted() -> bool {
        // UI Automation has no separate accessibility-permission gate the way
        // macOS does. Same-session, same-user code can read the UIA tree of
        // any visible window without prior approval.
        ensure_com().is_ok() && automation().is_ok()
    }

    pub fn test_access() -> bool {
        match automation() {
            Ok(a) => unsafe { a.GetRootElement().is_ok() },
            Err(_) => false,
        }
    }

    /// Returns the top-level UIA element for the first visible window owned by
    /// the given process, or None if the process owns no visible window.
    pub fn get_app_element(pid: u32) -> Option<IUIAutomationElement> {
        let auto = automation().ok()?;

        // Enumerate top-level windows, filter to those owned by `pid`.
        let mut state = EnumState { pid, hwnd: HWND(std::ptr::null_mut()) };
        unsafe {
            let _ = EnumWindows(
                Some(enum_windows_proc),
                windows::Win32::Foundation::LPARAM(&mut state as *mut _ as isize),
            );
        }
        if state.hwnd.0.is_null() {
            return None;
        }
        unsafe { auto.ElementFromHandle(state.hwnd).ok() }
    }

    struct EnumState {
        pid: u32,
        hwnd: HWND,
    }

    // WNDENUMPROC requires unsafe extern "system" fn - the unsafety here is
    // that we deref a raw pointer that the Win32 EnumWindows callback ABI
    // hands us back from `lparam`.
    unsafe extern "system" fn enum_windows_proc(
        hwnd: HWND,
        lparam: windows::Win32::Foundation::LPARAM,
    ) -> windows::Win32::Foundation::BOOL {
        let state = &mut *(lparam.0 as *mut EnumState);
        if !IsWindowVisible(hwnd).as_bool() {
            return true.into();
        }
        let mut owner_pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut owner_pid));
        if owner_pid == state.pid {
            state.hwnd = hwnd;
            return false.into(); // stop enumeration
        }
        true.into()
    }

    pub fn get_focused_window() -> Option<IUIAutomationElement> {
        let auto = automation().ok()?;
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return None;
            }
            auto.ElementFromHandle(hwnd).ok()
        }
    }

    pub fn get_name(el: &IUIAutomationElement) -> String {
        unsafe { el.CurrentName().map(bstr_to_string).unwrap_or_default() }
    }

    pub fn get_help_text(el: &IUIAutomationElement) -> Option<String> {
        unsafe {
            el.CurrentHelpText().ok().map(bstr_to_string).filter(|s| !s.is_empty())
        }
    }

    pub fn get_automation_id(el: &IUIAutomationElement) -> Option<String> {
        unsafe {
            el.CurrentAutomationId().ok()
                .map(bstr_to_string)
                .filter(|s| !s.is_empty())
        }
    }

    pub fn get_control_type(el: &IUIAutomationElement) -> i32 {
        unsafe { el.CurrentControlType().map(|c| c.0).unwrap_or(0) }
    }

    pub fn get_value(el: &IUIAutomationElement) -> Option<String> {
        unsafe {
            // Query for ValuePattern; if unsupported, fall back to Name (some
            // controls - e.g. CheckBoxes - expose their state via Name).
            let pat: Result<IUIAutomationValuePattern, _> = el
                .GetCurrentPatternAs(UIA_ValuePatternId);
            if let Ok(vp) = pat {
                if let Ok(b) = vp.CurrentValue() {
                    let s = bstr_to_string(b);
                    if !s.is_empty() {
                        return Some(s);
                    }
                }
            }
            None
        }
    }

    pub fn is_enabled(el: &IUIAutomationElement) -> bool {
        unsafe { el.CurrentIsEnabled().map(|v| v.as_bool()).unwrap_or(true) }
    }

    pub fn get_children(el: &IUIAutomationElement) -> Vec<IUIAutomationElement> {
        let auto = match automation() {
            Ok(a) => a,
            Err(_) => return Vec::new(),
        };
        unsafe {
            // TrueCondition matches every element - combined with TreeScope_Children
            // this yields direct children only (no descendants), mirroring the
            // macOS AXChildren semantics.
            let cond = match auto.CreateTrueCondition() {
                Ok(c) => c,
                Err(_) => return Vec::new(),
            };
            let arr = match el.FindAll(TreeScope_Children, &cond) {
                Ok(a) => a,
                Err(_) => return Vec::new(),
            };
            let len = arr.Length().unwrap_or(0);
            let cap = if len > 200 { 200 } else { len };
            let mut out = Vec::with_capacity(cap as usize);
            for i in 0..cap {
                if let Ok(child) = arr.GetElement(i) {
                    out.push(child);
                }
            }
            out
        }
    }

    pub fn set_value(el: &IUIAutomationElement, value: &str) -> bool {
        unsafe {
            let pat: Result<IUIAutomationValuePattern, _> = el
                .GetCurrentPatternAs(UIA_ValuePatternId);
            match pat {
                Ok(vp) => {
                    let bstr = BSTR::from(value);
                    vp.SetValue(&bstr).is_ok()
                }
                Err(_) => false,
            }
        }
    }

    pub fn invoke(el: &IUIAutomationElement) -> bool {
        unsafe {
            let pat: Result<IUIAutomationInvokePattern, _> = el
                .GetCurrentPatternAs(UIA_InvokePatternId);
            match pat {
                Ok(ip) => ip.Invoke().is_ok(),
                Err(_) => false,
            }
        }
    }

    /// Titles of every TabItem in an app's UIA tree - how Chromium-family
    /// browsers (and modern Firefox builds) expose their tab strip.
    /// Titles only: the strip carries no URLs. Empty on any failure.
    pub fn tab_titles(pid: u32, cap: usize) -> Vec<String> {
        let auto = match automation() {
            Ok(a) => a,
            Err(_) => return Vec::new(),
        };
        let Some(root) = get_app_element(pid) else {
            return Vec::new();
        };
        unsafe {
            let v = VARIANT::from(UIA_TabItemControlTypeId.0);
            let cond = match auto.CreatePropertyCondition(UIA_ControlTypePropertyId, &v) {
                Ok(c) => c,
                Err(_) => return Vec::new(),
            };
            let arr = match root.FindAll(TreeScope_Descendants, &cond) {
                Ok(a) => a,
                Err(_) => return Vec::new(),
            };
            let len = arr.Length().unwrap_or(0);
            let mut out: Vec<String> = Vec::new();
            for i in 0..len {
                if out.len() >= cap {
                    break;
                }
                if let Ok(el) = arr.GetElement(i) {
                    let name = get_name(&el);
                    let name = name.trim();
                    if !name.is_empty() && !out.iter().any(|t| t == name) {
                        out.push(name.to_string());
                    }
                }
            }
            out
        }
    }

    fn bstr_to_string(b: BSTR) -> String {
        b.to_string()
    }

    // ── Control type IDs (re-exported for the walker) ──
    pub const CT_BUTTON: i32 = UIA_ButtonControlTypeId.0;
    pub const CT_CHECKBOX: i32 = UIA_CheckBoxControlTypeId.0;
    pub const CT_COMBOBOX: i32 = UIA_ComboBoxControlTypeId.0;
    pub const CT_EDIT: i32 = UIA_EditControlTypeId.0;
    pub const CT_HYPERLINK: i32 = UIA_HyperlinkControlTypeId.0;
    pub const CT_IMAGE: i32 = UIA_ImageControlTypeId.0;
    pub const CT_LIST: i32 = UIA_ListControlTypeId.0;
    pub const CT_MENU_BAR: i32 = UIA_MenuBarControlTypeId.0;
    pub const CT_MENU_ITEM: i32 = UIA_MenuItemControlTypeId.0;
    pub const CT_PANE: i32 = UIA_PaneControlTypeId.0;
    pub const CT_SLIDER: i32 = UIA_SliderControlTypeId.0;
    pub const CT_TAB: i32 = UIA_TabControlTypeId.0;
    pub const CT_TABLE: i32 = UIA_TableControlTypeId.0;
    pub const CT_DATAGRID: i32 = UIA_DataGridControlTypeId.0;
    pub const CT_TEXT: i32 = UIA_TextControlTypeId.0;
    pub const CT_TOOLBAR: i32 = UIA_ToolBarControlTypeId.0;
    pub const CT_TREE: i32 = UIA_TreeControlTypeId.0;
    pub const CT_DOCUMENT: i32 = UIA_DocumentControlTypeId.0;
    pub const CT_TITLEBAR: i32 = UIA_TitleBarControlTypeId.0;

    /// Short lowercase label for a control type, mirroring the macOS
    /// reader's "AXRole minus the AX prefix" convention.
    pub fn control_type_label(ct: i32) -> &'static str {
        match ct {
            c if c == CT_BUTTON => "button",
            c if c == CT_CHECKBOX => "checkbox",
            c if c == CT_COMBOBOX => "combobox",
            c if c == CT_EDIT => "textfield",
            c if c == CT_HYPERLINK => "link",
            c if c == CT_IMAGE => "image",
            c if c == CT_LIST => "list",
            c if c == CT_MENU_BAR => "menubar",
            c if c == CT_MENU_ITEM => "menuitem",
            c if c == CT_PANE => "group",
            c if c == CT_SLIDER => "slider",
            c if c == CT_TAB => "tab",
            c if c == CT_TABLE => "table",
            c if c == CT_DATAGRID => "datagrid",
            c if c == CT_TEXT => "statictext",
            c if c == CT_TOOLBAR => "toolbar",
            c if c == CT_TREE => "tree",
            c if c == CT_DOCUMENT => "textarea",
            c if c == CT_TITLEBAR => "titlebar",
            _ => "element",
        }
    }

    // Suppress unused-warnings for symbols only used in conditional code.
    #[allow(dead_code)] const _UIA_NAME: i32 = UIA_NamePropertyId.0;
    #[allow(dead_code)] const _UIA_AUTOID: i32 = UIA_AutomationIdPropertyId.0;
    #[allow(dead_code)] const _UIA_HELP: i32 = UIA_HelpTextPropertyId.0;
    #[allow(dead_code)] const _UIA_CONTROLTYPE: i32 = UIA_ControlTypePropertyId.0;
    #[allow(dead_code)] const _UIA_PROCESSID: i32 = UIA_ProcessIdPropertyId.0;
    #[allow(dead_code)] const _UIA_ENABLED: i32 = UIA_IsEnabledPropertyId.0;
}

// ── Data Types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInput {
    pub id: String,
    pub label: String,
    pub input_type: String,
    pub required: bool,
    pub current_value: Option<String>,
    pub max_length: Option<u32>,
    pub validation: Option<String>,
    pub accessibility_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowDataElement {
    pub id: String,
    pub element_type: String,
    pub label: String,
    pub readable: bool,
    pub current_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowAction {
    pub id: String,
    pub label: String,
    pub action_type: String,
    pub accessibility_id: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowNavigation {
    pub sections: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowProfileData {
    pub inputs: Vec<WindowInput>,
    pub data_elements: Vec<WindowDataElement>,
    pub actions: Vec<WindowAction>,
    pub navigation: WindowNavigation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomatableSummary {
    pub fillable_fields: u32,
    pub clickable_actions: u32,
    pub readable_data_points: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowProfile {
    pub window_id: String,
    pub application: String,
    pub title: String,
    pub pid: i64,
    pub url: Option<String>,
    pub captured_at: String,
    pub last_updated: String,
    pub profile: WindowProfileData,
    pub field_count: u32,
    pub action_count: u32,
    pub status: WindowStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automatable_summary: Option<AutomatableSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WindowStatus {
    Live,
    Stale,
    Disconnected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchedWindow {
    pub id: String,
    pub application: String,
    pub url_pattern: Option<String>,
    pub file_pattern: Option<String>,
    pub label: String,
    pub update_frequency: UpdateFrequency,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateFrequency {
    Realtime,
    OnChange,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowTwinEvent {
    pub id: String,
    pub window_id: String,
    pub event_type: TwinEventType,
    pub field_id: Option<String>,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TwinEventType {
    ValueChanged,
    FocusChanged,
    PageNavigated,
    WindowOpened,
    ActionTriggered,
    ProfileCreated,
    ProfileUpdated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowReadResult {
    pub window_id: String,
    pub fields: HashMap<String, Option<String>>,
    pub read_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowWriteRequest {
    pub field: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowWriteResult {
    pub window_id: String,
    pub written: Vec<String>,
    pub failed: Vec<String>,
    pub written_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowActionResult {
    pub window_id: String,
    pub action_id: String,
    pub success: bool,
    pub triggered_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilityStatus {
    pub trusted: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowLibrary {
    pub active_windows: Vec<WindowProfile>,
    pub watched: Vec<WatchedWindow>,
    pub total_fields_mapped: u32,
    pub total_actions_mapped: u32,
}

// ── Store ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowIntelligenceStore {
    pub profiles: Vec<WindowProfile>,
    pub watched: Vec<WatchedWindow>,
    pub events: Vec<WindowTwinEvent>,
}

impl Default for WindowIntelligenceStore {
    fn default() -> Self {
        Self {
            profiles: vec![],
            watched: vec![],
            events: vec![],
        }
    }
}

fn store_path() -> std::path::PathBuf {
    crate::storage::data_dir().join("window_intelligence.json")
}

impl WindowIntelligenceStore {
    pub fn load() -> Result<Self, String> {
        let path = store_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let data = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&data).map_err(|e| e.to_string())
    }

    pub fn save(&self) -> Result<(), String> {
        let dir = crate::storage::data_dir();
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        crate::storage::atomic_write(store_path(), json).map_err(|e| e.to_string())
    }

    // Check accessibility permissions - uses actual AX call, not just AXIsProcessTrusted
    pub fn check_accessibility(&self) -> AccessibilityStatus {
        #[cfg(target_os = "macos")]
        {
            // First try the official API
            let api_trusted = ax::is_trusted();
            // Then try an actual AX call (more reliable after rebuilds)
            let actually_works = ax::test_access();

            let trusted = api_trusted || actually_works;
            AccessibilityStatus {
                trusted,
                message: if trusted {
                    "Accessibility access granted. Window Intelligence is ready.".into()
                } else {
                    "Accessibility access required. Click 'Request Access' or go to System Settings > Privacy & Security > Accessibility and enable Orbix Core.".into()
                },
            }
        }
        #[cfg(target_os = "windows")]
        {
            let trusted = uia::is_trusted() && uia::test_access();
            AccessibilityStatus {
                trusted,
                message: if trusted {
                    "UI Automation access granted. Window Intelligence is ready.".into()
                } else {
                    "UI Automation initialisation failed. Restart OrbixCore and try again.".into()
                },
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            AccessibilityStatus {
                trusted: false,
                message: "Window Intelligence requires macOS or Windows.".into(),
            }
        }
    }

    // Request accessibility access - shows the macOS system prompt
    pub fn request_accessibility(&self) -> AccessibilityStatus {
        #[cfg(target_os = "macos")]
        {
            let trusted = ax::request_access();
            let actually_works = ax::test_access();
            let ok = trusted || actually_works;
            AccessibilityStatus {
                trusted: ok,
                message: if ok {
                    "Accessibility access granted. Window Intelligence is ready.".into()
                } else {
                    "Accessibility access was not granted. Please enable Orbix Core in System Settings > Privacy & Security > Accessibility, then click 'Check Again'.".into()
                },
            }
        }
        #[cfg(target_os = "windows")]
        {
            // Windows UI Automation doesn't gate on a separate permission -
            // request_accessibility is effectively a re-check.
            self.check_accessibility()
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            AccessibilityStatus {
                trusted: false,
                message: "Window Intelligence requires macOS or Windows.".into(),
            }
        }
    }

    /// Called from main() when --wi-profile is passed. Runs the actual AX walk
    /// and prints JSON to stdout. If this crashes, only the subprocess dies.
    #[cfg(target_os = "macos")]
    pub fn run_profile_subprocess(pid: i64, app_name: String) -> Result<(), String> {
        let profile = Self::profile_window_direct(pid, app_name)?;
        let json = serde_json::to_string(&profile).map_err(|e| e.to_string())?;
        print!("{}", json);
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn profile_window_direct(pid: i64, app_name: String) -> Result<WindowProfile, String> {
            let app_el = ax::get_app_element(pid as i32)
                .ok_or("Could not access application. Make sure it is running and Orbix Core has accessibility access.")?;

            // Read window title
            let title = ax::get_attribute_string(app_el, "AXTitle")
                .or_else(|| ax::get_attribute_string(app_el, "AXDescription"))
                .unwrap_or_else(|| app_name.clone());

            // Get focused window
            let window_el = ax::get_attribute_value(app_el, "AXFocusedWindow")
                .unwrap_or(app_el);

            let window_title = ax::get_attribute_string(window_el, "AXTitle")
                .unwrap_or_else(|| title.clone());

            // Targeted shallow profile
            let mut inputs = Vec::new();
            let mut data_elements = Vec::new();
            let mut actions = Vec::new();
            let mut nav_sections = Vec::new();
            let mut element_count: u32 = 0;

            // Use a dummy store just for the walk method
            let store = Self::default();
            store.walk_ax_tree_shallow(
                window_el,
                &mut inputs,
                &mut data_elements,
                &mut actions,
                &mut nav_sections,
                0,
                4,
                &mut element_count,
                300,
            );

            let now = chrono::Utc::now().to_rfc3339();
            let window_id = format!("win_{}_{}", pid, uuid::Uuid::new_v4().to_string().split('-').next().unwrap());

            // ── Post-walk relevance filtering ──

            // Collect input labels for dedup
            let input_labels: std::collections::HashSet<String> = inputs.iter()
                .map(|i| i.label.to_lowercase())
                .filter(|l| !l.is_empty())
                .collect();

            // Filter data_elements: remove form labels and empty elements
            data_elements.retain(|d| {
                // Skip if label matches an input label (it's a form label, not standalone data)
                if !d.label.is_empty() && input_labels.contains(&d.label.to_lowercase()) {
                    return false;
                }
                // Skip very short text that's likely a separator
                if d.element_type == "text" && d.label.len() <= 2 && d.current_value.is_none() {
                    return false;
                }
                // Skip empty data elements
                if d.label.is_empty() && d.current_value.is_none() {
                    return false;
                }
                true
            });

            // Filter actions: remove system chrome buttons
            let chrome_buttons: std::collections::HashSet<&str> = [
                "close", "minimize", "zoom", "fullscreen", "full screen",
                "close button", "minimize button", "zoom button",
            ].iter().copied().collect();
            actions.retain(|a| {
                !chrome_buttons.contains(a.label.to_lowercase().as_str())
            });

            let field_count = inputs.len() as u32;
            let action_count = actions.len() as u32;

            let summary = AutomatableSummary {
                fillable_fields: field_count,
                clickable_actions: action_count,
                readable_data_points: data_elements.len() as u32,
            };

            let profile = WindowProfile {
                window_id: window_id.clone(),
                application: app_name,
                title: window_title,
                pid,
                url: None,
                captured_at: now.clone(),
                last_updated: now.clone(),
                profile: WindowProfileData {
                    inputs,
                    data_elements,
                    actions,
                    navigation: WindowNavigation { sections: nav_sections },
                },
                field_count,
                action_count,
                status: WindowStatus::Live,
                automatable_summary: Some(summary),
            };

            Ok(profile)
    }

    #[cfg(target_os = "macos")]
    fn walk_ax_tree_shallow(
        &self,
        element: ax::AXUIElementRef,
        inputs: &mut Vec<WindowInput>,
        data_elements: &mut Vec<WindowDataElement>,
        actions: &mut Vec<WindowAction>,
        nav_sections: &mut Vec<String>,
        depth: u32,
        max_depth: u32,
        element_count: &mut u32,
        max_elements: u32,
    ) {
        if depth > max_depth { return; }
        if *element_count >= max_elements { return; }
        if element.is_null() { return; }
        *element_count += 1;

        let role = match ax::get_attribute_string(element, "AXRole") {
            Some(r) => r,
            None => return, // stale or inaccessible element
        };
        let label = ax::get_attribute_string(element, "AXTitle")
            .or_else(|| ax::get_attribute_string(element, "AXDescription"))
            .unwrap_or_default();
        let value = ax::get_attribute_string(element, "AXValue");
        let identifier = ax::get_attribute_string(element, "AXIdentifier");

        match role.as_str() {
            // Input fields
            "AXTextField" | "AXTextArea" => {
                let id = identifier.clone().unwrap_or_else(|| format!("input_{}", inputs.len()));
                inputs.push(WindowInput {
                    id,
                    label: label.clone(),
                    input_type: if role == "AXTextArea" { "textarea".into() } else { "text".into() },
                    required: false,
                    current_value: value.clone(),
                    max_length: None,
                    validation: None,
                    accessibility_id: identifier.clone(),
                });
            }
            "AXSecureTextField" => { /* skip password fields entirely */ }
            "AXComboBox" | "AXPopUpButton" => {
                let id = identifier.clone().unwrap_or_else(|| format!("select_{}", inputs.len()));
                inputs.push(WindowInput {
                    id,
                    label: label.clone(),
                    input_type: "select".into(),
                    required: false,
                    current_value: value.clone(),
                    max_length: None,
                    validation: None,
                    accessibility_id: identifier.clone(),
                });
            }
            "AXCheckBox" => {
                let id = identifier.clone().unwrap_or_else(|| format!("check_{}", inputs.len()));
                inputs.push(WindowInput {
                    id,
                    label: label.clone(),
                    input_type: "checkbox".into(),
                    required: false,
                    current_value: value.clone(),
                    max_length: None,
                    validation: None,
                    accessibility_id: identifier.clone(),
                });
            }
            "AXSlider" => {
                let id = identifier.clone().unwrap_or_else(|| format!("slider_{}", inputs.len()));
                inputs.push(WindowInput {
                    id,
                    label: label.clone(),
                    input_type: "slider".into(),
                    required: false,
                    current_value: value.clone(),
                    max_length: None,
                    validation: None,
                    accessibility_id: identifier.clone(),
                });
            }

            // Action elements
            "AXButton" => {
                if !label.is_empty() {
                    let id = identifier.clone().unwrap_or_else(|| format!("btn_{}", actions.len()));
                    actions.push(WindowAction {
                        id, label: label.clone(), action_type: "button".into(),
                        accessibility_id: identifier.clone(), enabled: true,
                    });
                }
            }
            "AXMenuItem" | "AXMenuButton" => {
                if !label.is_empty() {
                    let id = identifier.clone().unwrap_or_else(|| format!("menu_{}", actions.len()));
                    actions.push(WindowAction {
                        id, label: label.clone(), action_type: "menu_item".into(),
                        accessibility_id: identifier.clone(), enabled: true,
                    });
                }
            }
            "AXLink" => {
                if !label.is_empty() {
                    let id = identifier.clone().unwrap_or_else(|| format!("link_{}", actions.len()));
                    actions.push(WindowAction {
                        id, label: label.clone(), action_type: "link".into(),
                        accessibility_id: identifier.clone(), enabled: true,
                    });
                }
            }

            // Data elements
            "AXStaticText" => {
                if !label.is_empty() || value.is_some() {
                    let id = identifier.clone().unwrap_or_else(|| format!("text_{}", data_elements.len()));
                    data_elements.push(WindowDataElement {
                        id, element_type: "text".into(), label: label.clone(),
                        readable: true, current_value: value.clone().or_else(|| Some(label.clone())),
                    });
                }
            }
            "AXTable" | "AXOutline" | "AXList" => {
                let id = identifier.clone().unwrap_or_else(|| format!("table_{}", data_elements.len()));
                data_elements.push(WindowDataElement {
                    id, element_type: role.trim_start_matches("AX").to_lowercase(),
                    label: label.clone(), readable: true, current_value: None,
                });
                // Don't recurse into tables/lists - they can have thousands of rows
                return;
            }
            "AXImage" => {
                if !label.is_empty() {
                    let id = identifier.clone().unwrap_or_else(|| format!("img_{}", data_elements.len()));
                    data_elements.push(WindowDataElement {
                        id, element_type: "image".into(), label: label.clone(),
                        readable: true, current_value: None,
                    });
                }
            }

            // Navigation
            "AXTabGroup" => {
                let (tab_arr, children) = ax::get_children_with_owner(element);
                for child in &children {
                    if let Some(tab_label) = ax::get_attribute_string(*child, "AXTitle") {
                        if !tab_label.is_empty() {
                            nav_sections.push(tab_label);
                        }
                    }
                }
                if !tab_arr.is_null() { unsafe { ax::release(tab_arr); } }
                // Don't recurse further into tab groups
                return;
            }
            "AXToolbar" | "AXMenuBar" => {
                if !label.is_empty() {
                    nav_sections.push(label.clone());
                }
            }

            // Skip web content areas entirely - they have massive DOM trees
            "AXWebArea" | "AXScrollArea" => {
                return;
            }

            _ => {}
        }

        // Recurse into children - keep array alive while iterating
        if *element_count < max_elements {
            let (arr_owner, children) = ax::get_children_with_owner(element);
            // Limit children per node at this level too
            let max_children = if depth < 2 { 50 } else { 20 };
            for (i, child) in children.iter().enumerate() {
                if i >= max_children { break; }
                if *element_count >= max_elements { break; }
                self.walk_ax_tree_shallow(
                    *child, inputs, data_elements, actions, nav_sections,
                    depth + 1, max_depth, element_count, max_elements,
                );
            }
            if !arr_owner.is_null() { unsafe { ax::release(arr_owner); } }
        }
    }

    // Refresh: get existing profile's pid/app for re-profiling
    pub fn get_profile_info(&self, window_id: &str) -> Result<(i64, String, String), String> {
        let existing = self.profiles.iter().find(|p| p.window_id == window_id)
            .ok_or("Window profile not found")?;
        Ok((existing.pid, existing.application.clone(), existing.captured_at.clone()))
    }

    // Update store after a refresh completes
    pub fn store_refreshed_profile(&mut self, window_id: &str, mut profile: WindowProfile, captured_at: String) {
        profile.window_id = window_id.to_string();
        profile.captured_at = captured_at;

        if let Some(p) = self.profiles.iter_mut().find(|p| p.window_id == window_id) {
            *p = profile.clone();
        }

        self.events.push(WindowTwinEvent {
            id: uuid::Uuid::new_v4().to_string(),
            window_id: window_id.to_string(),
            event_type: TwinEventType::ProfileUpdated,
            field_id: None,
            old_value: None,
            new_value: Some(format!("{} fields, {} actions", profile.field_count, profile.action_count)),
            timestamp: chrono::Utc::now().to_rfc3339(),
        });

        self.save().ok();
    }

    // Read field values from a profiled window
    pub fn read_fields(&self, window_id: &str, fields: Vec<String>) -> Result<WindowReadResult, String> {
        let profile = self.profiles.iter().find(|p| p.window_id == window_id)
            .ok_or("Window profile not found")?;

        #[cfg(target_os = "macos")]
        {
            let app_el = ax::get_app_element(profile.pid as i32)
                .ok_or("Application no longer running or accessibility access not granted.")?;

            let window_el = ax::get_attribute_value(app_el, "AXFocusedWindow")
                .unwrap_or(app_el);

            let mut result_fields = HashMap::new();

            for field_id in &fields {
                // Try to find the element by walking the tree and matching
                let value = self.find_field_value(window_el, field_id, &profile.profile.inputs, 0, 8);
                result_fields.insert(field_id.clone(), value);
            }

            Ok(WindowReadResult {
                window_id: window_id.to_string(),
                fields: result_fields,
                read_at: chrono::Utc::now().to_rfc3339(),
            })
        }

        #[cfg(target_os = "windows")]
        {
            let window_el = uia::get_app_element(profile.pid as u32)
                .ok_or("Application no longer running, or has no visible window.")?;

            let mut result_fields = HashMap::new();
            for field_id in &fields {
                let value = self.find_field_value_uia(&window_el, field_id, &profile.profile.inputs, 0, 8);
                result_fields.insert(field_id.clone(), value);
            }

            Ok(WindowReadResult {
                window_id: window_id.to_string(),
                fields: result_fields,
                read_at: chrono::Utc::now().to_rfc3339(),
            })
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let mut result_fields = HashMap::new();
            for field_id in &fields {
                let value = profile.profile.inputs.iter()
                    .find(|i| i.id == *field_id)
                    .and_then(|i| i.current_value.clone());
                result_fields.insert(field_id.clone(), value);
            }
            Ok(WindowReadResult {
                window_id: window_id.to_string(),
                fields: result_fields,
                read_at: chrono::Utc::now().to_rfc3339(),
            })
        }
    }

    #[cfg(target_os = "macos")]
    fn find_field_value(
        &self,
        element: ax::AXUIElementRef,
        field_id: &str,
        inputs: &[WindowInput],
        depth: u32,
        max_depth: u32,
    ) -> Option<String> {
        if depth > max_depth { return None; }

        let identifier = ax::get_attribute_string(element, "AXIdentifier");
        let _role = ax::get_attribute_string(element, "AXRole").unwrap_or_default();

        // Check if this element matches the field
        if identifier.as_deref() == Some(field_id) {
            return ax::get_attribute_string(element, "AXValue");
        }

        // Also check by matching label
        if let Some(input) = inputs.iter().find(|i| i.id == field_id) {
            let label = ax::get_attribute_string(element, "AXTitle")
                .or_else(|| ax::get_attribute_string(element, "AXDescription"));
            if label.as_deref() == Some(&input.label) {
                return ax::get_attribute_string(element, "AXValue");
            }
        }

        // Recurse
        let (arr_owner, children) = ax::get_children_with_owner(element);
        let mut found = None;
        for child in &children {
            if let Some(val) = self.find_field_value(*child, field_id, inputs, depth + 1, max_depth) {
                found = Some(val);
                break;
            }
        }
        if !arr_owner.is_null() { unsafe { ax::release(arr_owner); } }
        found
    }

    // Write values to fields in a window
    pub fn write_fields(&mut self, window_id: &str, writes: Vec<WindowWriteRequest>) -> Result<WindowWriteResult, String> {
        let profile = self.profiles.iter().find(|p| p.window_id == window_id)
            .ok_or("Window profile not found")?
            .clone();

        #[cfg(target_os = "macos")]
        {
            let app_el = ax::get_app_element(profile.pid as i32)
                .ok_or("Application no longer running or accessibility access not granted.")?;

            let window_el = ax::get_attribute_value(app_el, "AXFocusedWindow")
                .unwrap_or(app_el);

            let mut written = Vec::new();
            let mut failed = Vec::new();

            for write in &writes {
                let success = self.write_field_value(
                    window_el,
                    &write.field,
                    &write.value,
                    &profile.profile.inputs,
                    0,
                    8,
                );
                if success {
                    // Log event
                    let old_val = profile.profile.inputs.iter()
                        .find(|i| i.id == write.field)
                        .and_then(|i| i.current_value.clone());
                    self.events.push(WindowTwinEvent {
                        id: uuid::Uuid::new_v4().to_string(),
                        window_id: window_id.to_string(),
                        event_type: TwinEventType::ValueChanged,
                        field_id: Some(write.field.clone()),
                        old_value: old_val,
                        new_value: Some(write.value.clone()),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    });
                    written.push(write.field.clone());
                } else {
                    failed.push(write.field.clone());
                }
            }

            // Update cached values in profile
            if let Some(p) = self.profiles.iter_mut().find(|p| p.window_id == window_id) {
                for w in &writes {
                    if written.contains(&w.field) {
                        if let Some(input) = p.profile.inputs.iter_mut().find(|i| i.id == w.field) {
                            input.current_value = Some(w.value.clone());
                        }
                    }
                }
                p.last_updated = chrono::Utc::now().to_rfc3339();
            }

            self.save().ok();

            Ok(WindowWriteResult {
                window_id: window_id.to_string(),
                written,
                failed,
                written_at: chrono::Utc::now().to_rfc3339(),
            })
        }

        #[cfg(target_os = "windows")]
        {
            let window_el = uia::get_app_element(profile.pid as u32)
                .ok_or("Application no longer running, or has no visible window.")?;

            let mut written = Vec::new();
            let mut failed = Vec::new();

            for write in &writes {
                let success = self.write_field_value_uia(
                    &window_el, &write.field, &write.value,
                    &profile.profile.inputs, 0, 8,
                );
                if success {
                    let old_val = profile.profile.inputs.iter()
                        .find(|i| i.id == write.field)
                        .and_then(|i| i.current_value.clone());
                    self.events.push(WindowTwinEvent {
                        id: uuid::Uuid::new_v4().to_string(),
                        window_id: window_id.to_string(),
                        event_type: TwinEventType::ValueChanged,
                        field_id: Some(write.field.clone()),
                        old_value: old_val,
                        new_value: Some(write.value.clone()),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    });
                    written.push(write.field.clone());
                } else {
                    failed.push(write.field.clone());
                }
            }

            if let Some(p) = self.profiles.iter_mut().find(|p| p.window_id == window_id) {
                for w in &writes {
                    if written.contains(&w.field) {
                        if let Some(input) = p.profile.inputs.iter_mut().find(|i| i.id == w.field) {
                            input.current_value = Some(w.value.clone());
                        }
                    }
                }
                p.last_updated = chrono::Utc::now().to_rfc3339();
            }

            self.save().ok();

            Ok(WindowWriteResult {
                window_id: window_id.to_string(),
                written,
                failed,
                written_at: chrono::Utc::now().to_rfc3339(),
            })
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            Err("Window Intelligence requires macOS or Windows.".into())
        }
    }

    #[cfg(target_os = "macos")]
    fn write_field_value(
        &self,
        element: ax::AXUIElementRef,
        field_id: &str,
        value: &str,
        inputs: &[WindowInput],
        depth: u32,
        max_depth: u32,
    ) -> bool {
        if depth > max_depth { return false; }

        let identifier = ax::get_attribute_string(element, "AXIdentifier");

        // Check if this element matches
        if identifier.as_deref() == Some(field_id) {
            return ax::set_attribute_string(element, "AXValue", value);
        }

        // Also check by label
        if let Some(input) = inputs.iter().find(|i| i.id == field_id) {
            let label = ax::get_attribute_string(element, "AXTitle")
                .or_else(|| ax::get_attribute_string(element, "AXDescription"));
            if label.as_deref() == Some(&input.label) {
                return ax::set_attribute_string(element, "AXValue", value);
            }
        }

        // Recurse
        let (arr_owner, children) = ax::get_children_with_owner(element);
        let mut ok = false;
        for child in &children {
            if self.write_field_value(*child, field_id, value, inputs, depth + 1, max_depth) {
                ok = true;
                break;
            }
        }
        if !arr_owner.is_null() { unsafe { ax::release(arr_owner); } }
        ok
    }

    // Trigger an action in a window
    pub fn trigger_action(&mut self, window_id: &str, action_id: &str) -> Result<WindowActionResult, String> {
        let profile = self.profiles.iter().find(|p| p.window_id == window_id)
            .ok_or("Window profile not found")?
            .clone();

        let _action = profile.profile.actions.iter().find(|a| a.id == action_id)
            .ok_or("Action not found in profile")?;

        #[cfg(target_os = "macos")]
        {
            let app_el = ax::get_app_element(profile.pid as i32)
                .ok_or("Application no longer running or accessibility access not granted.")?;

            let window_el = ax::get_attribute_value(app_el, "AXFocusedWindow")
                .unwrap_or(app_el);

            let success = self.find_and_trigger(
                window_el,
                action_id,
                &profile.profile.actions,
                0,
                8,
            );

            if success {
                self.events.push(WindowTwinEvent {
                    id: uuid::Uuid::new_v4().to_string(),
                    window_id: window_id.to_string(),
                    event_type: TwinEventType::ActionTriggered,
                    field_id: Some(action_id.to_string()),
                    old_value: None,
                    new_value: None,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                });
                self.save().ok();
            }

            Ok(WindowActionResult {
                window_id: window_id.to_string(),
                action_id: action_id.to_string(),
                success,
                triggered_at: chrono::Utc::now().to_rfc3339(),
            })
        }

        #[cfg(target_os = "windows")]
        {
            let window_el = uia::get_app_element(profile.pid as u32)
                .ok_or("Application no longer running, or has no visible window.")?;

            let success = self.find_and_trigger_uia(
                &window_el, action_id, &profile.profile.actions, 0, 8,
            );

            if success {
                self.events.push(WindowTwinEvent {
                    id: uuid::Uuid::new_v4().to_string(),
                    window_id: window_id.to_string(),
                    event_type: TwinEventType::ActionTriggered,
                    field_id: Some(action_id.to_string()),
                    old_value: None,
                    new_value: None,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                });
                self.save().ok();
            }

            Ok(WindowActionResult {
                window_id: window_id.to_string(),
                action_id: action_id.to_string(),
                success,
                triggered_at: chrono::Utc::now().to_rfc3339(),
            })
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            Err("Window Intelligence requires macOS or Windows.".into())
        }
    }

    #[cfg(target_os = "macos")]
    fn find_and_trigger(
        &self,
        element: ax::AXUIElementRef,
        action_id: &str,
        actions: &[WindowAction],
        depth: u32,
        max_depth: u32,
    ) -> bool {
        if depth > max_depth { return false; }

        let identifier = ax::get_attribute_string(element, "AXIdentifier");
        let role = ax::get_attribute_string(element, "AXRole").unwrap_or_default();

        // Match by identifier
        if identifier.as_deref() == Some(action_id) {
            return ax::perform_action(element, "AXPress");
        }

        // Match by label
        if let Some(action) = actions.iter().find(|a| a.id == action_id) {
            let label = ax::get_attribute_string(element, "AXTitle")
                .or_else(|| ax::get_attribute_string(element, "AXDescription"));
            if label.as_deref() == Some(&action.label) && role == "AXButton" {
                return ax::perform_action(element, "AXPress");
            }
        }

        // Recurse
        let (arr_owner, children) = ax::get_children_with_owner(element);
        let mut ok = false;
        for child in &children {
            if self.find_and_trigger(*child, action_id, actions, depth + 1, max_depth) {
                ok = true;
                break;
            }
        }
        if !arr_owner.is_null() { unsafe { ax::release(arr_owner); } }
        ok
    }

    // Get library overview
    pub fn get_library(&self) -> WindowLibrary {
        let total_fields: u32 = self.profiles.iter().map(|p| p.field_count).sum();
        let total_actions: u32 = self.profiles.iter().map(|p| p.action_count).sum();

        WindowLibrary {
            active_windows: self.profiles.clone(),
            watched: self.watched.clone(),
            total_fields_mapped: total_fields,
            total_actions_mapped: total_actions,
        }
    }

    // Get a single profile
    pub fn get_profile(&self, window_id: &str) -> Option<&WindowProfile> {
        self.profiles.iter().find(|p| p.window_id == window_id)
    }

    // Remove a profile
    pub fn remove_profile(&mut self, window_id: &str) -> Result<(), String> {
        self.profiles.retain(|p| p.window_id != window_id);
        self.save()
    }

    // Add watched window
    pub fn add_watched(&mut self, watched: WatchedWindow) -> Result<WatchedWindow, String> {
        let w = WatchedWindow {
            id: uuid::Uuid::new_v4().to_string(),
            ..watched
        };
        self.watched.push(w.clone());
        self.save()?;
        Ok(w)
    }

    // Remove watched window
    pub fn remove_watched(&mut self, id: &str) -> Result<(), String> {
        self.watched.retain(|w| w.id != id);
        self.save()
    }

    // Toggle watched window active state
    pub fn toggle_watched(&mut self, id: &str) -> Result<WatchedWindow, String> {
        let w = self.watched.iter_mut().find(|w| w.id == id)
            .ok_or("Watched window not found")?;
        w.active = !w.active;
        let result = w.clone();
        self.save()?;
        Ok(result)
    }

    // Get recent events
    pub fn get_events(&self, window_id: Option<&str>, limit: usize) -> Vec<WindowTwinEvent> {
        let mut events: Vec<_> = match window_id {
            Some(wid) => self.events.iter().filter(|e| e.window_id == wid).cloned().collect(),
            None => self.events.clone(),
        };
        events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        events.truncate(limit);
        events
    }

    // List running applications (for the profile picker)
    pub fn list_running_apps(&self) -> Vec<RunningApp> {
        #[cfg(target_os = "macos")]
        {
            use std::process::Command;
            // Use AppleScript to list running apps with windows
            let output = Command::new("osascript")
                .arg("-e")
                .arg(r#"
                    set appList to ""
                    tell application "System Events"
                        set procs to every process whose background only is false
                        repeat with p in procs
                            set appList to appList & (name of p) & "|" & (unix id of p) & "\n"
                        end repeat
                    end tell
                    return appList
                "#)
                .output();

            match output {
                Ok(out) => {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    stdout.lines()
                        .filter_map(|line| {
                            let parts: Vec<&str> = line.split('|').collect();
                            if parts.len() == 2 {
                                let name = parts[0].trim().to_string();
                                let pid = parts[1].trim().parse::<i64>().ok()?;
                                Some(RunningApp { name, pid })
                            } else {
                                None
                            }
                        })
                        .collect()
                }
                Err(_) => vec![],
            }
        }

        #[cfg(target_os = "windows")]
        {
            // Foreground processes that own a visible window with a title.
            // We use Toolhelp32 via PowerShell to keep the dependency surface
            // minimal - Get-Process exposes everything we need.
            let output = crate::process_ext::std_command("powershell.exe")
                .args([
                    "-NoProfile", "-NonInteractive", "-Command",
                    "Get-Process | Where-Object { $_.MainWindowTitle -ne '' -and $_.MainWindowHandle -ne 0 } | Select-Object @{N='name';E={$_.ProcessName}}, @{N='pid';E={$_.Id}} | ConvertTo-Json -Compress -AsArray"
                ])
                .output();

            match output {
                Ok(out) if out.status.success() => {
                    let json = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if json.is_empty() { return vec![]; }
                    serde_json::from_str(&json).unwrap_or_default()
                }
                _ => vec![],
            }
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            vec![]
        }
    }

    // ── Windows UI Automation implementations ──────────────────────────────
    // Mirrors the macOS profile / read / write / trigger pipeline using UIA.

    #[cfg(target_os = "windows")]
    pub fn run_profile_subprocess(pid: i64, app_name: String) -> Result<(), String> {
        let profile = Self::profile_window_direct(pid, app_name)?;
        let json = serde_json::to_string(&profile).map_err(|e| e.to_string())?;
        print!("{}", json);
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn profile_window_direct(pid: i64, app_name: String) -> Result<WindowProfile, String> {
        let window_el = uia::get_app_element(pid as u32)
            .ok_or("Could not access application. Make sure it is running and has a visible window.")?;

        let window_title = {
            let n = uia::get_name(&window_el);
            if n.is_empty() { app_name.clone() } else { n }
        };

        let mut inputs = Vec::new();
        let mut data_elements = Vec::new();
        let mut actions = Vec::new();
        let mut nav_sections = Vec::new();
        let mut element_count: u32 = 0;

        let store = Self::default();
        store.walk_uia_tree_shallow(
            &window_el,
            &mut inputs,
            &mut data_elements,
            &mut actions,
            &mut nav_sections,
            0,
            4,
            &mut element_count,
            300,
        );

        let now = chrono::Utc::now().to_rfc3339();
        let window_id = format!("win_{}_{}", pid, uuid::Uuid::new_v4().to_string().split('-').next().unwrap());

        // Same post-walk filtering as the macOS path so the JSON shape stays identical.
        let input_labels: std::collections::HashSet<String> = inputs.iter()
            .map(|i| i.label.to_lowercase())
            .filter(|l| !l.is_empty())
            .collect();

        data_elements.retain(|d| {
            if !d.label.is_empty() && input_labels.contains(&d.label.to_lowercase()) {
                return false;
            }
            if d.element_type == "text" && d.label.len() <= 2 && d.current_value.is_none() {
                return false;
            }
            if d.label.is_empty() && d.current_value.is_none() {
                return false;
            }
            true
        });

        let chrome_buttons: std::collections::HashSet<&str> = [
            "close", "minimize", "maximize", "restore", "system menu",
            "close button", "minimize button", "maximize button",
        ].iter().copied().collect();
        actions.retain(|a| !chrome_buttons.contains(a.label.to_lowercase().as_str()));

        let field_count = inputs.len() as u32;
        let action_count = actions.len() as u32;

        let summary = AutomatableSummary {
            fillable_fields: field_count,
            clickable_actions: action_count,
            readable_data_points: data_elements.len() as u32,
        };

        Ok(WindowProfile {
            window_id,
            application: app_name,
            title: window_title,
            pid,
            url: None,
            captured_at: now.clone(),
            last_updated: now,
            profile: WindowProfileData {
                inputs,
                data_elements,
                actions,
                navigation: WindowNavigation { sections: nav_sections },
            },
            field_count,
            action_count,
            status: WindowStatus::Live,
            automatable_summary: Some(summary),
        })
    }

    #[cfg(target_os = "windows")]
    fn walk_uia_tree_shallow(
        &self,
        element: &windows::Win32::UI::Accessibility::IUIAutomationElement,
        inputs: &mut Vec<WindowInput>,
        data_elements: &mut Vec<WindowDataElement>,
        actions: &mut Vec<WindowAction>,
        nav_sections: &mut Vec<String>,
        depth: u32,
        max_depth: u32,
        element_count: &mut u32,
        max_elements: u32,
    ) {
        if depth > max_depth { return; }
        if *element_count >= max_elements { return; }
        *element_count += 1;

        let control_type = uia::get_control_type(element);
        let label = {
            let n = uia::get_name(element);
            if n.is_empty() {
                uia::get_help_text(element).unwrap_or_default()
            } else {
                n
            }
        };
        let value = uia::get_value(element);
        let identifier = uia::get_automation_id(element);
        let enabled = uia::is_enabled(element);

        match control_type {
            // ── Inputs ────────────────────────────────────────────────
            // UIA_EditControlTypeId is a single-line text box; UIA_DocumentControlTypeId
            // is the multi-line text-area equivalent (used by RichEdit, browsers, etc.).
            ct if ct == uia::CT_EDIT || ct == uia::CT_DOCUMENT => {
                let id = identifier.clone().unwrap_or_else(|| format!("input_{}", inputs.len()));
                // Filter password fields by AutomationId convention used by most
                // Windows controls - mirrors the AXSecureTextField skip on macOS.
                let is_password = label.to_lowercase().contains("password")
                    || identifier.as_deref().map(|s| s.to_lowercase().contains("password")).unwrap_or(false);
                if is_password {
                    return;
                }
                inputs.push(WindowInput {
                    id,
                    label: label.clone(),
                    input_type: if control_type == uia::CT_DOCUMENT { "textarea".into() } else { "text".into() },
                    required: false,
                    current_value: value.clone(),
                    max_length: None,
                    validation: None,
                    accessibility_id: identifier.clone(),
                });
            }
            ct if ct == uia::CT_COMBOBOX => {
                let id = identifier.clone().unwrap_or_else(|| format!("select_{}", inputs.len()));
                inputs.push(WindowInput {
                    id, label: label.clone(), input_type: "select".into(), required: false,
                    current_value: value.clone(), max_length: None, validation: None,
                    accessibility_id: identifier.clone(),
                });
            }
            ct if ct == uia::CT_CHECKBOX => {
                let id = identifier.clone().unwrap_or_else(|| format!("check_{}", inputs.len()));
                inputs.push(WindowInput {
                    id, label: label.clone(), input_type: "checkbox".into(), required: false,
                    current_value: value.clone(), max_length: None, validation: None,
                    accessibility_id: identifier.clone(),
                });
            }
            ct if ct == uia::CT_SLIDER => {
                let id = identifier.clone().unwrap_or_else(|| format!("slider_{}", inputs.len()));
                inputs.push(WindowInput {
                    id, label: label.clone(), input_type: "slider".into(), required: false,
                    current_value: value.clone(), max_length: None, validation: None,
                    accessibility_id: identifier.clone(),
                });
            }

            // ── Actions ───────────────────────────────────────────────
            ct if ct == uia::CT_BUTTON => {
                if !label.is_empty() {
                    let id = identifier.clone().unwrap_or_else(|| format!("btn_{}", actions.len()));
                    actions.push(WindowAction {
                        id, label: label.clone(), action_type: "button".into(),
                        accessibility_id: identifier.clone(), enabled,
                    });
                }
            }
            ct if ct == uia::CT_MENU_ITEM => {
                if !label.is_empty() {
                    let id = identifier.clone().unwrap_or_else(|| format!("menu_{}", actions.len()));
                    actions.push(WindowAction {
                        id, label: label.clone(), action_type: "menu_item".into(),
                        accessibility_id: identifier.clone(), enabled,
                    });
                }
            }
            ct if ct == uia::CT_HYPERLINK => {
                if !label.is_empty() {
                    let id = identifier.clone().unwrap_or_else(|| format!("link_{}", actions.len()));
                    actions.push(WindowAction {
                        id, label: label.clone(), action_type: "link".into(),
                        accessibility_id: identifier.clone(), enabled,
                    });
                }
            }

            // ── Static data ───────────────────────────────────────────
            ct if ct == uia::CT_TEXT => {
                if !label.is_empty() || value.is_some() {
                    let id = identifier.clone().unwrap_or_else(|| format!("text_{}", data_elements.len()));
                    data_elements.push(WindowDataElement {
                        id, element_type: "text".into(), label: label.clone(),
                        readable: true, current_value: value.clone().or_else(|| Some(label.clone())),
                    });
                }
            }
            ct if ct == uia::CT_TABLE || ct == uia::CT_DATAGRID || ct == uia::CT_LIST || ct == uia::CT_TREE => {
                let element_type = match control_type {
                    c if c == uia::CT_TABLE => "table",
                    c if c == uia::CT_DATAGRID => "datagrid",
                    c if c == uia::CT_LIST => "list",
                    _ => "tree",
                };
                let id = identifier.clone().unwrap_or_else(|| format!("table_{}", data_elements.len()));
                data_elements.push(WindowDataElement {
                    id, element_type: element_type.into(),
                    label: label.clone(), readable: true, current_value: None,
                });
                // Don't recurse: list views can have thousands of rows.
                return;
            }
            ct if ct == uia::CT_IMAGE => {
                if !label.is_empty() {
                    let id = identifier.clone().unwrap_or_else(|| format!("img_{}", data_elements.len()));
                    data_elements.push(WindowDataElement {
                        id, element_type: "image".into(), label: label.clone(),
                        readable: true, current_value: None,
                    });
                }
            }

            // ── Navigation ────────────────────────────────────────────
            ct if ct == uia::CT_TAB => {
                let children = uia::get_children(element);
                for child in &children {
                    let tab_label = uia::get_name(child);
                    if !tab_label.is_empty() {
                        nav_sections.push(tab_label);
                    }
                }
                return;
            }
            ct if ct == uia::CT_TOOLBAR || ct == uia::CT_MENU_BAR => {
                if !label.is_empty() {
                    nav_sections.push(label.clone());
                }
            }

            // ── Skip massive content panes ────────────────────────────
            // The browser's main viewport, document body, etc. is a Pane
            // that contains the entire DOM - recursing into it would walk
            // tens of thousands of nodes.
            ct if ct == uia::CT_PANE && depth >= 2 => {
                return;
            }

            _ => {}
        }

        if *element_count < max_elements {
            let children = uia::get_children(element);
            let max_children = if depth < 2 { 50 } else { 20 };
            for (i, child) in children.iter().enumerate() {
                if i >= max_children { break; }
                if *element_count >= max_elements { break; }
                self.walk_uia_tree_shallow(
                    child, inputs, data_elements, actions, nav_sections,
                    depth + 1, max_depth, element_count, max_elements,
                );
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn find_field_value_uia(
        &self,
        element: &windows::Win32::UI::Accessibility::IUIAutomationElement,
        field_id: &str,
        inputs: &[WindowInput],
        depth: u32,
        max_depth: u32,
    ) -> Option<String> {
        if depth > max_depth { return None; }

        let identifier = uia::get_automation_id(element);

        // Direct AutomationId match.
        if identifier.as_deref() == Some(field_id) {
            return uia::get_value(element);
        }

        // Match by label (Name) - handles inputs with no AutomationId.
        if let Some(input) = inputs.iter().find(|i| i.id == field_id) {
            let label = uia::get_name(element);
            if !label.is_empty() && label == input.label {
                return uia::get_value(element);
            }
        }

        for child in uia::get_children(element) {
            if let Some(val) = self.find_field_value_uia(&child, field_id, inputs, depth + 1, max_depth) {
                return Some(val);
            }
        }
        None
    }

    #[cfg(target_os = "windows")]
    fn write_field_value_uia(
        &self,
        element: &windows::Win32::UI::Accessibility::IUIAutomationElement,
        field_id: &str,
        value: &str,
        inputs: &[WindowInput],
        depth: u32,
        max_depth: u32,
    ) -> bool {
        if depth > max_depth { return false; }

        let identifier = uia::get_automation_id(element);
        if identifier.as_deref() == Some(field_id) {
            return uia::set_value(element, value);
        }
        if let Some(input) = inputs.iter().find(|i| i.id == field_id) {
            let label = uia::get_name(element);
            if !label.is_empty() && label == input.label {
                return uia::set_value(element, value);
            }
        }
        for child in uia::get_children(element) {
            if self.write_field_value_uia(&child, field_id, value, inputs, depth + 1, max_depth) {
                return true;
            }
        }
        false
    }

    #[cfg(target_os = "windows")]
    fn find_and_trigger_uia(
        &self,
        element: &windows::Win32::UI::Accessibility::IUIAutomationElement,
        action_id: &str,
        actions: &[WindowAction],
        depth: u32,
        max_depth: u32,
    ) -> bool {
        if depth > max_depth { return false; }

        let identifier = uia::get_automation_id(element);
        let control_type = uia::get_control_type(element);

        if identifier.as_deref() == Some(action_id) {
            return uia::invoke(element);
        }
        if let Some(action) = actions.iter().find(|a| a.id == action_id) {
            let label = uia::get_name(element);
            if !label.is_empty() && label == action.label && control_type == uia::CT_BUTTON {
                return uia::invoke(element);
            }
        }
        for child in uia::get_children(element) {
            if self.find_and_trigger_uia(&child, action_id, actions, depth + 1, max_depth) {
                return true;
            }
        }
        false
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunningApp {
    pub name: String,
    pub pid: i64,
}

// ─────────────────────────────────────────────
//  Window text reading (FABLE A2 - Tier 2)
// ─────────────────────────────────────────────
//
// read_app_window: Bob names an app and gets the focused window's readable
// content as an indented text outline. This is the fallback tier when an
// app has no scripting bridge (app_bridge.rs). Unlike the profiler above,
// this walk descends into web areas, scroll areas, tables and lists -
// that is where the content lives - and relies on hard caps instead of
// role skipping to stay bounded. Runs in the same crash-isolated
// subprocess pattern as --wi-profile.

/// Hard caps for the content walk. A huge window must never flood the
/// model's context.
const READ_MAX_DEPTH: u32 = 40;
const READ_MAX_ELEMENTS: u32 = 4000;
const READ_MAX_LINES: usize = 400;
const READ_MAX_LINE_CHARS: usize = 300;
const READ_MAX_TOTAL_CHARS: usize = 16_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredWindow {
    pub app: String,
    pub pid: i64,
    pub window_title: String,
    pub line_count: usize,
    pub truncated: bool,
    /// Indented outline: one line per readable element, "role: text".
    pub text: String,
}

/// Snapshot of running apps, shared by the store method and the free
/// functions below.
pub fn running_apps() -> Vec<RunningApp> {
    WindowIntelligenceStore::default().list_running_apps()
}

/// True when this process can make accessibility calls (AX on macOS,
/// UI Automation on Windows - which has no per-app permission gate).
pub fn accessibility_trusted() -> bool {
    #[cfg(target_os = "macos")]
    {
        ax::is_trusted() || ax::test_access()
    }
    #[cfg(target_os = "windows")]
    {
        uia::is_trusted() && uia::test_access()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        false
    }
}

/// Find a running app by name: exact match first, then prefix, then
/// substring, case-insensitive throughout.
pub fn find_running_app(app_name: &str) -> Option<RunningApp> {
    let needle = app_name.trim().to_lowercase();
    let apps = running_apps();
    apps.iter()
        .find(|a| a.name.to_lowercase() == needle)
        .or_else(|| apps.iter().find(|a| a.name.to_lowercase().starts_with(&needle)))
        .or_else(|| apps.iter().find(|a| a.name.to_lowercase().contains(&needle)))
        .cloned()
}

/// Called from main() when --wi-read-text is passed: do the walk in an
/// isolated process and print JSON, so an AX crash cannot take down the
/// app.
#[cfg(target_os = "macos")]
pub fn run_read_text_subprocess(pid: i64, app_name: String) -> Result<(), String> {
    let win = read_window_text_direct(pid, &app_name)?;
    let json = serde_json::to_string(&win).map_err(|e| e.to_string())?;
    print!("{}", json);
    Ok(())
}

#[cfg(target_os = "macos")]
fn read_window_text_direct(pid: i64, app_name: &str) -> Result<StructuredWindow, String> {
    let app_el = ax::get_app_element(pid as i32).ok_or_else(|| {
        format!(
            "Could not access {}. Make sure it is running and Orbix Core has Accessibility access.",
            app_name
        )
    })?;

    let window_el = ax_focused_window_ready(app_el);

    let window_title = ax::get_attribute_string(window_el, "AXTitle").unwrap_or_default();

    let mut lines: Vec<String> = Vec::new();
    let mut visited: u32 = 0;
    let mut total_chars: usize = 0;
    collect_window_text(window_el, 0, &mut lines, &mut visited, &mut total_chars);

    let truncated = visited >= READ_MAX_ELEMENTS
        || lines.len() >= READ_MAX_LINES
        || total_chars >= READ_MAX_TOTAL_CHARS;

    Ok(StructuredWindow {
        app: app_name.to_string(),
        pid,
        window_title,
        line_count: lines.len(),
        truncated,
        text: lines.join("\n"),
    })
}

#[cfg(target_os = "macos")]
fn collect_window_text(
    element: ax::AXUIElementRef,
    depth: u32,
    lines: &mut Vec<String>,
    visited: &mut u32,
    total_chars: &mut usize,
) {
    if depth > READ_MAX_DEPTH
        || *visited >= READ_MAX_ELEMENTS
        || lines.len() >= READ_MAX_LINES
        || *total_chars >= READ_MAX_TOTAL_CHARS
        || element.is_null()
    {
        return;
    }
    *visited += 1;

    let role = match ax::get_attribute_string(element, "AXRole") {
        Some(r) => r,
        None => return,
    };

    // Never read secrets; never dump the menu bar (hundreds of items of
    // chrome, zero content).
    if role == "AXSecureTextField" || role == "AXMenuBar" {
        return;
    }

    let label = ax::get_attribute_string(element, "AXTitle")
        .or_else(|| ax::get_attribute_string(element, "AXDescription"))
        .unwrap_or_default();
    let value = ax::get_attribute_string(element, "AXValue").unwrap_or_default();

    let content = match (label.trim(), value.trim()) {
        ("", "") => String::new(),
        (l, "") => l.to_string(),
        ("", v) => v.to_string(),
        (l, v) if l == v => l.to_string(),
        (l, v) => format!("{}: {}", l, v),
    };

    if !content.is_empty() {
        let short_role = role.trim_start_matches("AX").to_lowercase();
        let mut line = format!(
            "{}{}: {}",
            "  ".repeat((depth as usize).min(8)),
            short_role,
            content.replace(['\n', '\r'], " ")
        );
        if line.chars().count() > READ_MAX_LINE_CHARS {
            line = line.chars().take(READ_MAX_LINE_CHARS).collect();
        }
        // Lists repeat structure; drop consecutive duplicates.
        if lines.last().map(|l| l.trim()) != Some(line.trim()) {
            *total_chars += line.len();
            lines.push(line);
        }
    }

    let (arr_owner, children) = ax::get_children_with_owner(element);
    for child in &children {
        if *visited >= READ_MAX_ELEMENTS
            || lines.len() >= READ_MAX_LINES
            || *total_chars >= READ_MAX_TOTAL_CHARS
        {
            break;
        }
        collect_window_text(*child, depth + 1, lines, visited, total_chars);
    }
    if !arr_owner.is_null() {
        unsafe {
            ax::release(arr_owner);
        }
    }
}

/// Windows subprocess entry for --wi-read-text: same crash isolation as
/// the macOS path (a COM/UIA fault cannot take down the main app).
#[cfg(target_os = "windows")]
pub fn run_read_text_subprocess(pid: i64, app_name: String) -> Result<(), String> {
    let win = read_window_text_direct(pid, &app_name)?;
    let json = serde_json::to_string(&win).map_err(|e| e.to_string())?;
    print!("{}", json);
    Ok(())
}

#[cfg(target_os = "windows")]
fn read_window_text_direct(pid: i64, app_name: &str) -> Result<StructuredWindow, String> {
    let window_el = uia::get_app_element(pid as u32).ok_or_else(|| {
        format!(
            "Could not access {}. Make sure it is running and has a visible window.",
            app_name
        )
    })?;

    let window_title = uia::get_name(&window_el);

    let mut lines: Vec<String> = Vec::new();
    let mut visited: u32 = 0;
    let mut total_chars: usize = 0;
    collect_window_text_uia(&window_el, 0, &mut lines, &mut visited, &mut total_chars);

    let truncated = visited >= READ_MAX_ELEMENTS
        || lines.len() >= READ_MAX_LINES
        || total_chars >= READ_MAX_TOTAL_CHARS;

    Ok(StructuredWindow {
        app: app_name.to_string(),
        pid,
        window_title,
        line_count: lines.len(),
        truncated,
        text: lines.join("\n"),
    })
}

/// UIA mirror of collect_window_text: one indented "role: text" line per
/// readable element, hard-capped so a huge window never floods context.
#[cfg(target_os = "windows")]
fn collect_window_text_uia(
    element: &windows::Win32::UI::Accessibility::IUIAutomationElement,
    depth: u32,
    lines: &mut Vec<String>,
    visited: &mut u32,
    total_chars: &mut usize,
) {
    if depth > READ_MAX_DEPTH
        || *visited >= READ_MAX_ELEMENTS
        || lines.len() >= READ_MAX_LINES
        || *total_chars >= READ_MAX_TOTAL_CHARS
    {
        return;
    }
    *visited += 1;

    let control_type = uia::get_control_type(element);

    // Skip window chrome; password boxes need no special case here because
    // UIA's ValuePattern refuses to expose their content (get_value = None).
    if control_type == uia::CT_TITLEBAR || control_type == uia::CT_MENU_BAR {
        return;
    }

    let label = uia::get_name(element);
    let value = uia::get_value(element).unwrap_or_default();

    let content = match (label.trim(), value.trim()) {
        ("", "") => String::new(),
        (l, "") => l.to_string(),
        ("", v) => v.to_string(),
        (l, v) if l == v => l.to_string(),
        (l, v) => format!("{}: {}", l, v),
    };

    if !content.is_empty() {
        let short_role = uia::control_type_label(control_type);
        let mut line = format!(
            "{}{}: {}",
            "  ".repeat((depth as usize).min(8)),
            short_role,
            content.replace(['\n', '\r'], " ")
        );
        if line.chars().count() > READ_MAX_LINE_CHARS {
            line = line.chars().take(READ_MAX_LINE_CHARS).collect();
        }
        // Lists repeat structure; drop consecutive duplicates.
        if lines.last().map(|l| l.trim()) != Some(line.trim()) {
            *total_chars += line.len();
            lines.push(line);
        }
    }

    for child in uia::get_children(element) {
        if *visited >= READ_MAX_ELEMENTS
            || lines.len() >= READ_MAX_LINES
            || *total_chars >= READ_MAX_TOTAL_CHARS
        {
            break;
        }
        collect_window_text_uia(&child, depth + 1, lines, visited, total_chars);
    }
}

/// The Bob-facing entry point: resolve the app, check Accessibility, and
/// read its focused window in a crash-isolated subprocess. Every failure
/// is an actionable message.
pub async fn read_app_window(app_name: &str) -> Result<StructuredWindow, String> {
    if app_name.trim().is_empty() {
        return Err("read_app_window needs an app name.".to_string());
    }
    if !accessibility_trusted() {
        #[cfg(target_os = "macos")]
        return Err(
            "Accessibility access is not granted, so Bob cannot read app windows. \
             Grant it in System Settings > Privacy & Security > Accessibility (enable Orbix Core). \
             Bob can open that pane: shell_exec `open \"x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility\"`."
                .to_string(),
        );
        #[cfg(not(target_os = "macos"))]
        return Err(
            "UI Automation is not available right now, so Bob cannot read app windows."
                .to_string(),
        );
    }
    let app = find_running_app(app_name).ok_or_else(|| {
        if cfg!(target_os = "windows") {
            format!(
                "{} is not running, so there is no window to read. Open it first (shell_exec `start \"\" \"{}\"`), or use browse_read instead.",
                app_name, app_name
            )
        } else {
            format!(
                "{} is not running, so there is no window to read. Open it first (shell_exec `open -a \"{}\"`), or use app_bridge / browse_read instead.",
                app_name, app_name
            )
        }
    })?;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let output = crate::process_ext::tokio_command(&exe)
        .arg("--wi-read-text")
        .arg(app.pid.to_string())
        .arg(&app.name)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("Failed to spawn reader subprocess: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        return Err(format!(
            "Reading the {} window failed: {}",
            app.name,
            if stderr.is_empty() { "the app's accessibility tree was not readable" } else { stderr }
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str::<StructuredWindow>(&stdout)
        .map_err(|e| format!("Failed to parse window text result: {}", e))
}

// Live acceptance harness (FABLE A2). Ignored by default - needs
// Accessibility granted to the test process:
//   cargo test --bin orbixcore -- --ignored read_app_window_live --nocapture
#[cfg(all(test, target_os = "macos"))]
mod read_live {
    use super::*;

    #[test]
    #[ignore]
    fn read_app_window_live() {
        if !accessibility_trusted() {
            eprintln!("[skip] accessibility not granted to the test process");
            return;
        }
        // Read whichever of these is running; Finder always is.
        let target = ["Microsoft Outlook", "Notion", "Finder"]
            .iter()
            .find_map(|n| find_running_app(n))
            .expect("Finder should always be running");
        let win = read_window_text_direct(target.pid, &target.name).expect("read failed");
        eprintln!(
            "[ok] {} \"{}\" ({} lines, truncated={})\n{}",
            win.app, win.window_title, win.line_count, win.truncated,
            win.text.lines().take(40).collect::<Vec<_>>().join("\n")
        );
        assert!(win.line_count > 0 || win.text.is_empty());
    }
}

// ─────────────────────────────────────────────
//  Pair mode helpers (peer.rs): probe, send, read by pid
// ─────────────────────────────────────────────
//
// The accessibility rung of the pairing ladder. Same crash isolation as
// the profiler: every AX walk runs in a child process (--wi-ax-probe,
// --wi-ax-send, --wi-read-text) so a misbehaving app cannot take the
// desktop down with it.

/// What the accessibility probe found in an app's front window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxProbe {
    pub readable: bool,
    /// Description of the text input Bob would type into, if any.
    pub text_input: Option<String>,
    pub window_title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxSendReport {
    /// In Bob's words: where the text went and how it was sent.
    pub detail: String,
    pub sent_with: String,
}

/// Show the OS Accessibility prompt if the app is not trusted yet.
pub fn request_accessibility_prompt() -> bool {
    #[cfg(target_os = "macos")]
    {
        ax::request_access()
    }
    #[cfg(not(target_os = "macos"))]
    {
        accessibility_trusted()
    }
}

/// Read the front window of a running app by pid (crash-isolated).
pub async fn read_window_by_pid(pid: i64, app_name: &str) -> Result<StructuredWindow, String> {
    if !accessibility_trusted() {
        return Err("Accessibility access is not granted, so Bob cannot read app windows.".to_string());
    }
    let out = run_wi_subprocess("--wi-read-text", pid, app_name, None).await?;
    serde_json::from_str::<StructuredWindow>(&out)
        .map_err(|e| format!("Failed to parse window text result: {}", e))
}

/// Probe an app's front window for readability and a text input. Reads
/// only; never focuses, types, or clicks.
pub async fn ax_probe(pid: i64, app_name: &str) -> Result<AxProbe, String> {
    let out = run_wi_subprocess("--wi-ax-probe", pid, app_name, None).await?;
    serde_json::from_str::<AxProbe>(&out).map_err(|e| format!("Failed to parse probe result: {}", e))
}

/// Put text into the app's message input and send it (Send button or
/// the input's confirm action). Fails loudly when neither works so the
/// caller can fall back to the clipboard.
pub async fn ax_send_text(pid: i64, app_name: &str, text: &str) -> Result<AxSendReport, String> {
    let out = run_wi_subprocess("--wi-ax-send", pid, app_name, Some(text)).await?;
    serde_json::from_str::<AxSendReport>(&out).map_err(|e| format!("Failed to parse send result: {}", e))
}

async fn run_wi_subprocess(mode: &str, pid: i64, app_name: &str, stdin_text: Option<&str>) -> Result<String, String> {
    use tokio::io::AsyncWriteExt;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = crate::process_ext::tokio_command(&exe);
    cmd.arg(mode)
        .arg(pid.to_string())
        .arg(app_name)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    cmd.stdin(if stdin_text.is_some() { std::process::Stdio::piped() } else { std::process::Stdio::null() });
    let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn accessibility subprocess: {}", e))?;
    if let Some(text) = stdin_text {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes()).await;
            let _ = stdin.shutdown().await;
        }
    }
    let output = tokio::time::timeout(std::time::Duration::from_secs(25), child.wait_with_output())
        .await
        .map_err(|_| format!("{} took too long to answer", app_name))?
        .map_err(|e| format!("accessibility subprocess: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        return Err(if stderr.is_empty() {
            format!("the {} window was not reachable through Accessibility", app_name)
        } else {
            stderr.to_string()
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Child-process entry for --wi-ax-probe.
#[cfg(target_os = "macos")]
pub fn run_ax_probe_subprocess(pid: i64, app_name: String) -> Result<(), String> {
    let probe = ax_probe_direct(pid, &app_name)?;
    print!("{}", serde_json::to_string(&probe).map_err(|e| e.to_string())?);
    Ok(())
}

/// Child-process entry for --wi-ax-send: the text arrives on stdin.
#[cfg(target_os = "macos")]
pub fn run_ax_send_subprocess(pid: i64, app_name: String) -> Result<(), String> {
    use std::io::Read;
    let mut text = String::new();
    std::io::stdin().read_to_string(&mut text).map_err(|e| e.to_string())?;
    let report = ax_send_direct(pid, &app_name, text.trim_end_matches('\n'))?;
    print!("{}", serde_json::to_string(&report).map_err(|e| e.to_string())?);
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn run_ax_probe_subprocess(_pid: i64, app_name: String) -> Result<(), String> {
    // UI Automation send/probe lands with the Windows pass; reading works
    // through --wi-read-text already, so a Windows peer is read-only here.
    let probe = AxProbe { readable: true, text_input: None, window_title: app_name };
    print!("{}", serde_json::to_string(&probe).map_err(|e| e.to_string())?);
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn run_ax_send_subprocess(_pid: i64, app_name: String) -> Result<(), String> {
    Err(format!("typing into {} through UI Automation is not wired on Windows yet", app_name))
}

/// Chromium and Electron apps (ChatGPT, Claude Desktop, Slack, VS Code,
/// Cursor) build their accessibility tree lazily: until an assistive
/// client announces itself the app element has no children at all. Setting
/// AXManualAccessibility (Electron) / AXEnhancedUserInterface (Chromium)
/// on the app element switches the tree on; it then takes a moment to
/// appear, so wait briefly for a focused window. Native apps ignore both
/// attributes and answer immediately.
#[cfg(target_os = "macos")]
pub(crate) fn ax_focused_window_ready(app_el: ax::AXUIElementRef) -> ax::AXUIElementRef {
    let pick = || {
        ax::get_attribute_value(app_el, "AXFocusedWindow")
            .or_else(|| ax::get_attribute_value(app_el, "AXMainWindow"))
            .or_else(|| {
                // Not the active app: AppKit reports no focused window, but
                // the window list is still there. Take the frontmost one.
                let list = ax::get_attribute_value(app_el, "AXWindows")?;
                let (owner, wins) = ax::get_children_with_owner_of_list(list);
                let first = wins.first().copied();
                // The list owns the elements; keep the array alive for the
                // caller's lifetime by leaking it (one small array per read
                // in a short-lived child process).
                let _ = owner;
                first
            })
    };
    if let Some(w) = pick() {
        // A window that already has children needs no switch-on.
        let (owner, children) = ax::get_children_with_owner(w);
        let populated = !children.is_empty();
        if !owner.is_null() {
            unsafe { ax::release(owner) };
        }
        if populated {
            return w;
        }
    }
    let _ = ax::set_attribute_bool(app_el, "AXManualAccessibility", true);
    let _ = ax::set_attribute_bool(app_el, "AXEnhancedUserInterface", true);
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
    loop {
        if let Some(w) = pick() {
            let (owner, children) = ax::get_children_with_owner(w);
            let populated = !children.is_empty();
            if !owner.is_null() {
                unsafe { ax::release(owner) };
            }
            if populated || std::time::Instant::now() >= deadline {
                return w;
            }
        } else if std::time::Instant::now() >= deadline {
            return app_el;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// Diagnostic entry (--wi-ax-dump): what an app exposes through
/// Accessibility, as this process sees it. Prints to stderr; never used by
/// Bob. Run it alone: two clients walking one app at once confuse the
/// numbers.
#[cfg(target_os = "macos")]
pub fn run_ax_dump_subprocess(pid: i64, app_name: String) -> Result<(), String> {
    let app_el = ax::get_app_element(pid as i32).ok_or_else(|| format!("no app element for {}", app_name))?;
    eprintln!("app {:p} trusted={} hidden={:?}", app_el, ax::is_trusted(), ax::get_attribute_bool(app_el, "AXHidden"));
    let Some(list) = ax::get_attribute_value(app_el, "AXWindows") else {
        eprintln!("AXWindows: none");
        return Ok(());
    };
    let (owner, wins) = ax::get_children_with_owner_of_list(list);
    eprintln!("AXWindows: {}", wins.len());
    fn walk(el: ax::AXUIElementRef, depth: u32, roles: &mut std::collections::BTreeMap<String, usize>, n: &mut u32, max_depth: &mut u32, inputs: &mut Vec<String>) {
        if depth > AX_WALK_MAX_DEPTH || *n > AX_WALK_MAX_ELEMENTS { return; }
        *n += 1;
        *max_depth = (*max_depth).max(depth);
        let r = ax::get_attribute_string(el, "AXRole").unwrap_or_else(|| "?".into());
        if AX_TEXT_ROLES.contains(&r.as_str()) {
            let (label, score) = ax_score_input(el, &r);
            inputs.push(format!("d{} {} \"{}\" score={}", depth, r, label, score));
        }
        *roles.entry(r).or_insert(0) += 1;
        let (o, kids) = ax::get_children_with_owner(el);
        for k in &kids { walk(*k, depth + 1, roles, n, max_depth, inputs); }
        if !o.is_null() { unsafe { ax::release(o) }; }
    }
    for (i, w) in wins.iter().enumerate().take(4) {
        let role = ax::get_attribute_string(*w, "AXRole").unwrap_or_default();
        let title = ax::get_attribute_string(*w, "AXTitle").unwrap_or_default();
        let mut roles = std::collections::BTreeMap::new();
        let (mut n, mut max_depth, mut inputs) = (0, 0, Vec::new());
        walk(*w, 0, &mut roles, &mut n, &mut max_depth, &mut inputs);
        eprintln!("window {} role={} title={:?} visited={} max_depth={}", i, role, title, n, max_depth);
        eprintln!("  census={:?}", roles);
        for inp in inputs { eprintln!("  input {}", inp); }
    }
    if !owner.is_null() { unsafe { ax::release(owner) }; }
    Ok(())
}

/// The roles that take typed text. Secure fields are never touched.
#[cfg(target_os = "macos")]
const AX_TEXT_ROLES: &[&str] = &["AXTextArea", "AXTextField", "AXComboBox"];
#[cfg(target_os = "macos")]
const AX_WALK_MAX_DEPTH: u32 = 40;
#[cfg(target_os = "macos")]
const AX_WALK_MAX_ELEMENTS: u32 = 5000;

#[cfg(target_os = "macos")]
struct AxCandidate {
    element: ax::AXUIElementRef,
    role: String,
    label: String,
    score: i32,
}

#[cfg(target_os = "macos")]
fn ax_front_window(pid: i64, app_name: &str) -> Result<(ax::AXUIElementRef, String), String> {
    let app_el = ax::get_app_element(pid as i32).ok_or_else(|| {
        format!(
            "Could not access {}. Make sure it is running and Orbix Core has Accessibility access.",
            app_name
        )
    })?;
    let window_el = ax_focused_window_ready(app_el);
    if window_el == app_el {
        return Err(format!("{} has no window open", app_name));
    }
    let title = ax::get_attribute_string(window_el, "AXTitle").unwrap_or_default();
    Ok((window_el, title))
}

/// Score a text input for "this is where you talk to the app". Focused
/// wins, then a chat-like placeholder, then a multi-line area. Search
/// and address fields score below zero.
#[cfg(target_os = "macos")]
fn ax_score_input(element: ax::AXUIElementRef, role: &str) -> (String, i32) {
    let placeholder = ax::get_attribute_string(element, "AXPlaceholderValue").unwrap_or_default();
    let title = ax::get_attribute_string(element, "AXTitle").unwrap_or_default();
    let desc = ax::get_attribute_string(element, "AXDescription").unwrap_or_default();
    let label = [placeholder.as_str(), title.as_str(), desc.as_str()]
        .iter()
        .find(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let hay = format!("{} {} {}", placeholder, title, desc).to_lowercase();
    let mut score = 0;
    if ax::get_attribute_bool(element, "AXFocused").unwrap_or(false) {
        score += 4;
    }
    if ["message", "ask", "prompt", "write", "reply", "chat", "talk", "type", "send", "work with"]
        .iter()
        .any(|k| hay.contains(k))
    {
        score += 3;
    }
    if role == "AXTextArea" {
        score += 1;
    }
    if ["search", "find", "filter", "url", "address", "name", "title", "password"]
        .iter()
        .any(|k| hay.contains(k))
    {
        score -= 4;
    }
    if ax::get_attribute_bool(element, "AXEnabled") == Some(false) {
        score -= 10;
    }
    (label, score)
}

#[cfg(target_os = "macos")]
fn ax_collect(
    element: ax::AXUIElementRef,
    depth: u32,
    visited: &mut u32,
    inputs: &mut Vec<AxCandidate>,
    buttons: &mut Vec<AxCandidate>,
    readable_lines: &mut u32,
) {
    if depth > AX_WALK_MAX_DEPTH || *visited >= AX_WALK_MAX_ELEMENTS || element.is_null() {
        return;
    }
    *visited += 1;
    let Some(role) = ax::get_attribute_string(element, "AXRole") else { return };
    if role == "AXMenuBar" || role == "AXSecureTextField" {
        return;
    }
    let editable_group = role == "AXGroup" && {
        let rd = ax::get_attribute_string(element, "AXRoleDescription").unwrap_or_default().to_lowercase();
        rd.contains("text entry") || rd.contains("editable") || rd.contains("text field") || rd.contains("text area")
    };
    if AX_TEXT_ROLES.contains(&role.as_str()) || editable_group {
        let (label, score) = ax_score_input(element, &role);
        inputs.push(AxCandidate { element, role: role.clone(), label, score });
    } else if role == "AXButton" {
        let title = ax::get_attribute_string(element, "AXTitle").unwrap_or_default();
        let desc = ax::get_attribute_string(element, "AXDescription").unwrap_or_default();
        let ident = ax::get_attribute_string(element, "AXIdentifier").unwrap_or_default();
        let hay = format!("{} {} {}", title, desc, ident).to_lowercase();
        let label = if !title.is_empty() { title } else if !desc.is_empty() { desc } else { ident };
        let mut score = 0;
        if hay.split(|c: char| !c.is_alphanumeric()).any(|w| matches!(w, "send" | "submit" | "post" | "reply")) {
            score += 3;
        }
        if hay.contains("send") {
            score += 1;
        }
        if ax::get_attribute_bool(element, "AXEnabled") == Some(false) {
            score -= 10;
        }
        if score > 0 {
            buttons.push(AxCandidate { element, role, label, score });
        }
    } else if role == "AXStaticText" || role == "AXHeading" {
        if ax::get_attribute_string(element, "AXValue").map(|v| !v.trim().is_empty()).unwrap_or(false) {
            *readable_lines += 1;
        }
    }
    let (arr_owner, children) = ax::get_children_with_owner(element);
    for child in &children {
        if *visited >= AX_WALK_MAX_ELEMENTS {
            break;
        }
        ax_collect(*child, depth + 1, visited, inputs, buttons, readable_lines);
    }
    if !arr_owner.is_null() {
        unsafe { ax::release(arr_owner) };
    }
}

#[cfg(target_os = "macos")]
fn ax_probe_direct(pid: i64, app_name: &str) -> Result<AxProbe, String> {
    let (window_el, title) = ax_front_window(pid, app_name)?;
    let mut visited = 0;
    let mut inputs = Vec::new();
    let mut buttons = Vec::new();
    let mut readable = 0;
    ax_collect(window_el, 0, &mut visited, &mut inputs, &mut buttons, &mut readable);
    inputs.sort_by(|a, b| b.score.cmp(&a.score));
    let text_input = inputs.first().filter(|c| c.score >= 0).map(|c| {
        if c.label.is_empty() {
            format!("{} field", c.role.trim_start_matches("AX").to_lowercase())
        } else {
            c.label.clone()
        }
    });
    Ok(AxProbe {
        readable: readable > 0 || visited > 1,
        text_input,
        window_title: title,
    })
}

#[cfg(target_os = "macos")]
fn ax_send_direct(pid: i64, app_name: &str, text: &str) -> Result<AxSendReport, String> {
    let (window_el, _title) = ax_front_window(pid, app_name)?;
    let mut visited = 0;
    let mut inputs = Vec::new();
    let mut buttons = Vec::new();
    let mut readable = 0;
    ax_collect(window_el, 0, &mut visited, &mut inputs, &mut buttons, &mut readable);
    inputs.sort_by(|a, b| b.score.cmp(&a.score));
    let target = inputs
        .into_iter()
        .find(|c| c.score >= 0)
        .ok_or_else(|| format!("{} has no text input I can type into", app_name))?;

    let _ = ax::set_attribute_bool(target.element, "AXFocused", true);
    if !ax::set_attribute_string(target.element, "AXValue", text) {
        return Err(format!(
            "{} did not accept text in its {}",
            app_name,
            if target.label.is_empty() { "input".to_string() } else { format!("\"{}\" field", target.label) }
        ));
    }
    // Verify the app really took it: some fields report success and ignore.
    let now = ax::get_attribute_string(target.element, "AXValue").unwrap_or_default();
    if !now.contains(text.trim()) && !text.trim().is_empty() {
        return Err(format!("{} ignored the text I put in its input", app_name));
    }

    // Send: buttons can enable only once text is present, so look again.
    let mut visited2 = 0;
    let mut inputs2 = Vec::new();
    let mut buttons2 = Vec::new();
    let mut readable2 = 0;
    ax_collect(window_el, 0, &mut visited2, &mut inputs2, &mut buttons2, &mut readable2);
    buttons2.sort_by(|a, b| b.score.cmp(&a.score));
    let field = if target.label.is_empty() { "its input".to_string() } else { format!("\"{}\"", target.label) };
    if let Some(btn) = buttons2.first() {
        if ax::perform_action(btn.element, "AXPress") {
            return Ok(AxSendReport {
                detail: format!("Typed into {} in {} and pressed {}.", field, app_name, if btn.label.is_empty() { "Send".to_string() } else { btn.label.clone() }),
                sent_with: "button".to_string(),
            });
        }
    }
    if ax::perform_action(target.element, "AXConfirm") {
        return Ok(AxSendReport {
            detail: format!("Typed into {} in {} and confirmed it.", field, app_name),
            sent_with: "confirm".to_string(),
        });
    }
    Ok(AxSendReport {
        detail: format!("Typed into {} in {}. I could not find a Send button, so press Enter there to send it.", field, app_name),
        sent_with: "none".to_string(),
    })
}
