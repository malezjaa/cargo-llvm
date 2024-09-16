pub mod build;
pub mod config;
pub mod entry;
pub mod error;
pub mod resource;
pub mod commands;

use std::{
    env,
    path::PathBuf,
    process::{exit, Command},
};
use clap::{Parser, Subcommand, builder::{styling, Styles}};
use log::log;
use vit_logger::{VitLogger, Config as VitConfig};
use crate::commands::build_entry::build_entry_command;
use crate::commands::version::version_command;
use crate::error::CommandExt;

#[derive(Parser, Debug)]
#[command(
    name = "cargo-llvm",
    about = "Manage multiple LLVM/Clang builds",
    version,
    long_about = None,
    styles = Styles::styled()
        .header(styling::AnsiColor::Yellow.on_default())
        .usage(styling::AnsiColor::Yellow.on_default())
        .literal(styling::AnsiColor::Green.on_default())
)]
struct Program {
    #[arg(global = true, short, long)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(name = "init", about = "Initialize cargo-llvm")]
    Init {},

    #[command(name = "builds", about = "List usable build")]
    Builds {},

    #[command(name = "entries", about = "List entries to be built")]
    Entries {},

    #[command(name = "build-entry", about = "Build LLVM/Clang")]
    BuildEntry {
        name: String,
        #[arg(short = 'u', long = "update")]
        update: bool,
        #[arg(short = 'c', long = "clean", help = "clean build directory")]
        clean: bool,
        #[arg(
            short = 'G',
            long = "builder",
            help = "Overwrite cmake generator setting"
        )]
        builder: Option<String>,
        #[arg(
            short = 'd',
            long = "discard",
            help = "discard source directory for remote resources"
        )]
        discard: bool,
        #[arg(short = 's', long = "skip-download", help = "If you already have the source")]
        skip_download: bool,
        #[arg(short = 'j', long = "nproc")]
        nproc: Option<usize>,
        #[arg(
            short = 't',
            long = "build-type",
            help = "Overwrite cmake build type (Debug, Release, RelWithDebInfo, or MinSizeRel)"
        )]
        build_type: Option<entry::BuildType>,
    },

    #[command(name = "current", about = "Show the name of current build")]
    Current,

    #[command(name = "prefix", about = "Show the prefix of the current build")]
    Prefix,

    #[command(name = "version", about = "Show the base version of the current build")]
    Version {
        #[arg(short = 'n', long = "name")]
        name: Option<String>,
        #[arg(long = "major")]
        major: bool,
        #[arg(long = "minor")]
        minor: bool,
        #[arg(long = "patch")]
        patch: bool,
    },

    #[command(name = "global", about = "Set the build to use (global)")]
    Global { name: String },

    #[command(name = "local", about = "Set the build to use (local)")]
    Local {
        name: String,
        #[arg(short = 'p', long = "path")]
        path: Option<PathBuf>,
    },

    #[command(name = "archive", about = "archive build into *.tar.xz (require pixz)")]
    Archive {
        name: String,
    },

    #[command(name = "expand", about = "expand archive")]
    Expand {
        #[arg()]
        path: PathBuf,
    },

    #[command(name = "edit", about = "Edit cargo-llvm configure in your editor")]
    Edit {},

    #[command(name = "zsh", about = "Setup Zsh integration")]
    Zsh {},
}

fn main() -> error::Result<()> {
    let opt = Program::parse();
    let verbose = opt.verbose;

    std::env::set_var("RUST_LOG", if verbose { "trace" } else { "info" });
    VitLogger::new().init(
        VitConfig::builder()
            .text(true)
            .target(verbose)
            .file(verbose)
            .line(verbose)
            .time(false)
            .finish()
            .expect("Error building config"),
    );

    let result = match opt.command {
        Commands::Init {} => config::init_config(),

        Commands::Builds {} => {
            let builds = build::builds()?;
            let max = builds.iter().map(|b| b.name().len()).max().unwrap();
            log::info!("Builds:");
            for b in &builds {
                println!(
                    "{name:<width$}: {prefix}",
                    name = b.name(),
                    prefix = b.prefix().display(),
                    width = max
                );
            }

            Ok(())
        }

        Commands::Entries {} => {
            if let Ok(entries) = entry::load_entries() {
                log::info!("Entries:");

                for entry in &entries {
                    println!("     - {}", entry.name());
                }
            } else {
                panic!("No entries. Please define entries in $XDG_CONFIG_HOME/cargo-llvm/entry.toml");
            }

            Ok(())
        }

        Commands::BuildEntry {
            name,
            update,
            clean,
            discard,
            builder,
            skip_download,
            nproc,
            build_type,
        } => build_entry_command(name, update, clean, discard, builder, nproc, build_type, skip_download),

        Commands::Current => {
            let build = build::seek_build()?;
            log::info!("Current build: {}", build.name());
            if verbose {
                if let Some(env) = build.env_path() {
                    log::debug!("set by {}", env.display());
                }
            }

            Ok(())
        }

        Commands::Prefix => {
            let build = build::seek_build()?;
            log::info!("{}", build.prefix().display());
            if verbose {
                if let Some(env) = build.env_path() {
                    log::debug!("set by {}", env.display());
                }
            }

            Ok(())
        }
        Commands::Version {
            name,
            major,
            minor,
            patch,
        } => version_command(name, major, minor, patch),
        Commands::Global { name } => {
            let build = get_existing_build(&name);
            build.set_global()
        }
        Commands::Local { name, path } => {
            let build = get_existing_build(&name);
            let path = path.unwrap_or_else(|| env::current_dir().unwrap());
            build.set_local(&path)
        }
        Commands::Archive { name } => {
            let build = get_existing_build(&name);
            build.archive(verbose)
        }
        Commands::Expand { path } => {
            build::expand(&path, verbose)
        }
        Commands::Edit {} => {
            let editor = env::var("EDITOR").map_err(|_| {
                log::error!("No EDITOR environment variable set");
                exit(1);
            }).unwrap();
            Command::new(editor)
                .arg(config::config_dir()?.join(config::ENTRY_TOML))
                .check_run()
        }
        _ => {
            eprintln!("Subcommand not implemented");
            exit(1);
        }
    };

    match result {
        Ok(_) => {
            log::debug!("Done");
        }
        Err(e) => {
            log::error!("{}", e);
            exit(1);
        }
    }

    Ok(())
}

fn get_existing_build(name: &str) -> build::Build {
    let build = build::Build::from_name(name).unwrap();
    if build.exists() {
        build
    } else {
        log::error!("Build '{}' does not exists", name);
        exit(1)
    }
}
