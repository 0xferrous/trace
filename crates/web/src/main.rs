use futures::FutureExt;
use ratzilla::Theme;
use ratzilla::WebRenderer;
use ratzilla::ratatui::backend::Backend;
use ratzilla::{
    FontAtlasConfig, SelectionMode,
    backend::webgl2::WebGl2BackendOptions,
    event::{KeyCode, MouseEventKind, ScrollDelta},
    ratatui::style::Color,
};
use std::io;
use wasm_bindgen_futures::JsFuture;
use web_sys::window;
use web_sys::{console, wasm_bindgen::JsValue};

use trace_tui::{SelectionStyle, Tui};

mod utils;

use crate::utils::{BufferedKeyEvents, log_err, log_info};
use multi_backend::backend::{BackendType, MultiBackendBuilder};

const EXAMPLE_TRACE: &str = include_str!("../example_trace.json");

// Gruvbox Dark theme colors
const SELECTION_FG: Color = Color::Rgb(0xeb, 0xdb, 0xb2);
const SELECTION_BG: Color = Color::Rgb(0xd6, 0x5d, 0x0e);

async fn entrypoint() -> io::Result<()> {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));

    log_info("waiting for the fonts to load", None::<()>);

    let font = "Fira Code";
    async {
        let window = window().expect("window not found");
        let document = window.document().expect("document not found");
        let fonts = document.fonts();
        JsFuture::from(fonts.load(&format!("1em '{font}'")))
            .await
            .expect("error loading fira code");
        log_info("font loaded", None::<()>);
    }
    .await;

    log_info("starting app", None::<()>);

    let mut terminal = MultiBackendBuilder::with_fallback(BackendType::WebGl2)
        .webgl2_options(
            WebGl2BackendOptions::new()
                .enable_mouse_selection_with_mode(SelectionMode::Linear)
                .font_atlas_config(FontAtlasConfig::Dynamic(vec![font.to_string()], 16.)),
        )
        .theme(gruvbox_dark())
        .build_terminal()?;
    {
        terminal
            .backend_mut()
            .clear()
            .expect("unable to clear terminal");
    }

    let data = serde_json::from_str(EXAMPLE_TRACE)?;
    let (sender, receiver) = std::sync::mpsc::channel::<Vec<KeyCode>>();

    // Use Gruvbox Dark theme selection colors
    let selection_style = SelectionStyle {
        fg: Some(SELECTION_FG),
        bg: Some(SELECTION_BG),
    };
    let mut tui = Tui::with_selection_style(data, selection_style);
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

fn main() {
    wasm_bindgen_futures::spawn_local(entrypoint().map(|res| {
        if let Err(err) = res {
            log_err("entrypoint exited", Some(err));
        }
    }));
}

fn gruvbox_dark() -> Theme {
    Theme::builder()
        // ANSI colors (0-15)
        .palette_color(0, Color::Rgb(0x3c, 0x38, 0x36)) // color0 - dark gray
        .palette_color(1, Color::Rgb(0xcc, 0x24, 0x1d)) // color1 - red
        .palette_color(2, Color::Rgb(0x98, 0x97, 0x1a)) // color2 - green
        .palette_color(3, Color::Rgb(0xd7, 0x99, 0x21)) // color3 - yellow
        .palette_color(4, Color::Rgb(0x45, 0x85, 0x88)) // color4 - blue
        .palette_color(5, Color::Rgb(0xb1, 0x62, 0x86)) // color5 - magenta
        .palette_color(6, Color::Rgb(0x68, 0x9d, 0x6a)) // color6 - cyan
        .palette_color(7, Color::Rgb(0xa8, 0x99, 0x84)) // color7 - light gray
        .palette_color(8, Color::Rgb(0x92, 0x83, 0x74)) // color8 - bright black
        .palette_color(9, Color::Rgb(0xfb, 0x49, 0x34)) // color9 - bright red
        .palette_color(10, Color::Rgb(0xb8, 0xbb, 0x26)) // color10 - bright green
        .palette_color(11, Color::Rgb(0xfa, 0xbd, 0x2f)) // color11 - bright yellow
        .palette_color(12, Color::Rgb(0x83, 0xa5, 0x98)) // color12 - bright blue
        .palette_color(13, Color::Rgb(0xd3, 0x86, 0x9b)) // color13 - bright magenta
        .palette_color(14, Color::Rgb(0x8e, 0xc0, 0x7c)) // color14 - bright cyan
        .palette_color(15, Color::Rgb(0xfb, 0xf1, 0xc7)) // color15 - bright white
        // Default colors
        .foreground(Color::Rgb(0xeb, 0xdb, 0xb2)) // foreground
        .background(Color::Rgb(0x1d, 0x20, 0x21)) // background
        // Cursor colors
        .cursor_color(Color::Rgb(0xbd, 0xae, 0x93)) // cursor
        .cursor_text_color(Color::Rgb(0x66, 0x5c, 0x54)) // cursor_text_color
        // Selection colors (shared with TUI selection style)
        .selection_foreground(SELECTION_FG)
        .selection_background(SELECTION_BG)
        .build()
}
