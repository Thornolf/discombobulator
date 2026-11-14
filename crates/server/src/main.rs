mod bench;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "discombobulator",
    version,
    about = "Fast photo culling for Fujifilm RAF + JPG"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Bench {
        folder: PathBuf,
        #[arg(long, help = "Use an empty temporary cache instead of the app cache")]
        fresh: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Bench { folder, fresh } => bench::run(&folder, fresh),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
