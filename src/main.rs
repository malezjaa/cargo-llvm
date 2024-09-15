pub mod build;
pub mod config;
pub mod entry;
pub mod error;
pub mod resource;


use std::{
    env,
    path::PathBuf,
    process::{exit, Command},
};
use clap::{Parser, Subcommand,  builder::{styling, Styles},};
use vit_logger::{VitLogger, Config as VitConfig};
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
    Current {
        #[arg(short = 'v', long = "verbose")]
        verbose: bool,
    },

    #[command(name = "prefix", about = "Show the prefix of the current build")]
    Prefix {
        #[arg(short = 'v', long = "verbose")]
        verbose: bool,
    },

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
        #[arg(short = 'v', long = "verbose")]
        verbose: bool,
    },

    #[command(name = "expand", about = "expand archive")]
    Expand {
        #[arg()]
        path: PathBuf,
        #[arg(short = 'v', long = "verbose")]
        verbose: bool,
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
            .target(true)
            .file(verbose)
            .line(true)
            .time(false)
            .finish()
            .expect("Error building config"),
    );

    match opt.command {
        Commands::Init {} => config::init_config()?,

        Commands::Builds {} => {
            let builds = build::builds()?;
            let max = builds.iter().map(|b| b.name().len()).max().unwrap();
            for b in &builds {
                println!(
                    "{name:<width$}: {prefix}",
                    name = b.name(),
                    prefix = b.prefix().display(),
                    width = max
                );
            }
        }

        Commands::Entries {} => {
            if let Ok(entries) = entry::load_entries() {
                for entry in &entries {
                    println!("{}", entry.name());
                }
            } else {
                panic!("No entries. Please define entries in $XDG_CONFIG_HOME/cargo-llvm/entry.toml");
            }
        }
        Commands::BuildEntry {
            name,
            update,
            clean,
            discard,
            builder,
            nproc,
            build_type,
        } => {
            let mut entry = entry::load_entry(&name)?;
            let nproc = nproc.unwrap_or_else(num_cpus::get);
            if let Some(builder) = builder {
                entry.set_builder(&builder)?;
            }
            if let Some(build_type) = build_type {
                entry.set_build_type(build_type)?;
            }
            if discard {
                entry.clean_cache_dir().unwrap();
            }
            entry.checkout().unwrap();
            if update {
                entry.update().unwrap();
            }
            if clean {
                entry.clean_build_dir().unwrap();
            }
            entry.build(nproc).unwrap();
        }

        Commands::Current { verbose } => {
            let build = build::seek_build()?;
            println!("{}", build.name());
            if verbose {
                if let Some(env) = build.env_path() {
                    eprintln!("set by {}", env.display());
                }
            }
        }
        Commands::Prefix { verbose } => {
            let build = build::seek_build()?;
            println!("{}", build.prefix().display());
            if verbose {
                if let Some(env) = build.env_path() {
                    eprintln!("set by {}", env.display());
                }
            }
        }
        Commands::Version {
            name,
            major,
            minor,
            patch,
        } => {
            let build = if let Some(name) = name {
                get_existing_build(&name)
            } else {
                build::seek_build()?
            };
            let version = build.version()?;
            if !(major || minor || patch) {
                println!("{}.{}.{}", version.major, version.minor, version.patch);
            } else {
                if major {
                    print!("{}", version.major);
                }
                if minor {
                    print!("{}", version.minor);
                }
                if patch {
                    print!("{}", version.patch);
                }
                println!();
            }
        }

        Commands::Global { name } => {
            let build = get_existing_build(&name);
            build.set_global()?;
        }
        Commands::Local { name, path } => {
            let build = get_existing_build(&name);
            let path = path.unwrap_or_else(|| env::current_dir().unwrap());
            build.set_local(&path)?;
        }

        Commands::Archive { name, verbose } => {
            let build = get_existing_build(&name);
            build.archive(verbose)?;
        }
        Commands::Expand { path, verbose } => {
            build::expand(&path, verbose)?;
        }

        Commands::Edit {} => {
            let editor = env::var("EDITOR").expect("EDITOR environmental value is not set");
            Command::new(editor)
                .arg(config::config_dir()?.join(config::ENTRY_TOML))
                .check_run()?;
        }

        Commands::Zsh {} => {
            let src = include_str!("../cargo-llvm.zsh");
            println!("{}", src);
        }
    }
    Ok(())
}

fn get_existing_build(name: &str) -> build::Build {
    let build = build::Build::from_name(name).unwrap();
    if build.exists() {
        build
    } else {
        eprintln!("Build '{}' does not exists", name);
        exit(1)
    }
}
