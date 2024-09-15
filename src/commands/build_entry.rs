use crate::{entry, error::Result};
use crate::entry::BuildType;

pub fn build_entry_command(
    name: String,
    update: bool,
    clean: bool,
    discard: bool,
    builder: Option<String>,
    nproc: Option<usize>,
    build_type: Option<BuildType>,
) -> Result<()> {
    log::debug!("build_entry_command: name={}, update={}, clean={}, discard={}, builder={:?}, nproc={:?}, build_type={:?}",
        name, update, clean, discard, builder, nproc, build_type);
    
    let mut entry = entry::load_entry(&name)?;
    let nproc = nproc.unwrap_or_else(num_cpus::get);
    if let Some(builder) = builder {
        entry.set_builder(&builder)?;
    }
    if let Some(build_type) = build_type {
        entry.set_build_type(build_type)?;
    }
    if discard {
        entry.clean_cache_dir()?;
    }
    entry.checkout()?;
    if update {
        entry.update()?;
    }
    if clean {
        entry.clean_build_dir()?;
    }
    entry.build(nproc)?;

    Ok(())
}