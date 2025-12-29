use clap::Parser;
use revm_inspectors::tracing::{CallTraceArena, TraceWriter};
use std::{io, path::PathBuf};

mod traces;
mod tui;

use tui::Tui;

#[derive(clap::Parser, Debug)]
struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    Debug(CommonArgs),
    Tui(CommonArgs),
}

#[derive(clap::Parser, Debug)]
struct CommonArgs {
    path: Option<PathBuf>,
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
            let data = args.data()?;
            let mut trace_writer = TraceWriter::new(Vec::new())
                .color_cheatcodes(true)
                .use_colors(revm_inspectors::ColorChoice::Always)
                .write_bytecodes(true)
                .with_storage_changes(true);
            trace_writer.write_arena(&data)?;
            let to_string = String::from_utf8(trace_writer.into_writer())?;
            println!("{to_string}");
        }
        Command::Tui(args) => {
            let data = args.data()?;
            let mut terminal = ratatui::init();
            Tui::new(data).run(&mut terminal)?;
            ratatui::restore();
        }
    }

    Ok(())
}
