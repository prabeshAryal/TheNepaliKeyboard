use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows_tip::{
    current_registered_module, is_registered, register_text_service, try_resolve_module_path,
    unregister_text_service,
};

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about = "Register or inspect the The Nepali Keyboard Windows TIP"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Register {
        #[arg(long)]
        dll: Option<PathBuf>,
    },
    Unregister,
    Status,
}

fn main() -> Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .context("failed to initialize COM")?;
    }

    let result = run();

    unsafe {
        CoUninitialize();
    }

    result
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Register { dll } => {
            let path = try_resolve_module_path(dll.as_deref())?;
            register_text_service(&path)?;
            println!("Registered The Nepali Keyboard TIP from {}", path.display());
            println!(
                "You may need to add it from Windows Text Services and Input Languages if it is not already visible."
            );
        }
        Command::Unregister => {
            unregister_text_service()?;
            println!("Unregistered The Nepali Keyboard TIP.");
        }
        Command::Status => {
            println!("Registered: {}", is_registered()?);
            if let Some(path) = current_registered_module()? {
                println!("DLL: {}", path.display());
            }
        }
    }

    Ok(())
}
