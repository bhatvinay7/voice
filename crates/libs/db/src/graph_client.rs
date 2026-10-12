use crate::AdminGraphNode;
use neo4rs::{query, ConfigBuilder, Graph};
use std::sync::Arc;

/// Connects to a Neo4j / Memgraph graph database using provided credentials
pub async fn connect_graph_db(
    uri: &str,
    user: &str,
    pass: &str,
    database: Option<&str>,
) -> Result<Arc<Graph>, String> {
    let mut builder = ConfigBuilder::default()
        .uri(uri)
        .user(user)
        .password(pass);

    if let Some(db_name) = database {
        builder = builder.db(db_name);
    }

    let config = builder.build().map_err(|e| format!("Graph config error: {}", e))?;
    let graph = Graph::connect(config).await.map_err(|e| format!("Graph connection error: {}", e))?;
    Ok(Arc::new(graph))
}

/// Pushes workflow nodes and DAG relationships to the Graph DB
pub async fn push_workflow_dag_to_graph(
    graph: &Graph,
    nodes: &[AdminGraphNode],
) -> Result<(), String> {
    for node in nodes {
        let q = query("MERGE (n:WorkflowNode {id: $id}) SET n.step_type = $step_type, n.instruction = $instruction, n.status = $status, n.is_preset = $is_preset")
            .param("id", node.id.clone())
            .param("step_type", node.step_type.clone())
            .param("instruction", node.instruction.clone())
            .param("status", node.status.clone())
            .param("is_preset", node.is_preset);
        graph.run(q).await.map_err(|e| format!("Failed to push node {}: {}", node.id, e))?;
    }

    // Connect nodes sequentially in DAG: (a)-[:PRECEDES]->(b)
    for window in nodes.windows(2) {
        let from_id = window[0].id.clone();
        let to_id = window[1].id.clone();
        let edge_q = query("MATCH (a:WorkflowNode {id: $from_id}), (b:WorkflowNode {id: $to_id}) MERGE (a)-[r:PRECEDES]->(b)")
            .param("from_id", from_id)
            .param("to_id", to_id);
        let _ = graph.run(edge_q).await;
    }

    Ok(())
}
