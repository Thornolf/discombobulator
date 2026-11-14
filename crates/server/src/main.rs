mod bench;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use discombobulator_server::{AppState, DEFAULT_PORT, default_data_dir, local_address, serve};

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
    Serve {
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        #[arg(long, help = "Folder to open on start")]
        folder: Option<PathBuf>,
        #[arg(long, help = "Data folder for the index and cache")]
        data_dir: Option<PathBuf>,
    },
    Bench {
        folder: PathBuf,
        #[arg(long, help = "Use an empty temporary cache instead of the app cache")]
        fresh: bool,
    },
}

type AnyError = Box<dyn std::error::Error + Send + Sync>;

async fn run_server(
    port: u16,
    folder: Option<PathBuf>,
    data_dir: Option<PathBuf>,
) -> Result<(), AnyError> {
    let data_dir = data_dir
        .or_else(default_data_dir)
        .ok_or("no local data folder for this user")?;
    let state = AppState::open(&data_dir, port)?;
    let opener = std::sync::Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        let runtime = tokio::runtime::Handle::current();
        let _guard = runtime.enter();
        match folder {
            Some(folder) => {
                if let Err(error) = discombobulator_server::api::open_folder(&opener, folder) {
                    eprintln!("cannot open folder: {error:?}");
                }
            }
            None => opener.reopen_last_folder(),
        }
        opener.refresh_tailnet_host();
    })
    .await?;
    let address = local_address(port);
    let listener = tokio::net::TcpListener::bind(address).await?;
    println!("Discombobulator on http://{address}");
    serve(state, listener).await?;
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result: Result<(), AnyError> = match cli.command {
        Command::Serve {
            port,
            folder,
            data_dir,
        } => tokio::runtime::Runtime::new()
            .map_err(AnyError::from)
            .and_then(|runtime| runtime.block_on(run_server(port, folder, data_dir))),
        Command::Bench { folder, fresh } => {
            bench::run(&folder, fresh).map_err(|e| e.to_string().into())
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
