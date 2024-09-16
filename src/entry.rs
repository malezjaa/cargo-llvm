//! Describes how to compile LLVM/Clang
//!
//! entry.toml
//! -----------
//! **entry** in cargo-llvm describes how to compile LLVM/Clang, and set by `$XDG_CONFIG_HOME/cargo-llvm/entry.toml`.
//! `cargo-llvm init` generates default setting:
//!
//! ```toml
//! [llvm-mirror]
//! url    = "https://github.com/llvm-mirror/llvm"
//! target = ["X86"]
//!
//! [[llvm-mirror.tools]]
//! name = "clang"
//! url = "https://github.com/llvm-mirror/clang"
//!
//! [[llvm-mirror.tools]]
//! name = "clang-extra"
//! url = "https://github.com/llvm-mirror/clang-tools-extra"
//! relative_path = "tools/clang/tools/extra"
//! ```
//!
//! (TOML format has been changed largely at version 0.2.0)
//!
//! **tools** property means LLVM tools, e.g. clang, compiler-rt, lld, and so on.
//! These will be downloaded into `${llvm-top}/tools/${tool-name}` by default,
//! and `relative_path` property change it.
//! This toml will be decoded into [EntrySetting][EntrySetting] and normalized into [Entry][Entry].
//!
//! [Entry]: ./enum.Entry.html
//! [EntrySetting]: ./struct.EntrySetting.html
//!
//! Local entries (since v0.2.0)
//! -------------
//! Different from above *remote* entries, you can build locally cloned LLVM source with *local* entry.
//!
//! ```toml
//! [my-local-llvm]
//! path = "/path/to/your/src"
//! target = ["X86"]
//! ```
//!
//! Entry is regarded as *local* if there is `path` property, and *remote* if there is `url` property.
//! Other options are common to *remote* entries.
//!
//! Pre-defined entries
//! ------------------
//!
//! There is also pre-defined entries corresponding to the LLVM/Clang releases:
//!
//! ```shell
//! $ cargo-llvm entries
//
//   info Entries:
//       - 18.1.0
//       - 17.0.2
//       - 17.0.0
//       - 16.0.6
//       - 16.0.0
//       - 15.0.7
//       - 15.0.0
//       - 14.0.6
//       - 14.0.0
//       - 13.0.0
//       - 12.0.1
//       - 12.0.0
//       - 11.1.0
//       - 11.0.0
//       - 10.0.1
//       - 10.0.0
//       - 9.0.1
//       - 8.0.1
//       - 9.0.0
//       - 8.0.0
//       - 7.1.0
//       - 7.0.1
//       - 7.0.0
//       - 6.0.1
//       - 6.0.0
//       - 5.0.2
//       - 5.0.1
//       - 4.0.1
//       - 4.0.0
//       - 3.9.1
//       - 3.9.0
//! ```
//!
//! These are compiled with the default setting as shown above. You have to create entry manually
//! if you want to use custom settings.

use itertools::*;
use log::{info, warn};
use semver::{Version, VersionReq};
use serde_derive::Deserialize;
use std::{collections::HashMap, fs, path::PathBuf, process, str::FromStr};

use crate::{config::*, error::*, resource::*};

/// Option for CMake Generators
///
/// - Official document: [CMake Generators](https://cmake.org/cmake/help/latest/manual/cmake-generators.7.html)
///
/// ```
/// use cargo-llvm::entry::CMakeGenerator;
/// use std::str::FromStr;
/// assert_eq!(CMakeGenerator::from_str("Makefile").unwrap(), CMakeGenerator::Makefile);
/// assert_eq!(CMakeGenerator::from_str("Ninja").unwrap(), CMakeGenerator::Ninja);
/// assert_eq!(CMakeGenerator::from_str("vs").unwrap(), CMakeGenerator::VisualStudio);
/// assert_eq!(CMakeGenerator::from_str("VisualStudio").unwrap(), CMakeGenerator::VisualStudio);
/// assert!(CMakeGenerator::from_str("MySuperBuilder").is_err());
/// ```
#[derive(Deserialize, PartialEq, Debug, Clone, Default)]
pub enum CMakeGenerator {
    /// Use platform default generator (without -G option)
    #[default] Platform,
    /// Unix Makefile
    Makefile,
    /// Ninja generator
    Ninja,
    /// Visual Studio 15 2017
    VisualStudio,
    /// Visual Studio 15 2017 Win64
    VisualStudioWin64,
}

impl FromStr for CMakeGenerator {
    type Err = Error;
    fn from_str(generator: &str) -> Result<Self> {
        Ok(match generator.to_ascii_lowercase().as_str() {
            "makefile" => CMakeGenerator::Makefile,
            "ninja" => CMakeGenerator::Ninja,
            "visualstudio" | "vs" => CMakeGenerator::VisualStudio,
            _ => {
                return Err(Error::UnsupportedGenerator {
                    generator: generator.into(),
                });
            }
        })
    }
}

impl CMakeGenerator {
    /// Option for cmake
    pub fn option(&self) -> Vec<String> {
        match self {
            CMakeGenerator::Platform => Vec::new(),
            CMakeGenerator::Makefile => vec!["-G", "Unix Makefiles"],
            CMakeGenerator::Ninja => vec!["-G", "Ninja"],
            CMakeGenerator::VisualStudio => vec!["-G", "Visual Studio 15 2017"],
            CMakeGenerator::VisualStudioWin64 => {
                vec!["-G", "Visual Studio 15 2017 Win64", "-Thost=x64"]
            }
        }
            .into_iter()
            .map(|s| s.into())
            .collect()
    }

    /// Option for cmake build mode (`cmake --build` command)
    pub fn build_option(&self, nproc: usize, build_type: BuildType) -> Vec<String> {
        match self {
            CMakeGenerator::VisualStudioWin64 | CMakeGenerator::VisualStudio => {
                vec!["--config".into(), format!("{:?}", build_type)]
            }
            CMakeGenerator::Platform => Vec::new(),
            CMakeGenerator::Makefile | CMakeGenerator::Ninja => {
                vec!["--".into(), "-j".into(), format!("{}", nproc)]
            }
        }
    }
}

/// CMake build type
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Default)]
pub enum BuildType {
    Debug,
    #[default] Release,
    RelWithDebInfo,
    MinSizeRel,
}

impl FromStr for BuildType {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "debug" => Ok(BuildType::Debug),
            "release" => Ok(BuildType::Release),
            "relwithdebinfo" => Ok(BuildType::RelWithDebInfo),
            "minsizerel" => Ok(BuildType::MinSizeRel),
            _ => Err(Error::UnsupportedBuildType {
                build_type: s.to_string(),
            }),
        }
    }
}


/// Setting for both Remote and Local entries. TOML setting file will be decoded into this struct.
///
///
#[derive(Deserialize, Debug, Default, Clone, PartialEq)]
pub struct EntrySetting {
    /// URL of remote LLVM resource, see also [resouce](../resource/index.html) module
    pub url: Option<String>,

    /// Path of local LLVM source dir
    pub path: Option<String>,

    /// Target to be built, e.g. "X86". Empty means all backend
    #[serde(default)]
    pub target: Vec<String>,

    /// CMake Generator option (-G option in cmake)
    #[serde(default)]
    pub generator: CMakeGenerator,

    ///  Option for `CMAKE_BUILD_TYPE`
    #[serde(default)]
    pub build_type: BuildType,

    /// Additional LLVM build options
    #[serde(default)]
    pub option: HashMap<String, String>,
}

/// Describes how to compile LLVM/Clang
///
/// See also [module level document](index.html).
#[derive(Debug, PartialEq)]
pub enum Entry {
    Remote {
        name: String,
        version: Option<Version>,
        url: String,
        setting: EntrySetting,
    },
    Local {
        name: String,
        version: Option<Version>,
        path: PathBuf,
        setting: EntrySetting,
    },
}

fn load_entry_toml(toml_str: &str) -> Result<Vec<Entry>> {
    let entries: HashMap<String, EntrySetting> = toml::from_str(toml_str)?;
    entries
        .into_iter()
        .map(|(name, setting)| Entry::parse_setting(&name, Version::parse(&name).ok(), setting))
        .collect()
}

pub fn official_releases() -> Vec<Entry> {
    vec![
        Entry::official(18, 1, 0),
        Entry::official(17, 0, 2),
        Entry::official(17, 0, 0),
        Entry::official(16, 0, 6),
        Entry::official(16, 0, 0),
        Entry::official(15, 0, 7),
        Entry::official(15, 0, 0),
        Entry::official(14, 0, 6),
        Entry::official(14, 0, 0),
        Entry::official(13, 0, 0),
        Entry::official(12, 0, 1),
        Entry::official(12, 0, 0),
        Entry::official(11, 1, 0),
        Entry::official(11, 0, 0),
        Entry::official(10, 0, 1),
        Entry::official(10, 0, 0),
    ]
}

pub fn load_entries() -> Result<Vec<Entry>> {
    let global_toml = config_dir()?.join(ENTRY_TOML);
    let mut entries = load_entry_toml(&fs::read_to_string(&global_toml).with(&global_toml)?)?;
    let mut official = official_releases();
    entries.append(&mut official);
    Ok(entries)
}

pub fn load_entry(name: &str) -> Result<Entry> {
    let entries = load_entries()?;
    for entry in entries {
        if entry.name() == name {
            return Ok(entry);
        }

        if let Some(version) = entry.version() {
            if let Ok(req) = VersionReq::parse(name) {
                if req.matches(version) {
                    return Ok(entry);
                }
            }
        }
    }
    Err(Error::InvalidEntry {
        message: "Entry not found".into(),
        name: name.into(),
    })
}


impl Entry {
    /// Entry for official LLVM release
    pub fn official(major: u64, minor: u64, patch: u64) -> Self {
        let version = Version::new(major, minor, patch);
        let mut setting = EntrySetting::default();

        let base_url = format!(
            "https://github.com/llvm/llvm-project/archive/refs/tags/llvmorg-{}",
            version
        );

        setting.url = Some(format!("{}.tar.gz", base_url));

        let name = version.to_string();
        Entry::parse_setting(&name, Some(version), setting).unwrap()
    }

    fn parse_setting(name: &str, version: Option<Version>, setting: EntrySetting) -> Result<Self> {
        if setting.path.is_some() && setting.url.is_some() {
            return Err(Error::InvalidEntry {
                name: name.into(),
                message: "One of Path or URL are allowed".into(),
            });
        }
        if let Some(path) = &setting.path {
            return Ok(Entry::Local {
                name: name.into(),
                version,
                path: PathBuf::from(shellexpand::full(&path).unwrap().to_string()),
                setting,
            });
        }
        if let Some(url) = &setting.url {
            return Ok(Entry::Remote {
                name: name.into(),
                version,
                url: url.clone(),
                setting,
            });
        }
        Err(Error::InvalidEntry {
            name: name.into(),
            message: "Path nor URL are not found".into(),
        })
    }

    fn setting(&self) -> &EntrySetting {
        match self {
            Entry::Remote { setting, .. } => setting,
            Entry::Local { setting, .. } => setting,
        }
    }

    fn setting_mut(&mut self) -> &mut EntrySetting {
        match self {
            Entry::Remote { setting, .. } => setting,
            Entry::Local { setting, .. } => setting,
        }
    }

    pub fn set_builder(&mut self, generator: &str) -> Result<()> {
        let generator = CMakeGenerator::from_str(generator)?;
        self.setting_mut().generator = generator.clone();
        log::info!("CMake Generator: {:?}", generator);
        Ok(())
    }

    pub fn set_build_type(&mut self, build_type: BuildType) -> Result<()> {
        self.setting_mut().build_type = build_type;
        log::info!("Build type: {:?}", build_type);
        Ok(())
    }

    pub fn checkout(&self) -> Result<()> {
        match self {
            Entry::Remote { url, .. } => {
                log::info!("Checkout LLVM/Clang");
                let src = Resource::from_url(url)?;
                src.download(&self.src_dir()?, "llvm".to_string())?;

                log::info!("Checkout done");
            }
            Entry::Local { .. } => {}
        }
        Ok(())
    }

    pub fn clean_cache_dir(&self) -> Result<()> {
        let path = self.src_dir()?;
        info!("Remove cache dir: {}", path.display());
        fs::remove_dir_all(&path).with(&path)?;
        Ok(())
    }

    pub fn update(&self) -> Result<()> {
        match self {
            Entry::Remote { url, .. } => {
                let src = Resource::from_url(url)?;
                src.update(&self.src_dir()?)?;
            }
            Entry::Local { .. } => {}
        }
        Ok(())
    }

    pub fn name(&self) -> &str {
        match self {
            Entry::Remote { name, .. } => name,
            Entry::Local { name, .. } => name,
        }
    }

    pub fn version(&self) -> Option<&Version> {
        match self {
            Entry::Remote { version, .. } => version.as_ref(),
            Entry::Local { version, .. } => version.as_ref(),
        }
    }

    pub fn src_dir(&self) -> Result<PathBuf> {
        Ok(match self {
            Entry::Remote { name, .. } => cache_dir()?.join(name),
            Entry::Local { path, .. } => path.into(),
        })
    }

    pub fn build_dir(&self) -> Result<PathBuf> {
        let dir = self.src_dir()?.join("build");
        if !dir.exists() {
            info!("Created build dir: {}", dir.display());
            fs::create_dir_all(&dir).with(&dir)?;
        }
        Ok(dir)
    }

    pub fn clean_build_dir(&self) -> Result<()> {
        let path = self.build_dir()?;
        info!("Remove build dir: {}", path.display());
        fs::remove_dir_all(&path).with(&path)?;
        Ok(())
    }

    pub fn prefix(&self) -> Result<PathBuf> {
        Ok(data_dir()?.join(self.name()))
    }

    pub fn build(&self, nproc: usize) -> Result<()> {
        self.configure()?;
        let mut cmd = process::Command::new("cmake");

        cmd.args([
            "--build",
            &format!("{}", self.build_dir()?.display()),
            "--target",
            "install",
        ]);

        cmd.args(
            self
                .setting()
                .generator
                .build_option(nproc, self.setting().build_type),
        );

        log::debug!("Running: {:#?}", cmd);

        cmd.check_run()?;

        Ok(())
    }

    fn configure(&self) -> Result<()> {
        let setting = self.setting();
        let mut opts = setting.generator.option();
        opts.push(format!("{}", self.src_dir()?.display()));

        opts.push(format!(
            "-DCMAKE_INSTALL_PREFIX={}",
            data_dir()?.join(self.prefix()?).display()
        ));
        opts.push(format!("-DCMAKE_BUILD_TYPE={:?}", setting.build_type));

        // Enable ccache if exists
        if which::which("ccache").is_ok() {
            opts.push("-DLLVM_CCACHE_BUILD=ON".into());
        }

        // Enable lld if exists
        if which::which("lld").is_ok() {
            opts.push("-DLLVM_ENABLE_LLD=ON".into());
        }

        // Target architectures
        if !setting.target.is_empty() {
            opts.push(format!(
                "-DLLVM_TARGETS_TO_BUILD={}",
                setting.target.iter().join(";")
            ));
        }

        // Other options
        for (k, v) in &setting.option {
            opts.push(format!("-D{}={}", k, v));
        }

        let mut cmd = process::Command::new("cmake");

        cmd.args(&opts)
            .current_dir(self.build_dir()?);

        log::debug!("Running: {:#?}", cmd);

        Ok(())
    }
}
