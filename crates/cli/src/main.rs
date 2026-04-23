use std::io::{self, Write};
use std::time::Instant;

use anyhow::Result;
use clap::{Parser, Subcommand};
use core_engine::Lexicon;
use host_api::{
    HostAction, HostKeyEvent, LinuxImeAdapter, LinuxImeFramework, PlatformAdapter,
    WindowsTsfAdapter,
};

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about = "Nepali transliteration demo and benchmark CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Lookup {
        #[arg(long, default_value = "artifacts/nepali.lexicon.bin")]
        lexicon: String,
        #[arg(long)]
        input: String,
        #[arg(long, default_value_t = 5)]
        limit: usize,
    },
    Demo {
        #[arg(long, default_value = "artifacts/nepali.lexicon.bin")]
        lexicon: String,
    },
    Simulate {
        #[arg(long, default_value = "artifacts/nepali.lexicon.bin")]
        lexicon: String,
        #[arg(long, default_value = "windows")]
        platform: String,
        #[arg(long)]
        input: String,
    },
    Bench {
        #[arg(long, default_value = "artifacts/nepali.lexicon.bin")]
        lexicon: String,
        #[arg(long, default_value_t = 500)]
        iterations: usize,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Lookup {
            lexicon,
            input,
            limit,
        } => run_lookup(&lexicon, &input, limit),
        Command::Demo { lexicon } => run_demo(&lexicon),
        Command::Simulate {
            lexicon,
            platform,
            input,
        } => run_simulate(&lexicon, &platform, &input),
        Command::Bench {
            lexicon,
            iterations,
        } => run_bench(&lexicon, iterations),
    }
}

fn run_lookup(path: &str, input: &str, limit: usize) -> Result<()> {
    let lexicon = Lexicon::load_from_path(path)?;
    let candidates = lexicon.find_candidates(input, limit);

    println!("Input: {input}");
    if candidates.is_empty() {
        println!("No candidates found.");
        return Ok(());
    }

    for (idx, candidate) in candidates.iter().enumerate() {
        println!(
            "[{idx}] {}  ({}, score={})",
            candidate.word, candidate.romanized, candidate.score
        );
    }

    Ok(())
}

fn run_demo(path: &str) -> Result<()> {
    let lexicon = Lexicon::load_from_path(path)?;
    let mut adapter = WindowsTsfAdapter::new(lexicon);

    println!("Interactive Nepali transliteration demo");
    println!("Type Latin text and press Enter.");
    println!("Commands: :back, :reset, :up, :down, :enter, :commit N, :quit");

    loop {
        print!("> ");
        io::stdout().flush()?;

        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        let line = line.trim();

        if line == ":quit" {
            break;
        } else if line == ":back" {
            print_actions(adapter.handle_key_event(HostKeyEvent::Backspace)?)?;
        } else if line == ":reset" {
            print_actions(adapter.handle_key_event(HostKeyEvent::Reset)?)?;
        } else if line == ":up" {
            print_actions(adapter.handle_key_event(HostKeyEvent::PrevCandidate)?)?;
        } else if line == ":down" {
            print_actions(adapter.handle_key_event(HostKeyEvent::NextCandidate)?)?;
        } else if line == ":enter" {
            print_actions(adapter.handle_key_event(HostKeyEvent::CommitCurrent)?)?;
        } else if let Some(index) = line.strip_prefix(":commit ") {
            let index: usize = index.parse()?;
            print_actions(adapter.handle_key_event(HostKeyEvent::CommitSelection(index))?)?;
        } else {
            for ch in line.chars() {
                print_actions(adapter.handle_key_event(HostKeyEvent::Character(ch))?)?;
            }
        }
    }

    Ok(())
}

fn run_simulate(path: &str, platform: &str, input: &str) -> Result<()> {
    let lexicon = Lexicon::load_from_path(path)?;
    println!(
        "Loaded lexicon: {} entries, {} keys",
        lexicon.stats().unique_headwords,
        lexicon.stats().indexed_keys
    );

    match platform {
        "windows" => {
            let mut adapter = WindowsTsfAdapter::new(lexicon);
            simulate_input(&mut adapter, input)?;
        }
        "ibus" => {
            let mut adapter = LinuxImeAdapter::new(lexicon, LinuxImeFramework::IBus);
            simulate_input(&mut adapter, input)?;
        }
        "fcitx5" => {
            let mut adapter = LinuxImeAdapter::new(lexicon, LinuxImeFramework::Fcitx5);
            simulate_input(&mut adapter, input)?;
        }
        other => anyhow::bail!("unsupported platform `{other}`; expected windows, ibus, or fcitx5"),
    }

    Ok(())
}

fn run_bench(path: &str, iterations: usize) -> Result<()> {
    let lexicon = Lexicon::load_from_path(path)?;
    let queries = ["prabesh", "parbesh", "namaste", "pariksha", "sikshya"];
    let start = Instant::now();
    let mut total_results = 0usize;

    for _ in 0..iterations {
        for query in queries {
            total_results += lexicon.find_candidates(query, 5).len();
        }
    }

    let elapsed = start.elapsed();
    let total_queries = iterations * queries.len();
    let per_query = elapsed.as_secs_f64() * 1_000_000.0 / total_queries as f64;

    println!("Lexicon entries: {}", lexicon.stats().unique_headwords);
    println!("Indexed keys: {}", lexicon.stats().indexed_keys);
    println!("Queries: {total_queries}");
    println!("Total candidate batches: {total_results}");
    println!("Elapsed: {:.2?}", elapsed);
    println!("Average per query: {:.2}µs", per_query);

    Ok(())
}

fn simulate_input(adapter: &mut dyn PlatformAdapter, input: &str) -> Result<()> {
    println!("Platform: {}", adapter.platform_id());
    for ch in input.chars() {
        print_actions(adapter.handle_key_event(HostKeyEvent::Character(ch))?)?;
    }
    print_actions(adapter.handle_key_event(HostKeyEvent::CommitCurrent)?)?;
    Ok(())
}

fn print_actions(actions: Vec<HostAction>) -> Result<()> {
    let selected_index = actions.iter().find_map(|action| match action {
        HostAction::UpdatePreedit(preedit) => preedit.selected_index,
        _ => None,
    });

    for action in actions {
        match action {
            HostAction::UpdatePreedit(preedit) => {
                println!("Buffer: {}", preedit.latin_buffer);
                println!("Key: {}", preedit.normalized_key);
                println!(
                    "Auto: {}",
                    preedit
                        .auto_selected
                        .as_ref()
                        .map(|candidate| candidate.word.as_str())
                        .unwrap_or("-")
                );
            }
            HostAction::ShowCandidates(candidates) => {
                for (idx, candidate) in candidates.iter().enumerate() {
                    let marker = if Some(idx) == selected_index {
                        ">"
                    } else {
                        " "
                    };
                    println!(
                        "{marker} [{idx}] {}  ({}, score={})",
                        candidate.word, candidate.romanized, candidate.score
                    );
                }
            }
            HostAction::CommitText(text) => println!("Committed: {text}"),
            HostAction::ClearComposition => println!("Composition cleared"),
            HostAction::Noop => {}
        }
    }

    io::stdout().flush()?;
    Ok(())
}
