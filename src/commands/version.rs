use crate::error::Result;
use crate::{build, get_existing_build};

pub fn version_command(
    name: Option<String>,
    major: bool,
    minor: bool,
    patch: bool,
) -> Result<()> {
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

    Ok(())
}