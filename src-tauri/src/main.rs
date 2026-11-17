// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Dev builds only: expose the WebView2 debugger so the DOM can be inspected
    // with any CDP client (needed to verify layout and catch render errors).
    #[cfg(debug_assertions)]
    if std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS").is_err() {
        std::env::set_var(
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
            "--remote-debugging-port=9333",
        );
    }

    sonic_lib::run();
}
