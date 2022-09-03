use crate::buildinfo::BuildInfo;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::{path::Path, process::{Command, Stdio}};

#[derive(Serialize, Deserialize)]
pub struct DenoInfo {
    pub root: String,
    pub size: u64,
    pub modules: Vec<DenoInfoModule>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DenoInfoModule {
    pub specifier: String,
    pub dependencies: Vec<DenoInfoDependency>,
    pub size: u64,
    pub media_type: String,
    pub local: String,
    pub checksum: String,
    pub emit: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DenoInfoDependency {
    pub specifier: String,
    pub code: String,
}

/// Run `deno info <file>` to get the hashes of all the modules and hash them
/// all together.
pub fn hash_buildinfo(file: &Path) -> Result<String> {
    let output = Command::new("deno")
        .stderr(Stdio::inherit())
        .arg("info")
        .arg("--unstable")
        .arg("--json")
        .arg("--")
        .arg(file)
        .output()?;

    if !output.status.success() {
        bail!(
            "`deno info` failing with exit status: {}",
            output.status
        );
    }

    let deno_info: DenoInfo = serde_json::from_slice(&output.stdout)?;

    // Just concat all the hashes.
    let mut all_hashes = String::new();
    for info in deno_info.modules.iter() {
        all_hashes.push_str(&info.checksum);
    }
    return Ok(all_hashes);
}

/// Run `deno <file>`, gather and decode BuildInfo.
pub fn run_buildinfo(file: &Path) -> Result<BuildInfo> {
    let output = Command::new("deno")
        .stderr(Stdio::inherit())
        .arg("run")
        .arg("--allow-read")
        .arg("--")
        .arg(file)
        .output()?;

    if !output.status.success() {
        bail!(
            "`deno run` failed with exit code: {}",
            output.status
        );
    }

    let build_info: BuildInfo = serde_json::from_slice(&output.stdout)?;

    Ok(build_info)
}
