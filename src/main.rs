mod commands;
mod config;
mod package_manager;
mod scm;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "lockfix", about = "Generate an update plan and apply dependency upgrades from a lock file")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a plan of what changes will be made
    Plan {
        /// Path to the lockfix config file
        #[arg(short, long)]
        config: String,
    },
    /// Apply dependency upgrades from a plan or config file
    Apply {
        /// Path to the lockfix config file (generates a plan on the fly)
        #[arg(short, long, conflicts_with = "plan")]
        config: Option<String>,
        /// Path to a pre-generated plan JSON file
        #[arg(short, long, conflicts_with = "config")]
        plan: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Plan { config } => {
            match commands::plan::runner_par::run(&config) {
                Ok(plan) => println!("{}", serde_json::to_string_pretty(&plan).unwrap()),
                Err(e) => eprintln!("error: {e}"),
            }
        }
        Commands::Apply { config, plan } => {
            commands::apply::runner::run(config.as_deref(), plan.as_deref());
        }
    }
}
