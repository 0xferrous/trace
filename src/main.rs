use ansi_to_tui::IntoText;
use clap::Parser;
use crossterm::event::{self, Event, KeyCode};
use foundry_evm_traces::{CallTraceArena, SparsedTraceArena, render_trace_arena_inner};
use ratatui::{DefaultTerminal, Frame, layout::Rect, widgets::Paragraph};
use std::io;

#[derive(clap::Parser, Debug)]
struct Args {
    trace_type: TraceType,
}

#[derive(Clone, clap::ValueEnum, Debug)]
enum TraceType {
    Cast,
}

enum TraceData {
    Cast(CallTraceArena),
}

fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    let args = Args::parse();

    let data = match args.trace_type {
        TraceType::Cast => {
            TraceData::Cast(serde_json::from_reader::<_, CallTraceArena>(io::stdin())?)
        }
    };

    let mut terminal = ratatui::init();
    Tui::new(data).run(&mut terminal)?;
    ratatui::restore();

    Ok(())
}

struct Tui {
    scroll_offset: (u16, u16),
    viewport_offset: u16,
    sparsed_trace: SparsedTraceArena,
    exit: bool,
}

impl Tui {
    fn new(data: TraceData) -> Self {
        match data {
            TraceData::Cast(data) => Self {
                sparsed_trace: SparsedTraceArena {
                    arena: data,
                    ignored: Default::default(),
                },
                exit: false,
                scroll_offset: (0, 0),
                viewport_offset: 0,
            },
        }
    }

    fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        while !self.exit {
            terminal.try_draw(|frame| self.draw(frame))?;
            self.handle_events(terminal)?;
        }

        Ok(())
    }

    fn draw(&self, frame: &mut Frame) -> io::Result<()> {
        let rendered = render_trace_arena_inner(&self.sparsed_trace, true, true)
            .into_text()
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "failed to convert ansi colored text to Text widget",
                )
            })?;
        let n_lines = rendered.iter().len();
        let para = Paragraph::new(rendered).scroll(self.scroll_offset);

        let mut trace_area = frame.area();
        trace_area.width -= 1;
        trace_area.x += 1;

        let mut pointer_text = vec![""; n_lines];
        pointer_text[(self.scroll_offset.0 + self.viewport_offset) as usize] = ">";
        let pointer_text = pointer_text.join("\n");
        let pointer_para = Paragraph::new(pointer_text).scroll(self.scroll_offset);

        frame.render_widget(para, trace_area);
        frame.render_widget(pointer_para, Rect::new(0, 0, 1, frame.area().height));
        Ok(())
    }

    fn handle_events(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        let height = terminal.size()?.height;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => self.exit = true,
                KeyCode::Up => {
                    if self.viewport_offset == 0 {
                        self.scroll_offset.0 = self.scroll_offset.0.saturating_sub(1)
                    } else {
                        self.viewport_offset -= 1;
                    }
                }
                KeyCode::Down => {
                    if self.viewport_offset == height - 1 {
                        self.scroll_offset.0 += 1
                    } else {
                        self.viewport_offset += 1;
                    }
                }
                KeyCode::Left => self.scroll_offset.1 = self.scroll_offset.1.saturating_sub(1),
                KeyCode::Right => self.scroll_offset.1 += 1,
                _ => {}
            }
        }
        Ok(())
    }
}
