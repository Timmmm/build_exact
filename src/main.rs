mod dag;
mod dag_walker;
mod buildinfo;
mod deno;
mod graphviz;

use anyhow::Result;
use dag::Target;
use env_logger::Builder;
use log::{info, warn};
use std::path::PathBuf;
use structopt::StructOpt;

use crate::dag::BuildDag;

#[derive(Debug, StructOpt)]
#[structopt(name = "build_exact", about = "Build with exact dependency tracking.")]
struct Opt {
    /// Config file to build with (required)
    #[structopt(parse(from_os_str))]
    config: PathBuf,

    /// RUST_LOG-style logging string, e.g. --log debug
    #[structopt(long)]
    log: Option<String>,

    /// Disable the filesystem sandbox.
    #[structopt(long)]
    no_sandbox: bool,

    /// Visualise build graph
    #[structopt(long)]
    visualise: bool,

    targets: Vec<Target>,
}

#[show_image::main]
fn main() -> Result<()> {
    let opt = Opt::from_args();

    Builder::new().parse_filters(&opt.log.unwrap_or_default()).init();

    // 1. Run `deno info --unstable --json buildinfo.ts` to find the dependencies.
    // 2. Check all their hashes.
    // 3. Compare to the hash in the JSON.
    // 4. If so re-run the deno command to regenerate the JSON.

    // 5. Build the DAG.
    // 6. Run all the commands as needed.

    info!("Hashing buildinfo");

    let _build_info_hash = deno::hash_buildinfo(&opt.config)?;

    info!("Running buildinfo");

    // TODO: We need some way of saving the build info hash.
    // if build_info_hash != existing_hash {
    let build_info = deno::run_buildinfo(&opt.config)?;
    // }

    info!("Building");

    let dag = BuildDag::new(&build_info)?;

    if opt.targets.is_empty() {
        warn!("No targets selected, try adding `all`");
    }

    dag.build(&opt.targets, opt.no_sandbox, opt.visualise)?;

    Ok(())
}


// TODO:

// 1. Switch to Starlark Rust: https://github.com/facebookexperimental/starlark-rust
// 2. Dynamic dependency discovery. Basically when we depend on certain
//    files (e.g. foo.cpp) we run a rule on them first that will determine
//    their dependencies.
//    Nah that's tricky because the rule must be able to do anything so
//    the whole thing is no longer hermetic.
//  So scratch that, we'll just use Typescript.
//
// Also, 3: Use SQLite for storing build info.
