use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// A build command. All paths are absolute.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildCommand {
    /// Command to run.
    pub command: Vec<String>,
    /// All files that it reads inside the sandboxed dirs.
    pub inputs: Vec<String>,
    /// All files the it writes inside the sandboxed dirs (it can also read these).
    pub outputs: Vec<String>,
    /// The working dir.
    pub working_dir: String,
    /// The environment variables. Currently this is in addition to the ambient
    /// environment but at some point it would make sense to clean it.
    pub env: HashMap<String, String>,
}

/// A test. All paths are absolute.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestCommand {
    /// Command to run.
    pub command: Vec<String>,
    /// All files that it reads inside the sandboxed dirs.
    pub inputs: Vec<String>,
    /// The working dir.
    pub working_dir: String,
    /// The environment variables. Currently this is in addition to the ambient
    /// environment but at some point it would make sense to clean it.
    pub env: HashMap<String, String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildInfo {
    /// List of commands (nodes in the build graph).
    pub commands: Vec<BuildCommand>,
    /// List of tests. These are root nodes of the graph. They have no outputs
    /// and always run (if requested).
    pub tests: HashMap<String, TestCommand>,
    /// List of directories to protect. Everything outside these directories
    /// can be read and written without explicitly declaring it in
    /// BuildCommands.inputs/outputs.
    pub sandboxed_dirs: Vec<String>,
}
