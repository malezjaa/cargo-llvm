//! Get remote LLVM/Clang source

use futures::{
    executor::{block_on_stream, BlockingStream},
    Stream,
};
use indicatif::{ProgressBar, ProgressStyle};
use log::*;
use std::{fs, io, path::*, process::Command};
use std::fs::File;
use std::io::{Read, Write};
use flate2::read::GzDecoder;
use tar::Archive;
use tempfile::TempDir;
use url::Url;
use crate::config::cache_dir;
use crate::error::*;

/// Remote LLVM/Clang resource
#[derive(Debug, PartialEq)]
pub enum Resource {
    /// Remote Subversion repository
    Svn { url: String },
    /// Remote Git repository
    Git { url: String, branch: Option<String> },
    /// Tar archive
    Tar { url: String },
}

impl Resource {
    /// Detect remote resorce from URL
    ///
    /// - Official subversion repository
    ///
    /// ```
    /// # use llvmenv::resource::Resource;
    /// let llvm_official_url = "http://llvm.org/svn/llvm-project/llvm/trunk";
    /// let svn = Resource::from_url(llvm_official_url).unwrap();
    /// assert_eq!(svn, Resource::Svn { url: llvm_official_url.into() });
    /// ```
    ///
    /// - GitHub
    ///
    /// ```
    /// # use llvmenv::resource::Resource;
    /// let github_mirror = "https://github.com/llvm/llvm-project";
    /// let git = Resource::from_url(github_mirror).unwrap();
    /// assert_eq!(git, Resource::Git { url: github_mirror.into(), branch: None });
    /// ```
    ///
    /// - Tar Archive
    ///
    /// ```
    /// # use llvmenv::resource::Resource;
    /// let tar_url = "http://releases.llvm.org/6.0.1/llvm-6.0.1.src.tar.xz";
    /// let tar = Resource::from_url(tar_url).unwrap();
    /// assert_eq!(tar, Resource::Tar { url: tar_url.into() });
    /// ```
    pub fn from_url(url_str: &str) -> Result<Self> {
        // Check file extension
        if let Ok(filename) = get_filename_from_url(url_str) {
            for ext in &[".tar.gz", ".tar.xz", ".tar.bz2", ".tar.Z", ".tgz", ".taz"] {
                if filename.ends_with(ext) {
                    debug!("Find archive extension '{}' at the end of URL", ext);
                    return Ok(Resource::Tar {
                        url: url_str.into(),
                    });
                }
            }

            if filename.ends_with("trunk") {
                debug!("Find 'trunk' at the end of URL");
                return Ok(Resource::Svn {
                    url: url_str.into(),
                });
            }

            if filename.ends_with(".git") {
                debug!("Find '.git' extension");
                return Ok(Resource::Git {
                    url: strip_branch_from_url(url_str)?,
                    branch: get_branch_from_url(url_str)?,
                });
            }
        }

        // Hostname
        let url = Url::parse(url_str).map_err(|_| Error::InvalidUrl {
            url: url_str.into(),
        })?;
        for service in &["github.com", "gitlab.com"] {
            if url.host_str() == Some(service) {
                debug!("URL is a cloud git service: {}", service);
                return Ok(Resource::Git {
                    url: strip_branch_from_url(url_str)?,
                    branch: get_branch_from_url(url_str)?,
                });
            }
        }

        if url.host_str() == Some("llvm.org") {
            if url.path().starts_with("/svn") {
                debug!("URL is LLVM SVN repository");
                return Ok(Resource::Svn {
                    url: url_str.into(),
                });
            }
            if url.path().starts_with("/git") {
                debug!("URL is LLVM Git repository");
                return Ok(Resource::Git {
                    url: strip_branch_from_url(url_str)?,
                    branch: get_branch_from_url(url_str)?,
                });
            }
        }

        // Try access with git
        //
        // - SVN repository cannot handle git access
        // - Some Git service (e.g. GitHub) *can* handle svn access
        //
        // ```
        // git init
        // git remote add $url
        // git ls-remote       # This must fail for SVN repo
        // ```
        debug!("Try access with git to {}", url_str);
        let tmp_dir = TempDir::new().with("/tmp")?;
        Command::new("git")
            .arg("init")
            .current_dir(tmp_dir.path())
            .silent()
            .check_run()?;
        Command::new("git")
            .args(["remote", "add", "origin"])
            .arg(url_str)
            .current_dir(tmp_dir.path())
            .silent()
            .check_run()?;
        match Command::new("git")
            .args(["ls-remote"])
            .current_dir(tmp_dir.path())
            .silent()
            .check_run()
        {
            Ok(_) => {
                debug!("Git access succeeds");
                Ok(Resource::Git {
                    url: strip_branch_from_url(url_str)?,
                    branch: get_branch_from_url(url_str)?,
                })
            }
            Err(_) => {
                debug!("Git access failed. Regarded as a SVN repository.");
                Ok(Resource::Svn {
                    url: url_str.into(),
                })
            }
        }
    }

    pub fn download(&self, dest: &Path, tool_name: String) -> Result<()> {
        if !dest.exists() {
            fs::create_dir_all(dest).with(dest)?;
        }
        if !dest.is_dir() {
            return Err(io::Error::new(io::ErrorKind::Other, "Not a directory")).with(dest);
        }

        match self {
            Resource::Svn { url, .. } => Command::new("svn")
                .args(["co", url.as_str(), "-r", "HEAD"])
                .arg(dest)
                .check_run()?,
            Resource::Git { url, branch } => {
                info!("Git clone {}", url);
                let mut git = Command::new("git");
                git.args(["clone", url.as_str(), "-q", "--depth", "1"])
                    .arg(dest);
                if let Some(branch) = branch {
                    git.args(["-b", branch]);
                }
                git.check_run()?;
            }
            Resource::Tar { url } => {
                let filename = get_filename_from_url(url)?;
                let cache_dir = cache_dir()?.join("cache");

                if !cache_dir.exists() {
                    fs::create_dir_all(&cache_dir).with(&cache_dir)?;
                }

                let tar_file = cache_dir.join(&filename);

                if tar_file.exists() {
                    info!("Using cached tar file: {}", tar_file.display());
                } else {
                    info!("Downloading tar file: {}", url);
                    let rt = tokio::runtime::Runtime::new()?;
                    let bytes = rt.block_on(download(url))?;

                    drop(rt);

                    fs::write(&tar_file, &bytes)?;

                    info!("Tar file cached: {}", tar_file.display());
                }

                let tar_gz = File::open(
                    &tar_file
                )?;
                let tar = GzDecoder::new(tar_gz);
                let mut archive = Archive::new(tar);
                let entries = archive
                    .entries()
                    .expect("Tar archive does not contain entries");

                let bar = ProgressBar::new_spinner();
                bar.set_style(
                    ProgressStyle::default_spinner()
                        .template("{spinner:.green} [{elapsed_precise}] Unpacking: {msg} [{pos}]")
                        .expect("Invalid template")
                );

                for entry in entries {
                    let mut entry = entry.expect("Invalid tar entry");
                    let path = entry.path().expect("Invalid unicode in filename");

                    bar.set_message(path.to_string_lossy().to_string());

                    let mut target = dest.to_owned().join(&tool_name);
                    for comp in path.components().skip(1) {
                        target = target.join(comp);
                    }
                    if let Err(e) = entry.unpack(&target) {
                        match e.kind() {
                            io::ErrorKind::AlreadyExists => debug!("{:?}", e.to_string()),
                            _ => warn!("{:?}", e.to_string()),
                        }
                    }

                    bar.inc(1);
                }

                bar.finish_with_message("Unpacking completed");
            }
        }
        Ok(())
    }

    pub fn update(&self, dest: &Path) -> Result<()> {
        match self {
            Resource::Svn { .. } => Command::new("svn")
                .arg("update")
                .current_dir(dest)
                .check_run()?,
            Resource::Git { .. } => Command::new("git")
                .arg("pull")
                .current_dir(dest)
                .check_run()?,
            Resource::Tar { .. } => {}
        }
        Ok(())
    }
}

async fn download(url: &str) -> Result<Vec<u8>> {
    let req = reqwest::get(url).await?;
    let status = req.status();
    if !status.is_success() {
        return Err(Error::HttpError {
            url: url.into(),
            status,
        });
    }

    let content_length = req
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|len| len.to_str().ok()?.parse().ok())
        .unwrap_or(0);

    let bar = ProgressBar::new(content_length)
        .with_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:38.cyan/blue}] {bytes}/{total_bytes} ({eta}) [{bytes_per_sec}]")
                ?
                .progress_chars("#>-")
        );

    let mut bytes: Vec<u8> = Vec::new();
    let stream = block_on_stream(req.bytes_stream());

    for chunk in stream {
        let chunk = chunk.map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
        bar.inc(chunk.len() as u64);
        bytes.extend_from_slice(&chunk);
    }

    bar.finish();
    Ok(bytes)
}


fn get_filename_from_url(url_str: &str) -> Result<String> {
    let url = ::url::Url::parse(url_str).map_err(|_| Error::InvalidUrl {
        url: url_str.into(),
    })?;
    let seg = url.path_segments().ok_or(Error::InvalidUrl {
        url: url_str.into(),
    })?;
    let filename = seg.last().ok_or(Error::InvalidUrl {
        url: url_str.into(),
    })?;
    Ok(filename.to_string())
}

fn get_branch_from_url(url_str: &str) -> Result<Option<String>> {
    let url = ::url::Url::parse(url_str).map_err(|_| Error::InvalidUrl {
        url: url_str.into(),
    })?;
    Ok(url.fragment().map(ToOwned::to_owned))
}

fn strip_branch_from_url(url_str: &str) -> Result<String> {
    let mut url = ::url::Url::parse(url_str).map_err(|_| Error::InvalidUrl {
        url: url_str.into(),
    })?;
    url.set_fragment(None);
    Ok(url.into())
}

