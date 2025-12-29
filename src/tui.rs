use std::io;

use crossterm::event::{self, Event, KeyCode};
use foundry_evm_traces::CallTraceArena;
use ratatui::{DefaultTerminal, Frame};

use crate::{
    traces::TracesState,
    widget::{TraceWidget, TraceWidgetState},
};

pub struct Tui {
    trace_state: TracesState,
    exit: bool,
}

impl Tui {
    pub fn new(trace_data: CallTraceArena) -> Self {
        Self {
            exit: false,
            trace_state: TracesState::new(trace_data),
        }
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        while !self.exit {
            terminal.try_draw(|frame| self.draw(frame))?;
            self.handle_events(terminal)?;
        }

        Ok(())
    }

    fn draw(&self, frame: &mut Frame) -> io::Result<()> {
        let result: TuiResult<_, _> = self.trace_state.to_text().into();
        frame.render_widget(result.map_err()?, frame.area());
        Ok(())
    }

    fn handle_events(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        let height = terminal.size()?.height;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => self.exit = true,
                KeyCode::Up => {
                    // if self.viewport_offset == 0 {
                    //     self.scroll_offset.0 = self.scroll_offset.0.saturating_sub(1)
                    // } else {
                    //     self.viewport_offset -= 1;
                    // }
                    // self.trace_widget.lock().unwrap().prev();
                }
                KeyCode::Down => {
                    // if self.viewport_offset == height - 1 {
                    //     self.scroll_offset.0 += 1
                    // } else {
                    //     self.viewport_offset += 1;
                    // }
                    // self.trace_widget.lock().unwrap().next();
                }
                // KeyCode::Left => self.scroll_offset.1 = self.scroll_offset.1.saturating_sub(1),
                // KeyCode::Right => self.scroll_offset.1 += 1,
                _ => {}
            }
        }
        Ok(())
    }
}

enum TuiResult<T, E> {
    Ok(T),
    Err(E),
}

impl<T> Into<io::Error> for TuiResult<T, eyre::Error> {
    fn into(self) -> io::Error {
        match self {
            TuiResult::Ok(_) => unreachable!(),
            TuiResult::Err(e) => io::Error::new(io::ErrorKind::Other, e),
        }
    }
}

impl<T> From<eyre::Result<T>> for TuiResult<T, eyre::Error> {
    fn from(result: eyre::Result<T>) -> Self {
        match result {
            Ok(t) => TuiResult::Ok(t),
            Err(e) => TuiResult::Err(e),
        }
    }
}

impl<T> TuiResult<T, eyre::Error> {
    fn map_err(self) -> io::Result<T> {
        match self {
            TuiResult::Ok(t) => Ok(t),
            TuiResult::Err(e) => Err(TuiResult::<T, _>::Err(e).into()),
        }
    }
}
