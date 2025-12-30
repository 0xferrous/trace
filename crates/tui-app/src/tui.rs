use std::io;

use ratatui::{Frame, widgets::Paragraph};
use revm_inspectors::tracing::CallTraceArena;

use crate::traces::TracesState;

pub struct Tui {
    trace_state: TracesState,
    scroll_offset: (u16, u16),
    exit: bool,
}

impl Tui {
    pub fn new(trace_data: CallTraceArena) -> Self {
        Self {
            exit: false,
            scroll_offset: (0, 0),
            trace_state: TracesState::new(trace_data),
        }
    }

    pub fn draw(&self, frame: &mut Frame) -> TuiResult<()> {
        let result: TuiResult<_> = self.trace_state.to_text().map_err(|err| err.into());
        let text_widget = result?;
        let para = Paragraph::new(text_widget).scroll((self.scroll_offset.0, self.scroll_offset.1));
        frame.render_widget(para, frame.area());
        Ok(())
    }

    pub fn on_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') => self.exit = true,
            KeyCode::Char('j') => {
                self.trace_state.step_over();
            }
            KeyCode::Char('k') => {
                self.trace_state.reverse_step_over();
            }
            KeyCode::Char('h') => {
                self.trace_state.step_out();
            }
            KeyCode::Char('l') => {
                self.trace_state.step_into();
            }
            KeyCode::Char(' ') => {
                self.trace_state.toggle_collapse();
            }
            KeyCode::Up => {
                // if self.viewport_offset == 0 {
                self.scroll_offset.0 = self.scroll_offset.0.saturating_sub(1)
                // } else {
                //     self.viewport_offset -= 1;
                // }
            }
            KeyCode::Down => {
                // if self.viewport_offset == (height - 1) {
                self.scroll_offset.0 += 1
                // } else {
                //     self.viewport_offset += 1;
                // }
            }
            KeyCode::Left => self.scroll_offset.1 = self.scroll_offset.1.saturating_sub(1),
            KeyCode::Right => self.scroll_offset.1 += 1,
            _ => {}
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
