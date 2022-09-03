use crate::buildinfo::{BuildCommand, BuildInfo, TestCommand};
use crate::dag_walker::walk_recursively;
use crate::graphviz::show_graphviz;
use anyhow::{anyhow, bail, Result};
use petgraph::algo::is_cyclic_directed;
use petgraph::dot::{Config, Dot};
use petgraph::visit::IntoNodeReferences;
use petgraph::{Direction, Graph, graph::NodeIndex};
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::str::FromStr;
use std::time::SystemTime;
use log::{info, debug, error};

// Hmm the graph nodes are commands, and the *edges* are files.

// 1. Do a recursive walk from the target nodes up to their dependencies.
// 2. Add all of the commands to a map of commands to the number of dependencies
//    that are not yet done (initially this is all of them).
//    For any that are leaf commands (no dependencies), add them to a "ready to run"
//    heap.
// 3. Repeatedly pick commands from the "ready to run" heap and run them until
//    the heap is empty.
//    * Executing a command is a nop, if all of its inputs are older than its output
//    * Commands can be prioritised by estimated time, whether they are on the
//      critical path, etc.
// 4. When a command finishes, find all its dependants and decrement their count
//    of dependencies that need to be done. If it is zero, move it to the
//    "ready to run" heap.

type InputFileIndex = usize;

enum CommandIndex {
    BuildCommandIndex(usize),
    TestCommandIndex(usize),
}

pub struct BuildDag<'a> {
    info: &'a BuildInfo,
    /// The DAG. Also, we have the index of the input file for the
    /// command that this goes into for debugging purposes.
    dag: Graph::<CommandIndex, InputFileIndex>,
    /// Map from output file to the command that generates it.
    output_file_generators: HashMap::<String, NodeIndex>,
    /// Map from input file to the set of commands that consume them.
    input_file_consumers: HashMap::<String, Vec<NodeIndex>>,
    /// List of test names. This provides the "test index" since in `info`
    /// they are in a HashMap.
    test_names: Vec<String>,

    /// Node index for each build command.
    build_command_node_index: Vec<NodeIndex>,
    /// Node index for each build command.
    test_command_node_index: Vec<NodeIndex>,

}

/// A thing that we might want to build.
#[derive(Debug)]
pub enum Target {
    /// Build the file (and all of its dependencies).
    Output(String),
    /// Build all output files that depend on the file (including transitively).
    OutputsThatDependOnFile(String),
    /// Build all outputs.
    AllOutputs,
    /// Run a test.
    Test(String),
    /// Run all tests that depend on the file (including transitively).
    TestsThatDependOnFile(String),
    /// Run all tests.
    AllTests,
}

impl FromStr for Target {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.split_once(':').unwrap_or((s, "")) {
            ("output_all", "") => Target::AllOutputs,
            ("test_all", "") => Target::AllTests,
            ("output", file) => Target::Output(file.to_owned()),
            ("test", test) => Target::Test(test.to_owned()),
            ("output_dependencies", file) => Target::OutputsThatDependOnFile(file.to_owned()),
            ("test_dependencies", file) => Target::TestsThatDependOnFile(file.to_owned()),
            _ => bail!("Unknown option: {}", s),
        })
    }
}

impl<'a> BuildDag<'a> {
    pub fn new(info: &'a BuildInfo) -> Result<Self> {
        ensure_absolute_normalised_paths(info)?;

        let mut bd = Self {
            info,
            dag: Graph::new(),
            output_file_generators: HashMap::new(),
            input_file_consumers: HashMap::new(),
            test_names: Vec::with_capacity(info.tests.len()),
            build_command_node_index: Vec::with_capacity(info.commands.len()),
            test_command_node_index: Vec::with_capacity(info.tests.len()),
        };

        // Go through all the commands and add all the inputs and output files.
        // Output files can only be from one command so return an error if we try
        // to the same output file twice.

        for (build_command_index, command) in info.commands.iter().enumerate() {
            let node_index = bd.dag.add_node(CommandIndex::BuildCommandIndex(build_command_index));
            bd.build_command_node_index.push(node_index);

            for output in command.outputs.iter() {
                if bd.output_file_generators
                    .insert(output.clone(), node_index)
                    .is_some()
                {
                    bail!(
                        "File '{}' is specified as the output of more than one command.",
                        output
                    );
                }
            }
        }

        // Now add the build command edges.
        for (build_command_index, command) in info.commands.iter().enumerate() {
            let node_index = bd.build_command_node_index[build_command_index];

            for (input_index, input) in command.inputs.iter().enumerate() {
                // Add an edge pointing to the command that generates this file (if any;
                // it might be a source file).
                if let Some(parent_index) = bd.output_file_generators.get(input) {
                    bd.dag.add_edge(*parent_index, node_index, input_index);
                }

                // Record a map from input file to the build commands that use it.
                bd.input_file_consumers.entry(input.clone()).or_default().push(node_index);
            }
        }

        // Collect the test names so we can iterate over them in a consistent order
        // and look them up by index.
        bd.test_names = info.tests.keys().map(ToOwned::to_owned).collect();

        // Add tests directly after build commands.
        for test_command_index in 0..bd.test_names.len() {
            let node_index = bd.dag.add_node(CommandIndex::TestCommandIndex(test_command_index));
            bd.test_command_node_index.push(node_index);
        }

        // Add the test input edges.
        for (test_command_index, test_name) in bd.test_names.iter().enumerate() {
            let node_index = bd.test_command_node_index[test_command_index];

            let command = &info.tests[test_name];

            for (input_index, input) in command.inputs.iter().enumerate() {
                // Add an edge pointing to the command that generates this file (if any;
                // it might be a source file).
                if let Some(parent_index) = bd.output_file_generators.get(input) {
                    bd.dag.add_edge(*parent_index, node_index, input_index);
                }

                // Record a map from input file to the test commands that use it.
                bd.input_file_consumers.entry(input.clone()).or_default().push(node_index);
            }
        }

        // Now ensure it is a dag.
        ensure_not_cyclic(&bd.dag)?;

        Ok(bd)
    }

    /// Add the target commands to the set of commands that needs to be built.
    fn add_target_commands(&self, target: &Target, to: &mut HashSet<NodeIndex>) -> Result<()> {
        match target {
            Target::Output(path) => {
                // Build the output path (so we need to build all of its
                // dependencies).
                let generator_node = self.output_file_generators.get(path).ok_or_else(|| anyhow!("No command generates output {:?}", path))?;
                walk_recursively(&self.dag, *generator_node, Direction::Incoming, |node_index| {
                    to.insert(node_index)
                });
            }
            Target::OutputsThatDependOnFile(path) => {
                // Build all the dependencies and dependents of `path`.
                // And we'll need to build their dependencies too.
                let consumer_nodes = self.input_file_consumers.get(path).ok_or_else(|| anyhow!("No command use file {:?}", path))?;
                for consumer_node in consumer_nodes {
                    walk_recursively(&self.dag, *consumer_node, Direction::Outgoing, |outgoing_node_index| {
                        let node_weight = self.dag.node_weight(outgoing_node_index).expect("Internal logic error 1");
                        match node_weight {
                            CommandIndex::BuildCommandIndex(_) => {
                                if to.insert(outgoing_node_index) {
                                    walk_recursively(&self.dag, outgoing_node_index, Direction::Incoming, |incoming_node_index| {
                                        to.insert(incoming_node_index)
                                    });
                                    true
                                } else {
                                    false
                                }
                            }
                            CommandIndex::TestCommandIndex(_) => false,
                        }
                    });
                }
            }
            Target::AllOutputs => {
                // Add all build commands.
                for (node_index, node_weight) in self.dag.node_references() {
                    match node_weight {
                        CommandIndex::BuildCommandIndex(_) => {
                            to.insert(node_index);
                        }
                        CommandIndex::TestCommandIndex(_) => {},
                    }
                }
            }
            Target::Test(test) => {
                // Run one test (so we need to build all of its dependencies).

                // Get the test command index.
                // TODO: HashMap so we don't need linear search.
                let test_command_index = self.test_names.iter().position(|n| n == test).ok_or_else(|| anyhow!("Test {} not found", test))?;
                let test_node_index = self.test_command_node_index[test_command_index];
                // Add the test and its dependencies.
                walk_recursively(&self.dag, test_node_index, Direction::Incoming, |node_index| {
                    info!("Adding node: {:?}", test_node_index);
                    to.insert(node_index)
                });
            }
            Target::TestsThatDependOnFile(path) => {
                // Run all tests that depend on the file, so we need to build
                // all the dependencies of those tests too.

                // Note that this doesn't necessarily mean that we build all
                // outputs that depend on the file because some might not
                // be tested.

                // Get the set of tests to run.
                let mut tests_to_run: HashSet<NodeIndex> = HashSet::new();
                let generator_node = self.output_file_generators.get(path).ok_or_else(|| anyhow!("No command generates output {:?}", path))?;
                walk_recursively(&self.dag, *generator_node, Direction::Outgoing, |outgoing_node_index| {
                    let node_weight = self.dag.node_weight(outgoing_node_index).expect("Internal logic error 0");
                    match node_weight {
                        CommandIndex::BuildCommandIndex(_) => false,
                        CommandIndex::TestCommandIndex(_) => {
                            tests_to_run.insert(outgoing_node_index);
                            true
                        }
                    }
                });

                // Now add their dependencies.
                for test_to_run in tests_to_run.iter() {
                    walk_recursively(&self.dag, *test_to_run, Direction::Incoming, |node_index| {
                        to.insert(node_index)
                    });
                }
            }
            Target::AllTests => {
                // Run all tests (and build all of their dependencies).
                // Note that this isn't necessarily all build commands
                // (like for AllOutputs) because some outputs may not be
                // tested.
                for test_command_index in 0..self.test_names.len() {
                    let test_node_index = self.test_command_node_index[test_command_index];
                    walk_recursively(&self.dag, test_node_index, Direction::Incoming, |node_index| {
                        to.insert(node_index)
                    });
                }
            }
        }
        Ok(())
    }

    /// Build files and run tests, depending on the value of targets.
    pub fn build(&self, targets: &[Target], no_sandbox: bool, visualise: bool) -> Result<()> {

        let mut commands_to_run: HashSet<NodeIndex> = HashSet::with_capacity(self.dag.node_count());
        for target in targets {
            self.add_target_commands(target, &mut commands_to_run)?;
        }

        // Map from command index (into info.commands) to the number of its
        // inputs that still need to be updated.
        let mut command_dependencies_remaining =
            HashMap::<NodeIndex, usize>::with_capacity(commands_to_run.len());

        // Commands that are ready to run. TODO: Sort in priority order: BinaryHeap::<(CommandPriority, NodeIndex)>, with CommandPriority = i32.
        let mut ready_to_run = BinaryHeap::<NodeIndex>::new();

        for command_index in &commands_to_run {
            let dependencies = self.dag.neighbors_directed(*command_index, Direction::Incoming).count();

            if dependencies == 0 {
                ready_to_run.push(*command_index);
            } else {
                command_dependencies_remaining.insert(*command_index, dependencies);
            }
        }

        // Show visualisation if requested.
        if visualise {
            self.show_visualisation(&commands_to_run)?;
        }

        // Now we can start building!

        // TODO: This can easily be multithreaded.
        while let Some(node_index) = ready_to_run.pop() {
            let node_weight = self.dag.node_weight(node_index).expect("Internal logic error 2");
            match node_weight {
                CommandIndex::BuildCommandIndex(build_command_index) => {
                    run_command_if_necessary(&self.info.commands[*build_command_index], &self.info.sandboxed_dirs, no_sandbox)?;
                }
                CommandIndex::TestCommandIndex(test_command_index) => {
                    let test_name = &self.test_names[*test_command_index];
                    let test_result = run_test(&self.info.tests[test_name], &self.info.sandboxed_dirs, no_sandbox)?;
                    if !test_result.success() {
                        error!("Test failed! Exit status: {:?}", test_result.code());
                    }
                },
            }

            // Now decrement the required number of dependencies for its dependants.
            for child_index in self.dag.neighbors_directed(node_index, Direction::Outgoing) {
                if commands_to_run.contains(&child_index) {
                    let remaining = command_dependencies_remaining
                        .get_mut(&child_index)
                        .expect("Internal logic error 5");

                    *remaining -= 1;
                    if *remaining == 0 {
                        command_dependencies_remaining.remove(&child_index);
                        ready_to_run.push(child_index);
                    }
                }
            }
        }

        assert!(command_dependencies_remaining.is_empty());

        Ok(())
    }

    fn show_visualisation(&self, highlight_commands: &HashSet<NodeIndex>) -> Result<()> {
        // Map the graph node/edges to strings. See
        // https://github.com/petgraph/petgraph/issues/194

        let labelled_graph = self.dag.map(
            |_node_index, node_weight| {
                match node_weight {
                    CommandIndex::BuildCommandIndex(build_command_index) => {
                        self.info.commands[*build_command_index].command.join(" ")
                    }
                    CommandIndex::TestCommandIndex(test_command_index) => {
                        self.test_names[*test_command_index].clone()
                    }
                }
            },
            |edge_index, edge_weight| {
                // Get the node that this points to.
                let (_, incoming_node_index) = self.dag.edge_endpoints(edge_index).expect("Internal logic error 4");
                let incoming_node_weight = self.dag.node_weight(incoming_node_index).expect("Internal logic error 5");
                let input = match incoming_node_weight {
                    CommandIndex::BuildCommandIndex(build_command_index) => {
                        &self.info.commands[*build_command_index].inputs[*edge_weight]
                    }
                    CommandIndex::TestCommandIndex(test_command_index) => {
                        let test_name = &self.test_names[*test_command_index];
                        &self.info.tests[test_name].inputs[*edge_weight]
                    }
                };
                input.split('/').last().expect("Internal logic error").clone()
            }
        );

        let get_node_attributes = |_graph, (node_index, _node_weight)| {
            // Get node weight of the original graph.
            let mut attr = match self.dag.node_weight(node_index).expect("Internal logic error 3") {
                CommandIndex::BuildCommandIndex(_) => "shape=box, style=rounded",
                CommandIndex::TestCommandIndex(_) => "shape=box, style=\"rounded,filled\", fillcolor=yellow",
            }.to_string();
            if highlight_commands.contains(&node_index) {
                attr.push_str(", color=red");
            }
            attr
        };

        let dot = Dot::with_attr_getters(
            &labelled_graph,
            &[Config::GraphContentOnly],
            &|_graph, _edge_ref| {
                "".to_string()
            },
            &get_node_attributes,
        );
        let dot_str = format!(
            "
digraph {{
    rankdir=LR;
    {}
}}
            ",
            dot,
        );

        show_graphviz(&dot_str)?;
        Ok(())
    }
}

/// Verify that all paths in the buildinfo are absolute and don't have any ..s
/// in them. That makes everything way easier, and Typescript can easily take
/// care of it.
fn ensure_absolute_normalised_paths(info: &BuildInfo) -> Result<()> {
    fn check_path(path: &Path) -> Result<()> {
        if !path.is_absolute() {
            bail!("Path {:?} must be absolute.", path);
        }
        if path.iter().any(|component| component == ".." || component == ".") {
            bail!("Path {:?} must be canonical (no .. or .).", path);
        }
        Ok(())
    }

    for command in info.commands.iter() {
        for input in command.inputs.iter() {
            check_path(Path::new(input))?;
        }
        for output in command.outputs.iter() {
            check_path(Path::new(output))?;
        }
        check_path(Path::new(&command.working_dir))?;
    }

    for dir in info.sandboxed_dirs.iter() {
        check_path(Path::new(dir))?;
    }

    Ok(())
}

/// Return an error if the directed graph is cyclic.
fn ensure_not_cyclic<NW, EW>(graph: &Graph<NW, EW>) -> Result<()> {
    if is_cyclic_directed(graph) {
        // TODO: Better error.
        bail!("Build graph is cyclic");
    }
    Ok(())
}

fn rerun_necessary(command: &BuildCommand) -> bool {
    // Set the max time to zero; if a command has no declared outputs then we
    // don't know when it was last run so we always need to re-run it. This
    // could include tests for example.
    let mut max_output_mtime = SystemTime::UNIX_EPOCH;
    for file in command.outputs.iter() {
        let metadata = match fs::metadata(file) {
            Ok(m) => m,
            // Probably doesn't exist.
            Err(_) => return true,
        };

        let mtime = match metadata.modified() {
            Ok(m) => m,
            // Probably fs doesn't support mtimes?
            Err(_) => return true,
        };

        max_output_mtime = std::cmp::max(max_output_mtime, mtime);
    }

    for file in command.inputs.iter() {
        let metadata = match fs::metadata(file) {
            Ok(m) => m,
            // Probably doesn't exist.
            Err(_) => return true,
        };

        let mtime = match metadata.modified() {
            Ok(m) => m,
            // Probably fs doesn't support mtimes?
            Err(_) => return true,
        };

        if mtime > max_output_mtime {
            return true;
        }
    }
    return false;
}

// Run the command but only if at least one of its inputs has a more recent
// mtime (modified time) than its any of its outputs.
fn run_command_if_necessary(command: &BuildCommand, sandboxed_dirs: &[String], no_sandbox: bool) -> Result<()> {
    if !rerun_necessary(command) {
        debug!("Skipping command (output is already up to date): {:?}", command.command);
        return Ok(());
    }
    info!("Running command: {:?}", command.command);

    if command.command.is_empty() {
        bail!("Command is empty");
    }

    let mut c = if no_sandbox {
        Command::new(&command.command[0])
    } else {
        let mut sbc = Command::new("sandbox");
        sbc.arg("--sandbox");
        sbc.args(sandboxed_dirs);
        sbc.arg("--allow-read");
        sbc.args(&command.inputs);
        sbc.arg("--allow-write");
        sbc.args(&command.outputs);
        sbc.arg("--");
        sbc.args(&command.command);

        debug!("Sandboxed command: {:?}", sbc);

        sbc
    };

    c.stderr(Stdio::inherit());
    c.current_dir(&command.working_dir);
    // TODO: Clear the environment probably.
    // c.env_clear();
    c.envs(&command.env);

    c.args(command.command.iter().skip(1));

    let output = c.output()?;

    if !output.status.success() {
        bail!(
            "Build command failed with exit status: {}",
            output.status
        );
    }

    Ok(())
}


fn run_test(command: &TestCommand, sandboxed_dirs: &[String], no_sandbox: bool) -> Result<ExitStatus> {
    info!("Running test: {:?}", command.command);

    if command.command.is_empty() {
        bail!("Command is empty");
    }

    let mut c = if no_sandbox {
        Command::new(&command.command[0])
    } else {
        let mut sbc = Command::new("sandbox");
        sbc.arg("--sandbox");
        sbc.args(sandboxed_dirs);
        sbc.arg("--allow-read");
        sbc.args(&command.inputs);
        sbc.arg("--");
        sbc.args(&command.command);

        debug!("Sandboxed command: {:?}", sbc);

        sbc
    };

    c.stderr(Stdio::inherit());
    c.current_dir(&command.working_dir);
    // TODO: Clear the environment probably.
    // c.env_clear();
    c.envs(&command.env);

    c.args(command.command.iter().skip(1));

    let output = c.output()?;

    Ok(output.status)
}
