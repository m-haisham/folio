mod cmd;

use clap::{Parser, Subcommand};
use eyre::Result;

#[derive(Parser)]
#[command(name = "folio", about = "Freelance invoice management tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init(cmd::init::InitArgs),
    New(cmd::new::NewArgs),
    Build(cmd::build::BuildArgs),
    Send(cmd::send::SendArgs),
    Paid(cmd::paid::PaidArgs),
    Void(cmd::void::VoidArgs),
    List(cmd::list::ListArgs),
    Summary(cmd::summary::SummaryArgs),
    Preview(cmd::preview::PreviewArgs),
    Templates(cmd::templates::TemplatesArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    match cli.command {
        Commands::Init(args) => cmd::init::run(args).await,
        Commands::New(args) => cmd::new::run(args).await,
        Commands::Build(args) => cmd::build::run(args).await,
        Commands::Send(args) => cmd::send::run(args).await,
        Commands::Paid(args) => cmd::paid::run(args).await,
        Commands::Void(args) => cmd::void::run(args).await,
        Commands::List(args) => cmd::list::run(args).await,
        Commands::Summary(args) => cmd::summary::run(args).await,
        Commands::Preview(args) => cmd::preview::run(args).await,
        Commands::Templates(args) => cmd::templates::run(args).await,
    }
}
