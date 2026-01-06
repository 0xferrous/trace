use std::io;

#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};

use ratatui::{
    Frame,
    layout::{Margin, Rect},
    text::Text,
    widgets::Paragraph,
};
use revm_inspectors::tracing::CallTraceArena;

use crate::traces::TracesState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    StepOver,
    ReverseStepOver,
    StepOut,
    StepInto,
    ToggleCollapse,
    ScrollDown,
    ScrollUp,
    GoToTop,
    GoToBottom,
    Up,
    Down,
    ScrollLeft,
    ScrollRight,
}

pub struct Tui {
    trace_state: TracesState,
    scroll_offset: (u16, u16),
    exit: bool,
    frames: u64,
    last_frames: u64,
    last_render: Instant,
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
        }
    }

    pub fn draw(&mut self, frame: &mut Frame) -> TuiResult<()> {
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
        Ok(())
    }

    pub fn on_key(&mut self, key: KeyCode, frame: &Frame) {
        if let Some(action) = Self::key_to_action(key) {
            self.dispatch(action, frame);
        }
    }

    fn key_to_action(key: KeyCode) -> Option<Action> {
        match key {
            KeyCode::Char('q') => Some(Action::Quit),
            KeyCode::Char('j') => Some(Action::StepOver),
            KeyCode::Char('k') => Some(Action::ReverseStepOver),
            KeyCode::Char('h') => Some(Action::StepOut),
            KeyCode::Char('l') => Some(Action::StepInto),
            KeyCode::Char(' ') => Some(Action::ToggleCollapse),
            KeyCode::Char('J') => Some(Action::ScrollDown),
            KeyCode::Char('K') => Some(Action::ScrollUp),
            KeyCode::Char('g') => Some(Action::GoToTop),
            KeyCode::Char('G') => Some(Action::GoToBottom),
            KeyCode::Up => Some(Action::Up),
            KeyCode::Down => Some(Action::Down),
            KeyCode::Left => Some(Action::ScrollLeft),
            KeyCode::Right => Some(Action::ScrollRight),
            _ => None,
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
}

#[derive(Debug)]
pub enum KeyCode {
    Up,
    Down,
    Left,
    Right,
    Char(char),
}

impl From<TuiError> for std::io::Error {
    fn from(err: TuiError) -> Self {
        match err {
            TuiError::IoError(err) => err,
            TuiError::ArbitraryError(report) => io::Error::other(format!("{}", report)),
        }
    }
}

#[cfg(feature = "crossterm")]
impl TryFrom<crossterm::event::KeyEvent> for KeyCode {
    type Error = String;

    fn try_from(value: crossterm::event::KeyEvent) -> Result<Self, Self::Error> {
        match value.code {
            crossterm::event::KeyCode::Up => Ok(Self::Up),
            crossterm::event::KeyCode::Down => Ok(Self::Down),
            crossterm::event::KeyCode::Left => Ok(Self::Left),
            crossterm::event::KeyCode::Right => Ok(Self::Right),
            crossterm::event::KeyCode::Char(c) => Ok(Self::Char(c)),
            _ => Err(format!("Unsupported key code: {:?}", value.code)),
        }
    }
}

#[cfg(feature = "ratzilla")]
impl TryFrom<ratzilla::event::KeyCode> for KeyCode {
    type Error = String;

    fn try_from(value: ratzilla::event::KeyCode) -> Result<Self, Self::Error> {
        match value {
            ratzilla::event::KeyCode::Up => Ok(Self::Up),
            ratzilla::event::KeyCode::Down => Ok(Self::Down),
            ratzilla::event::KeyCode::Left => Ok(Self::Left),
            ratzilla::event::KeyCode::Right => Ok(Self::Right),
            ratzilla::event::KeyCode::Char(c) => Ok(Self::Char(c)),
            _ => Err(format!("Unsupported key code: {:?}", value)),
        }
    }
}
