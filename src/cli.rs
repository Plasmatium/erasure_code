use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "erasure-code")]
#[command(about = "A demo cli tool for erasure-code learning", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Create erasure code block
    #[command(arg_required_else_help = true)]
    Create {
        #[arg(value_name = "INPUT-FILE", short = 'i')]
        file_name: PathBuf,

        #[arg(required = true, short = 'd')]
        data_dir: PathBuf,

        #[arg(default_value = "3+2", short = 'p')]
        pattern: String,
    },

    /// Rebuild try to rebuild the source data from remain parts
    #[command(arg_required_else_help = true)]
    Rebuild {
        #[arg(required = true, short = 'd')]
        data_dir: PathBuf,

        #[arg(required = true, short = 'o')]
        output_file_name: PathBuf,
    },
}
