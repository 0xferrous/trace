use ratzilla::WebRenderer;
use ratzilla::{
    FontAtlasConfig, SelectionMode,
    backend::webgl2::WebGl2BackendOptions,
    event::{KeyCode, MouseEventKind, ScrollDelta},
};
use std::io;
use web_sys::{console, wasm_bindgen::JsValue};

use tui_app::Tui;

mod utils;

use crate::utils::BufferedKeyEvents;
use multi_backend::backend::{BackendType, MultiBackendBuilder};

const EXAMPLE_TRACE: &str = include_str!("../example_trace.json");

fn main() -> io::Result<()> {
    let terminal = MultiBackendBuilder::with_fallback(BackendType::WebGl2)
        .webgl2_options(
            WebGl2BackendOptions::new().enable_mouse_selection_with_mode(SelectionMode::Linear), // .font_atlas_config(FontAtlasConfig::Dynamic(vec!["Fira Code".to_string()], 30.)),
        )
        .build_terminal()?;

    let data = serde_json::from_str(EXAMPLE_TRACE)?;
    let (sender, receiver) = std::sync::mpsc::channel::<Vec<KeyCode>>();
    let mut tui = Tui::new(data);
    let debounced = BufferedKeyEvents::new(sender);

    terminal.on_key_event({
        let clone = debounced.clone();
        move |key_event| {
            clone.push(key_event.code);
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
