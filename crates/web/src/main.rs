use ratzilla::{
    SelectionMode, WebGl2Backend,
    backend::webgl2::{FontAtlasData, WebGl2BackendOptions},
    event::{MouseEventKind, ScrollDelta},
    ratatui::Terminal,
};
use std::io;
use web_sys::{console, wasm_bindgen::JsValue};

use ratzilla::{DomBackend, WebRenderer};
use tui_app::{Tui, tui::KeyCode};

mod utils;

use crate::utils::{BufferedKeyEvents, log_err};

const EXAMPLE_TRACE: &str = include_str!("../example_trace.json");

fn main() -> io::Result<()> {
    let backend = WebGl2Backend::new_with_options(
        WebGl2BackendOptions::new()
            .enable_mouse_selection_with_mode(SelectionMode::Linear)
            .font_atlas(
                FontAtlasData::from_binary(include_bytes!("../RecMonoCasual.atlas"))
                    .inspect_err(|err| log_err("error loadng font atlas", Some(err)))
                    .expect("Failed to load font atlas"),
            ),
    )?;
    // let backend = DomBackend::new()?;
    let terminal = Terminal::new(backend)?;

    let data = serde_json::from_str(EXAMPLE_TRACE)?;
    let (sender, receiver) = std::sync::mpsc::channel::<Vec<KeyCode>>();
    let mut tui = Tui::new(data);
    let debounced = BufferedKeyEvents::new(sender);

    terminal.on_key_event({
        let clone = debounced.clone();
        move |key_event| {
            if let Ok(key_code) = key_event.code.try_into() {
                clone.push(key_code);
            }
        }
    });

    terminal.on_wheel_event({
        let clone = debounced.clone();
        move |mouse_event| {
            if let MouseEventKind::ScrolledVertical(scroll) = mouse_event.event {
                let keycode = match scroll {
                    ScrollDelta::Pages(0..)
                    | ScrollDelta::Lines(0..)
                    | ScrollDelta::Pixels(0..) => KeyCode::Char('J'),
                    ScrollDelta::Pages(..=-1)
                    | ScrollDelta::Lines(..=-1)
                    | ScrollDelta::Pixels(..=-1) => KeyCode::Char('K'),
                };
                clone.push(keycode);
            }
        }
    });

    terminal.draw_web(move |f| {
        tui.draw(f)
            .inspect_err(|err| {
                console::error_1(&JsValue::from_str(&format!("Error drawing TUI: {err:#?}")))
            })
            .ok();
        if let Ok(key_codes) = receiver.try_recv() {
            // console::log_1(&JsValue::from_str(&format!(
            //     "received key event: {key_code:?}"
            // )));
            for key_code in key_codes {
                tui.on_key(key_code, f);
            }
        }
    });

    Ok(())
}
