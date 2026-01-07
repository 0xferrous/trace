use std::io;

#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};

use ratatui::{
    Frame,
    layout::{HorizontalAlignment, Margin, Rect},
    text::Text,
    widgets::{Block, Clear, Paragraph},
};
use revm_inspectors::tracing::CallTraceArena;

use crate::{bindings::Action, traces::TracesState};

pub struct Tui {
    trace_state: TracesState,
    scroll_offset: (u16, u16),
    exit: bool,
    frames: u64,
    last_frames: u64,
    last_render: Instant,
    help: bool,
}

const ONE_SEC: Duration = Duration::from_secs(1);

impl Tui {
    pub fn new(trace_data: CallTraceArena) -> Self {
        Self {
            exit: false,
            scroll_offset: (0, 0),
            trace_state: TracesState::new(trace_data),
            frames: 0,
            last_frames: 0,
            last_render: Instant::now(),
            help: false,
        }
    }

    pub fn draw(&mut self, frame: &mut Frame) -> TuiResult<()> {
        // render the trace
        let result: TuiResult<_> = self.trace_state.to_text(true).map_err(|err| err.into());
        let text_widget = result?;
        let para = Paragraph::new(text_widget).scroll((self.scroll_offset.0, self.scroll_offset.1));
        frame.render_widget(
            para,
            frame.area().inner(Margin {
                horizontal: 0,
                vertical: 1,
            }),
        );

        // render the status line
        let now = Instant::now();
        if now.duration_since(self.last_render) > ONE_SEC {
            self.last_frames = self.frames;
            self.frames = 0;
            self.last_render = now;
        }

        let fps = Text::from(format!(
            "addr: {}, curr_idx: {}, order_idx: {:?}, fps: {}",
            self.trace_state.curr_address(),
            self.trace_state.curr_idx(),
            self.trace_state.active_item(),
            self.last_frames
        ))
        .alignment(ratatui::layout::HorizontalAlignment::Right);
        let frame_rect = frame.area();
        let last_line = Rect {
            x: 0,
            y: frame_rect.height - 1,
            height: 1,
            width: frame_rect.width,
        };
        frame.render_widget(fps, last_line);
        self.frames += 1;

        // render the help modal if open
        if self.help {
            let text = Action::to_text();
            let text_rect = text_to_encapsulating_rect(&text);
            let text_rect_centered = centered_rectangle(frame.area(), text_rect);
            let text_rect_margined = text_rect_centered.outer(Margin::new(1, 1));

            let block_rect = text_rect_margined.outer(Margin::new(1, 1));

            let block = Block::bordered()
                .title(" Help ")
                .title_alignment(HorizontalAlignment::Center);
            frame.render_widget(Clear, block_rect);
            frame.render_widget(block, block_rect);
            frame.render_widget(text, text_rect_centered);
        }

        Ok(())
    }

    pub fn on_key(&mut self, key: crate::bindings::KeyCode, frame: &Frame) {
        // any key press will close help
        self.help = false;

        if let Ok(action) = key.try_into() {
            self.dispatch(action, frame);
        }
    }

    pub fn dispatch(&mut self, action: Action, frame: &Frame) {
        let area = frame.area();
        match action {
            Action::Quit => self.exit = true,
            Action::StepOver => {
                self.trace_state.step_over();
            }
            Action::ReverseStepOver => {
                self.trace_state.reverse_step_over();
            }
            Action::StepOut => {
                self.trace_state.step_out();
            }
            Action::StepInto => {
                self.trace_state.step_into();
            }
            Action::ToggleCollapse => {
                self.trace_state.toggle_collapse();
            }
            Action::ScrollDown => {
                self.scroll_offset.0 += 1;
            }
            Action::ScrollUp => {
                self.scroll_offset.0 = self.scroll_offset.0.saturating_sub(1);
            }
            Action::GoToTop => {
                self.trace_state.first();
                self.scroll_offset.0 = 0;
            }
            Action::GoToBottom => {
                self.trace_state.last();
                let n_lines = self.trace_state.to_text(true).unwrap().lines.len();
                self.scroll_offset.0 = n_lines as u16 - area.height + 2;
            }
            Action::Up => {
                self.trace_state.up();
            }
            Action::Down => {
                self.trace_state.down();
            }
            Action::ScrollLeft => {
                self.scroll_offset.1 = self.scroll_offset.1.saturating_sub(1);
            }
            Action::ScrollRight => {
                self.scroll_offset.1 += 1;
            }
            Action::Help => self.help = true,
        }
    }

    pub fn exit(&self) -> bool {
        self.exit
    }
}

pub type TuiResult<T> = Result<T, TuiError>;

#[derive(thiserror::Error, Debug)]
pub enum TuiError {
    #[error("Error reading TUI: {0}")]
    IoError(#[from] io::Error),
    #[error("Error: {0}")]
    ArbitraryError(#[from] eyre::Error),
    #[error("Unknown keybind")]
    UnknownKeybindError,
}

impl From<TuiError> for std::io::Error {
    fn from(err: TuiError) -> Self {
        match err {
            TuiError::IoError(err) => err,
            TuiError::ArbitraryError(report) => io::Error::other(format!("{}", report)),
            TuiError::UnknownKeybindError => {
                io::Error::new(io::ErrorKind::Other, "Unknown keybind")
            }
        }
    }
}

/// Calculate the rectangle size needed to fully render a text widget
fn text_to_encapsulating_rect(text: &Text<'_>) -> Rect {
    let height = text.lines.len() as u16;
    let width = text
        .lines
        .iter()
        .map(|l| l.spans.iter().map(|sp| sp.content.len() as u16).sum())
        .max()
        .unwrap_or_default();
    Rect {
        height,
        width,
        ..Default::default()
    }
}

fn centered_rectangle(container: Rect, content: Rect) -> Rect {
    let x = (container.width - content.width) / 2;
    let y = (container.height - content.height) / 2;
    Rect { x, y, ..content }
}
