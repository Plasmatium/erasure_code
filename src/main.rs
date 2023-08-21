mod cli;
mod erasure;

use std::fs;

use clap::Parser;
use erasure::FileHandler;

use crate::cli::Cli;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args = Cli::parse();
    match args.command {
        cli::Commands::Create {
            file_name,
            data_dir,
            ref pattern,
        } => {
            let metadata = pattern.parse()?;
            let fh = FileHandler::new(metadata, file_name, data_dir);

            fh.reconstruct()
        }
        cli::Commands::Rebuild {
            data_dir,
            output_file_name,
            force,
        } => {
            let md_file_path = data_dir.join("metadata.json");
            let md_str = fs::read_to_string(md_file_path)?;
            let metadata = serde_json::from_str(&md_str)?;
            let fh = FileHandler::new(metadata, output_file_name, data_dir);

            fh.rebuild(force)
        }
    }
}
