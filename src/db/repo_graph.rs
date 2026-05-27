use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::error::Result;
use crate::model::graph::{GraphEdge, GraphNode};

pub async fn delete_session(pool: &SqlitePool, session_id: &str) -> Result<()> {
    let mut tx = pool.begin().await?;
    delete_session_in_tx(&mut tx, session_id).await?;
    tx.commit().await?;
    Ok(())
}

/// Slice-9 — tx-aware DELETE so `rebuild_session` can hold the whole
/// rebuild (DELETE then INSERT) inside a single transaction. Concurrent
/// SELECTs see the pre-state row count until commit, never zero rows mid
/// rebuild (DEV-S8-12 fix).
pub async fn delete_session_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    session_id: &str,
) -> Result<()> {
    sqlx::query("DELETE FROM graph_edge WHERE session_id = ?")
        .bind(session_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM graph_node WHERE session_id = ?")
        .bind(session_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn insert_nodes_edges(
    pool: &SqlitePool,
    nodes: &[GraphNode],
    edges: &[GraphEdge],
) -> Result<()> {
    let mut tx = pool.begin().await?;
    insert_nodes_edges_in_tx(&mut tx, nodes, edges).await?;
    tx.commit().await?;
    Ok(())
}

/// Slice-9 — tx-aware INSERT companion to `delete_session_in_tx`. Same
/// callers, single commit boundary.
pub async fn insert_nodes_edges_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    nodes: &[GraphNode],
    edges: &[GraphEdge],
) -> Result<()> {
    for n in nodes {
        sqlx::query(
            "INSERT INTO graph_node(node_id, schema_version, session_id, node_kind, \
             started_at, ended_at, merge_keys, source_event_ids, source_uris, payload) \
             VALUES (?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(&n.node_id)
        .bind(&n.schema_version)
        .bind(&n.session_id)
        .bind(&n.node_kind)
        .bind(n.started_at.to_rfc3339())
        .bind(n.ended_at.map(|t| t.to_rfc3339()))
        .bind(n.merge_keys.to_string())
        .bind(serde_json::to_string(&n.source_event_ids).unwrap())
        .bind(serde_json::to_string(&n.source_uris).unwrap())
        .bind(n.payload.to_string())
        .execute(&mut **tx)
        .await?;
    }
    for e in edges {
        sqlx::query(
            "INSERT INTO graph_edge(edge_id, schema_version, session_id, \
             from_node_id, to_node_id, edge_kind, origin, attributes, \
             inference_rule_id, confidence) \
             VALUES (?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(&e.edge_id)
        .bind(&e.schema_version)
        .bind(&e.session_id)
        .bind(&e.from_node_id)
        .bind(&e.to_node_id)
        .bind(&e.edge_kind)
        .bind(&e.origin)
        .bind(e.attributes.to_string())
        .bind(&e.inference_rule_id)
        .bind(e.confidence.map(|c| c as f64))
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

pub async fn load_session(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<(Vec<GraphNode>, Vec<GraphEdge>)> {
    let nrows = sqlx::query(
        "SELECT * FROM graph_node WHERE session_id = ? ORDER BY started_at ASC, node_id ASC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;

    let erows = sqlx::query("SELECT * FROM graph_edge WHERE session_id = ? ORDER BY edge_id ASC")
        .bind(session_id)
        .fetch_all(pool)
        .await?;

    let nodes = nrows
        .into_iter()
        .map(|r| GraphNode {
            node_id: r.get("node_id"),
            schema_version: r.get("schema_version"),
            session_id: r.get("session_id"),
            node_kind: r.get("node_kind"),
            started_at: chrono::DateTime::parse_from_rfc3339(&r.get::<String, _>("started_at"))
                .unwrap()
                .with_timezone(&chrono::Utc),
            ended_at: r.try_get::<String, _>("ended_at").ok().and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|t| t.with_timezone(&chrono::Utc))
            }),
            merge_keys: serde_json::from_str(&r.get::<String, _>("merge_keys"))
                .unwrap_or(serde_json::Value::Null),
            source_event_ids: serde_json::from_str(&r.get::<String, _>("source_event_ids"))
                .unwrap_or_default(),
            source_uris: serde_json::from_str(&r.get::<String, _>("source_uris"))
                .unwrap_or_default(),
            payload: serde_json::from_str(&r.get::<String, _>("payload"))
                .unwrap_or(serde_json::Value::Null),
        })
        .collect();

    let edges = erows
        .into_iter()
        .map(|r| GraphEdge {
            edge_id: r.get("edge_id"),
            schema_version: r.get("schema_version"),
            session_id: r.get("session_id"),
            from_node_id: r.get("from_node_id"),
            to_node_id: r.get("to_node_id"),
            edge_kind: r.get("edge_kind"),
            origin: r.get("origin"),
            attributes: serde_json::from_str(&r.get::<String, _>("attributes"))
                .unwrap_or(serde_json::Value::Null),
            inference_rule_id: r.try_get("inference_rule_id").ok().flatten(),
            confidence: r
                .try_get::<f64, _>("confidence")
                .ok()
                .map(|v| v as f32),
        })
        .collect();

    Ok((nodes, edges))
}
