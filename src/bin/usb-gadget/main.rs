//! usb-gadget CLI tool — configure USB gadgets from TOML files.

mod build;
mod config;

use std::{
    ffi::OsStr,
    fs,
    io::{Error, ErrorKind, Result},
    path::PathBuf,
};

use clap::{Parser, Subcommand};
use usb_gadget::{default_udc, registered, remove_all, udcs, Udc};

use crate::config::GadgetConfig;

/// USB gadget configuration tool.
#[derive(Parser)]
#[command(name = "usb-gadget", version, about = "Configure USB gadgets from TOML files")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Register and bind USB gadgets from config file(s) or a directory.
    Up {
        /// Path to a TOML config file or directory of TOML files.
        path: PathBuf,
    },
    /// Unbind and remove a USB gadget by name.
    Down {
        /// Gadget name(s) to remove, or --all to remove all.
        names: Vec<String>,
        /// Remove all registered gadgets.
        #[arg(long)]
        all: bool,
    },
    /// List registered USB gadgets.
    #[command(alias = "ls")]
    List,
    /// Validate a config file without registering.
    Check {
        /// Path to a TOML config file or directory.
        path: PathBuf,
    },
    /// Print a configuration template to stdout.
    Template {
        /// Template name, or --list to show available templates.
        name: Option<String>,
        /// List available templates.
        #[arg(long)]
        list: bool,
    },
}

fn main() {
    env_logger::init();
    let cli = Cli::parse();

    if let Err(e) = run(cli) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Up { path } => cmd_up(&path),
        Command::Down { names, all } => cmd_down(&names, all),
        Command::List => cmd_list(),
        Command::Check { path } => cmd_check(&path),
        Command::Template { name, list } => cmd_template(name.as_deref(), list),
    }
}

fn cmd_up(path: &PathBuf) -> Result<()> {
    let configs = load_configs(path)?;
    if configs.is_empty() {
        return Err(Error::new(ErrorKind::InvalidInput, "no config files found"));
    }

    for (file, cfg) in &configs {
        let file_stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("gadget").to_string();
        let (mut gadget, name) = build::build_gadget(cfg)?;
        let gadget_name = name.unwrap_or(file_stem);

        let udc = if let Some(ref udc_name) = cfg.udc {
            find_udc(udc_name)?
        } else {
            default_udc().map_err(|e| Error::new(ErrorKind::NotFound, format!("no UDC found: {e}")))?
        };

        gadget.name = Some(gadget_name.clone());

        let mut reg = gadget.bind(&udc)?;
        reg.detach();

        println!("registered and bound gadget '{gadget_name}' to UDC '{}'", udc.name().to_string_lossy());
    }

    Ok(())
}

fn cmd_down(names: &[String], all: bool) -> Result<()> {
    if all {
        remove_all()?;
        println!("removed all gadgets");
        return Ok(());
    }

    if names.is_empty() {
        return Err(Error::new(ErrorKind::InvalidInput, "specify gadget names or --all"));
    }

    let gadgets = registered()?;
    for name in names {
        let matching: Vec<_> = gadgets.iter().filter(|g| g.name() == OsStr::new(name)).collect();

        if matching.is_empty() {
            eprintln!("warning: gadget '{name}' not found");
            continue;
        }
    }

    // Re-fetch to get ownership for removal.
    let gadgets = registered()?;
    for gadget in gadgets {
        let gname = gadget.name().to_string_lossy().to_string();
        if names.iter().any(|n| n == &gname) {
            gadget.remove()?;
            println!("removed gadget '{gname}'");
        }
    }

    Ok(())
}

fn cmd_list() -> Result<()> {
    let gadgets = registered()?;
    if gadgets.is_empty() {
        println!("no gadgets registered");
        return Ok(());
    }

    for gadget in &gadgets {
        let name = gadget.name().to_string_lossy();
        let udc = match gadget.udc()? {
            Some(u) => u.to_string_lossy().to_string(),
            None => "(unbound)".to_string(),
        };
        println!("{name}\t{udc}");
    }

    Ok(())
}

fn cmd_check(path: &PathBuf) -> Result<()> {
    let configs = load_configs(path)?;
    if configs.is_empty() {
        return Err(Error::new(ErrorKind::InvalidInput, "no config files found"));
    }

    for (file, cfg) in &configs {
        match build::build_gadget(cfg) {
            Ok((_, name)) => {
                let name = name
                    .or_else(|| file.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string()))
                    .unwrap_or_else(|| "unknown".to_string());
                println!("{}: ok (gadget '{name}')", file.display());
            }
            Err(e) => {
                println!("{}: error: {e}", file.display());
            }
        }
    }

    Ok(())
}

/// Load config file(s) from a path (single file or directory).
fn load_configs(path: &PathBuf) -> Result<Vec<(PathBuf, GadgetConfig)>> {
    let mut configs = Vec::new();

    if path.is_dir() {
        let mut entries: Vec<_> = fs::read_dir(path)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|ext| ext.to_str()) == Some("toml"))
            .collect();
        entries.sort_by_key(|e| e.path());

        for entry in entries {
            let cfg = load_one_config(&entry.path())?;
            configs.push((entry.path(), cfg));
        }
    } else {
        let cfg = load_one_config(path)?;
        configs.push((path.clone(), cfg));
    }

    Ok(configs)
}

fn load_one_config(path: &PathBuf) -> Result<GadgetConfig> {
    let content =
        fs::read_to_string(path).map_err(|e| Error::new(e.kind(), format!("{}: {e}", path.display())))?;
    let cfg: GadgetConfig = toml::from_str(&content)
        .map_err(|e| Error::new(ErrorKind::InvalidData, format!("{}: {e}", path.display())))?;
    Ok(cfg)
}

fn find_udc(name: &str) -> Result<Udc> {
    udcs()?
        .into_iter()
        .find(|u| u.name() == name)
        .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("UDC '{name}' not found")))
}

const TEMPLATES: &[(&str, &str)] = &[
    ("serial", include_str!("templates/serial.toml")),
    ("printer", include_str!("templates/printer.toml")),
    ("ethernet", include_str!("templates/ethernet.toml")),
    ("mass-storage", include_str!("templates/mass-storage.toml")),
    ("audio", include_str!("templates/audio.toml")),
    ("uac1", include_str!("templates/uac1.toml")),
    ("webcam", include_str!("templates/webcam.toml")),
    ("hid", include_str!("templates/hid.toml")),
    ("midi", include_str!("templates/midi.toml")),
    ("loopback", include_str!("templates/loopback.toml")),
    ("composite", include_str!("templates/composite.toml")),
];

fn cmd_template(name: Option<&str>, list: bool) -> Result<()> {
    if list || name.is_none() {
        println!("Available templates:");
        for (tname, _) in TEMPLATES {
            println!("  {tname}");
        }
        if !list {
            println!("\nUsage: usb-gadget template <name>");
        }
        return Ok(());
    }

    let name = name.unwrap();
    let template = TEMPLATES.iter().find(|(n, _)| *n == name).map(|(_, content)| *content).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            format!("unknown template '{name}' (use --list to see available templates)"),
        )
    })?;

    print!("{template}");
    Ok(())
}
