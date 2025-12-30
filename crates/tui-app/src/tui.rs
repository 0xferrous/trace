use std::{io, marker::PhantomData};

use ratatui::{Frame, Terminal, prelude::Backend, widgets::Paragraph};
use revm_inspectors::tracing::CallTraceArena;

use crate::traces::TracesState;

pub struct Tui<B>
where
    B: Backend,
    B::Error: From<TuiError<B>>,
{
    trace_state: TracesState,
    scroll_offset: (u16, u16),
    exit: bool,
    _backend: PhantomData<Terminal<B>>,
}

impl<B> Tui<B>
where
    B: Backend,
    B::Error: From<TuiError<B>>,
{
    pub fn new(trace_data: CallTraceArena) -> Self {
        Self {
            exit: false,
            scroll_offset: (0, 0),
            trace_state: TracesState::new(trace_data),
            _backend: Default::default(),
        }
    }

    pub fn draw(&self, frame: &mut Frame) -> TuiResult<(), B> {
        let result: TuiResult<_, _> = self.trace_state.to_text().map_err(|err| err.into());
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

pub type TuiResult<T, B> = Result<T, TuiError<B>>;

#[derive(thiserror::Error, Debug)]
pub enum TuiError<B: Backend> {
    #[error("Error drawing TUI: {0}")]
    DrawError(B::Error),
    #[error("Error reading TUI: {0}")]
    IoError(#[from] io::Error),
    #[error("Error: {0}")]
    ArbitraryError(#[from] eyre::Error),
}

pub enum KeyCode {
    Up,
    Down,
    Left,
    Right,
    Char(char),
}

#[cfg(feature = "crossterm")]
mod crossterm {
    use std::io::{self, Write};

    use ratatui::prelude::CrosstermBackend;

    use crate::tui::TuiError;

    impl TryFrom<crossterm::event::KeyEvent> for super::KeyCode {
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

    impl<W: Write> From<TuiError<CrosstermBackend<W>>> for std::io::Error {
        fn from(err: TuiError<CrosstermBackend<W>>) -> Self {
            match err {
                TuiError::DrawError(err) => err,
                TuiError::IoError(err) => err,
                TuiError::ArbitraryError(report) => io::Error::other(format!("{}", report)),
            }
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
