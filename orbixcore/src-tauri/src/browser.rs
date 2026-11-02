use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct BrowserSession {
    pub active: bool,
    pub current_url: String,
    pub page_title: String,
    pub status: BrowserStatus,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserStatus {
    Idle,
    Navigating,
    Ready,
    Error,
    Closed,
}

impl Default for BrowserSession {
    fn default() -> Self {
        Self {
            active: false,
            current_url: String::new(),
            page_title: String::new(),
            status: BrowserStatus::Closed,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct BrowserActionResult {
    pub success: bool,
    pub data: String,
    pub error: Option<String>,
}

// ─────────────────────────────────────────────
//  Browser Engine (headless_chrome feature)
// ─────────────────────────────────────────────

#[cfg(feature = "browser")]
mod engine {
    use super::*;
    use headless_chrome::{Browser, LaunchOptions};
    use std::time::Duration;

    pub struct BrowserEngine {
        browser: Option<Browser>,
        pub session: BrowserSession,
    }

    impl Default for BrowserEngine {
        fn default() -> Self {
            Self {
                browser: None,
                session: BrowserSession::default(),
            }
        }
    }

    impl BrowserEngine {
        pub fn launch(&mut self) -> Result<(), String> {
            if self.browser.is_some() {
                return Ok(());
            }

            let options = LaunchOptions::default_builder()
                .headless(true)
                .idle_browser_timeout(Duration::from_secs(300))
                .build()
                .map_err(|e| format!("Failed to build launch options: {}", e))?;

            let browser = Browser::new(options)
                .map_err(|e| format!("Failed to launch browser: {}", e))?;

            // Ensure at least one tab exists before proceeding
            let _tab = browser.new_tab()
                .map_err(|e| format!("Failed to create initial tab: {}", e))?;

            self.browser = Some(browser);
            self.session.active = true;
            self.session.status = BrowserStatus::Idle;
            Ok(())
        }

        pub fn close(&mut self) {
            self.browser = None;
            self.session = BrowserSession::default();
        }

        fn get_tab(&self) -> Result<Arc<headless_chrome::Tab>, String> {
            let browser = self.browser.as_ref().ok_or("Browser not launched")?;
            let tabs = browser.get_tabs().lock().map_err(|e| format!("Lock error: {}", e))?;
            if let Some(tab) = tabs.first().cloned() {
                return Ok(tab);
            }
            drop(tabs);
            // No tabs found - create a new one
            browser.new_tab()
                .map_err(|e| format!("Failed to create tab: {}", e))
        }

        pub fn navigate(&mut self, url: &str) -> Result<BrowserActionResult, String> {
            self.launch()?;
            self.session.status = BrowserStatus::Navigating;

            let tab = self.get_tab()?;
            tab.navigate_to(url)
                .map_err(|e| format!("Navigation failed: {}", e))?;
            tab.wait_until_navigated()
                .map_err(|e| format!("Wait failed: {}", e))?;

            self.session.current_url = url.to_string();
            self.session.status = BrowserStatus::Ready;

            // Try to get page title
            if let Ok(title) = tab.evaluate("document.title", false) {
                if let Some(val) = title.value {
                    self.session.page_title = val.as_str().unwrap_or("").to_string();
                }
            }

            Ok(BrowserActionResult {
                success: true,
                data: format!("Navigated to {}", url),
                error: None,
            })
        }

        pub fn click(&mut self, selector: &str) -> Result<BrowserActionResult, String> {
            let tab = self.get_tab()?;
            tab.find_element(selector)
                .map_err(|e| format!("Element not found: {}", e))?
                .click()
                .map_err(|e| format!("Click failed: {}", e))?;

            // Brief wait for page to react
            std::thread::sleep(Duration::from_millis(500));

            Ok(BrowserActionResult {
                success: true,
                data: format!("Clicked '{}'", selector),
                error: None,
            })
        }

        pub fn type_text(&mut self, selector: &str, text: &str) -> Result<BrowserActionResult, String> {
            let tab = self.get_tab()?;
            tab.find_element(selector)
                .map_err(|e| format!("Element not found: {}", e))?
                .click()
                .map_err(|e| format!("Focus failed: {}", e))?;
            tab.type_str(text)
                .map_err(|e| format!("Type failed: {}", e))?;

            Ok(BrowserActionResult {
                success: true,
                data: format!("Typed into '{}'", selector),
                error: None,
            })
        }

        pub fn screenshot(&mut self) -> Result<BrowserActionResult, String> {
            let tab = self.get_tab()?;
            let png_data = tab.capture_screenshot(
                headless_chrome::protocol::cdp::Page::CaptureScreenshotFormatOption::Png,
                None,
                None,
                true,
            ).map_err(|e| format!("Screenshot failed: {}", e))?;

            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&png_data);

            Ok(BrowserActionResult {
                success: true,
                data: b64,
                error: None,
            })
        }

        pub fn get_content(&mut self) -> Result<BrowserActionResult, String> {
            let tab = self.get_tab()?;

            // Get text content, stripping HTML
            let result = tab.evaluate(
                "document.body ? document.body.innerText : document.documentElement.innerText",
                false,
            ).map_err(|e| format!("Content extraction failed: {}", e))?;

            let text = result.value
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_default();

            // Truncate if very long
            let truncated = if text.len() > 30_000 {
                format!("{}...\n[truncated at 30KB]", &text[..30_000])
            } else {
                text
            };

            Ok(BrowserActionResult {
                success: true,
                data: truncated,
                error: None,
            })
        }

        pub fn evaluate_js(&mut self, script: &str) -> Result<BrowserActionResult, String> {
            let tab = self.get_tab()?;
            let result = tab.evaluate(script, false)
                .map_err(|e| format!("JS evaluation failed: {}", e))?;

            let output = result.value
                .map(|v| {
                    if let Some(s) = v.as_str() {
                        s.to_string()
                    } else {
                        v.to_string()
                    }
                })
                .unwrap_or_else(|| "undefined".into());

            Ok(BrowserActionResult {
                success: true,
                data: output,
                error: None,
            })
        }

        pub fn get_session(&self) -> BrowserSession {
            self.session.clone()
        }
    }
}

// ─────────────────────────────────────────────
//  Fallback (no browser feature)
// ─────────────────────────────────────────────

#[cfg(not(feature = "browser"))]
mod engine {
    use super::*;

    pub struct BrowserEngine {
        pub session: BrowserSession,
    }

    impl Default for BrowserEngine {
        fn default() -> Self {
            Self {
                session: BrowserSession::default(),
            }
        }
    }

    impl BrowserEngine {
        fn not_available() -> Result<BrowserActionResult, String> {
            Err("Browser automation requires the 'browser' feature. Rebuild with --features browser".into())
        }

        pub fn launch(&mut self) -> Result<(), String> { Self::not_available().map(|_| ()) }
        pub fn close(&mut self) { self.session = BrowserSession::default(); }
        pub fn navigate(&mut self, _url: &str) -> Result<BrowserActionResult, String> { Self::not_available() }
        pub fn click(&mut self, _selector: &str) -> Result<BrowserActionResult, String> { Self::not_available() }
        pub fn type_text(&mut self, _selector: &str, _text: &str) -> Result<BrowserActionResult, String> { Self::not_available() }
        pub fn screenshot(&mut self) -> Result<BrowserActionResult, String> { Self::not_available() }
        pub fn get_content(&mut self) -> Result<BrowserActionResult, String> { Self::not_available() }
        pub fn evaluate_js(&mut self, _script: &str) -> Result<BrowserActionResult, String> { Self::not_available() }
        pub fn get_session(&self) -> BrowserSession { self.session.clone() }
    }
}

pub use engine::BrowserEngine;
