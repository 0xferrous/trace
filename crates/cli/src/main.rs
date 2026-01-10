use clap::Parser;
use ratatui::{
    crossterm::{
        QueueableCommand,
        event::{self, Event, KeyCode, MouseEvent, MouseEventKind},
        execute,
        style::{Print, ResetColor, SetStyle},
    },
    prelude::IntoCrossterm,
};
use revm_inspectors::tracing::{CallTraceArena, TraceWriter};
use simplelog::WriteLogger;
use std::{
    io::{self, Write},
    path::PathBuf,
    time::Duration,
};
use tui_app::{TracesState, Tui};

#[derive(clap::Parser, Debug)]
struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    Debug(DebugArgs),
    Tui(CommonArgs),
}

#[derive(clap::Parser, Debug)]
struct CommonArgs {
    path: Option<PathBuf>,
}

#[derive(clap::Parser, Debug)]
struct DebugArgs {
    #[clap(flatten)]
    common: CommonArgs,
    #[clap(long, short, default_value_t = false)]
    revm: bool,
}

impl CommonArgs {
    fn data(&self) -> eyre::Result<CallTraceArena> {
        fn read(reader: impl std::io::Read) -> serde_json::Result<CallTraceArena> {
            serde_json::from_reader(reader)
        }
        let data = if let Some(path) = &self.path {
            let file = std::fs::File::open(path)?;
            read(io::BufReader::new(file))
        } else {
            read(io::BufReader::new(io::stdin()))
        };
        Ok(data?)
    }
}

fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    let args = Args::parse();

    match args.command {
        Command::Debug(args) => {
            let data = args.common.data()?;
            if args.revm {
                let mut trace_writer = TraceWriter::new(Vec::new())
                    .color_cheatcodes(true)
                    .use_colors(revm_inspectors::ColorChoice::Always)
                    .write_bytecodes(true)
                    .with_storage_changes(true);
                trace_writer.write_arena(&data)?;
                let to_string = String::from_utf8(trace_writer.into_writer())?;
                println!("{to_string}");
            } else {
                let trace_state = TracesState::new(data);
                let text = trace_state.to_text(None)?;
                let mut stdout = std::io::stdout();

                for line in text.lines {
                    for span in line.spans {
                        stdout.queue(SetStyle(span.style.into_crossterm()))?;
                        stdout.queue(Print(span.content))?;
                        stdout.queue(ResetColor)?;
                    }
                    stdout.queue(Print("\n"))?;
                }
                stdout.flush()?;
            }
        }
        Command::Tui(args) => {
            // Parse log level from RUST_LOG env var, defaulting to Info
            let log_level = std::env::var("RUST_LOG")
                .ok()
                .and_then(|s| s.parse::<log::LevelFilter>().ok())
                .unwrap_or(log::LevelFilter::Info);

            WriteLogger::init(
                log_level,
                Default::default(),
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("/tmp/traces-cli.log")?,
            )?;

            let data = args.data()?;
            let mut tui = Tui::new(data);
            let mut terminal = ratatui::init();
            let mut tui_loop = || -> eyre::Result<()> {
                while !tui.exit() {
                    terminal.try_draw(|f| tui.draw(f))?;
                    let frame = terminal.get_frame();
                    while let Ok(true) = event::poll(Duration::from_millis(1)) {
                        if let Ok(event) = event::read() {
                            match event {
                                Event::Key(key) => {
                                    tui.on_key(key.code, &frame);
                                }
                                Event::Mouse(MouseEvent {
                                    kind:
                                        kind @ (MouseEventKind::ScrollDown | MouseEventKind::ScrollUp),
                                    // modifiers: KeyModifiers::SHIFT,
                                    ..
                                }) => match kind {
                                    MouseEventKind::ScrollDown => {
                                        tui.on_key(KeyCode::Char('J'), &frame);
                                    }
                                    MouseEventKind::ScrollUp => {
                                        tui.on_key(KeyCode::Char('K'), &frame);
                                    }
                                    _ => {}
                                },
                                _ => {}
                            }
                        }
                    }
                }
                Ok(())
            };

            let mut stdout = std::io::stdout();
            execute!(stdout, event::EnableMouseCapture).ok();
            let ret = tui_loop();
            // do cleanup
            execute!(stdout, event::DisableMouseCapture).ok();
            ratatui::restore();

            ret?
        }
    }

    Ok(())
}
