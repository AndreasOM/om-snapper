use indicatif::MultiProgress;
use om_snapper::Snapshot;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Get {
        snapshot_id: String,
        #[arg(short, long)]
        r#continue: bool,
    },
    Status {
        snapshot_id: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let use_ansi = atty::is(atty::Stream::Stdout);

    let subscriber = FmtSubscriber::builder()
        //		.with_max_level(Level::TRACE)
        // .with_max_level(Level::INFO)
        .with_max_level(Level::WARN)
        .with_ansi(use_ansi) // sublime console doesn't like it :(
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let cli = Cli::parse();

    match &cli.command {
        Commands::Get {
            snapshot_id,
            r#continue,
        } => {
            println!("Getting snapshot. snapshot_id: {snapshot_id:?}");
            let mut snap = Snapshot::new(&snapshot_id);
            if *r#continue {
                snap.enable_continue();
            }
            //dbg!(&snap);

            let m = MultiProgress::new();
            snap.use_progress(m);

            match snap.download().await {
                Ok(_) => {
                    tracing::info!("Done");
                }
                Err(e) => {
                    println!("Download failed: {}", e);
                    //tracing::warn!("Download failed: {}", e);
                }
            };
        }
        Commands::Status { snapshot_id } => {
            let mut snap = Snapshot::new(&snapshot_id);
            let m = MultiProgress::new();
            snap.use_progress(m);

            match snap.status().await {
                Ok(true) => {
                    tracing::info!("Done - OK");
                }
                Ok(false) => {
                    tracing::info!("Done - Not OK");
                }
                Err(e) => {
                    println!("Status failed: {}", e);
                    //tracing::warn!("Download failed: {}", e);
                }
            };
        }
    }

    Ok(())
}
