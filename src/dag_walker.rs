use petgraph::{Graph, graph::NodeIndex, Direction};

/// Walk all of a node's outgoing/incoming edges recursively. Call `func` for each node, and
/// only walk that node's outgoing/incoming edges if `func` returns true.
/// It starts by calling `func(starting_node_index)` so if that returns
/// `false` no other nodes will be walked.
///
/// This will probably do weird things if the graph contains loops.
///
/// I suspect there's a way to do this using petgraph's Dfs and NodeFilteredNode
/// types but there's no documentation and it isn't obvious. Easier to write it
/// myself.
pub fn walk_recursively<N, E, F>(dag: &Graph<N, E>, starting_node_index: NodeIndex, direction: Direction, mut func: F)
    where
        F: FnMut(NodeIndex) -> bool,
{
    let mut pending: Vec<NodeIndex> = vec![starting_node_index];

    while let Some(node_index) = pending.pop() {
        if func(node_index) {
            pending.extend(dag.neighbors_directed(node_index, direction));
        }
    }
}
