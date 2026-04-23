use anyhow::Result;
use clap::{Parser, Subcommand};
use data_builder::{BuildConfig, build_artifact, write_artifact};

#[derive(Debug, Parser)]
#[command(author, version, about = "Offline Nepali lexicon artifact builder")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Build {
        #[arg(long, default_value = "dictionaries/db.sqlite")]
        sabdakosh: String,
        #[arg(long, default_value = "dictionaries/content.db")]
        content: String,
        #[arg(long, default_value = "artifacts/nepali.lexicon.bin")]
        output: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Build {
            sabdakosh,
            content,
            output,
        } => {
            let config = BuildConfig {
                sabdakosh_path: sabdakosh,
                content_path: content,
            };
            let artifact = build_artifact(&config)?;
            write_artifact(output, &artifact)?;
            println!(
                "Built lexicon artifact with {} entries and {} keys.",
                artifact.entries.len(),
                artifact.key_index.len()
            );
            println!(
                "Source rows: sabdakosh={}, content={}",
                artifact.stats.sabdakosh_rows, artifact.stats.content_rows
            );
            println!(
                "Dropped rows: empty_words={}, unromanizable={}",
                artifact.stats.dropped_empty_words, artifact.stats.dropped_unromanizable_words
            );
        }
    }

    Ok(())
}
