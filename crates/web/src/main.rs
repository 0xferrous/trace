use ratzilla::ratatui::Terminal;
use std::io;
use web_sys::{console, wasm_bindgen::JsValue};

use ratzilla::{DomBackend, WebRenderer};
use tui_app::{Tui, tui::KeyCode};

const EXAMPLE_TRACE: &str = include_str!("../example_trace.json");

fn main() -> io::Result<()> {
    let backend = DomBackend::new()?;
    let terminal = Terminal::new(backend)?;

    let data = serde_json::from_str(EXAMPLE_TRACE)?;
    let (sender, receiver) = std::sync::mpsc::channel::<KeyCode>();
    let mut tui = Tui::new(data);

    terminal.on_key_event({
        let sender_cloned = sender.clone();
        move |key_event| {
            if let Ok(key_code) = key_event.code.try_into() {
                // console::log_1(&JsValue::from_str(&format!(
                //     "received key event: {key_code:?}"
                // )));
                sender_cloned
                    .send(key_code)
                    .inspect_err(|e| {
                        console::error_1(&JsValue::from_str(&format!(
                            "Error sending key event: {e:#?}"
                        )))
                    })
                    .ok();
            }
        }
    });

    terminal.draw_web(move |f| {
        tui.draw(f)
            .inspect_err(|err| {
                console::error_1(&JsValue::from_str(&format!("Error drawing TUI: {err:#?}")))
            })
            .ok();
        if let Ok(key_code) = receiver.try_recv() {
            // console::log_1(&JsValue::from_str(&format!(
            //     "received key event: {key_code:?}"
            // )));
            tui.on_key(key_code);
        }
    });

    Ok(())
}
