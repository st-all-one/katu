//! Mapa estrutural de conhecimento (`memo knowledge`, E20-T06).
//!
//! Espelha `kd map` sem `--semantic`/`--communities`/`--write` (adiados; documentados no épico).

use katu_core::memory::{Cluster, MemoryError, NoteRef, QueryOutcome, QueryReq, QueryResult};
use knudge_core::graph::Graph;
use knudge_core::handoff::manifest::belongs_to;
use knudge_core::lifecycle::{Cluster as KdCluster, structural_clusters_filtered};

use super::super::Inner;
use super::convert::filter;

/// Modo `memo knowledge` (mapa estrutural).
pub(super) fn map(inner: &mut Inner, req: &QueryReq) -> Result<QueryResult, MemoryError> {
    let _span = katu_core::trace_fn!("memory::query::map::map");

    inner.ensure_index()?;
    inner.ensure_graph()?;
    let (Some(index), Some(graph)) = (inner.index.as_ref(), inner.graph.as_ref()) else {
        return Err(MemoryError::internal(
            "índice/grafo ausentes após construção",
        ));
    };
    let mut clusters = structural_clusters_filtered(index, graph, &filter(req)?);
    if let Some(axis) = req.axis.as_deref() {
        clusters.retain(|cluster| cluster.axis.axis() == axis);
    }
    if let Some(scope) = req.filter.scope.as_deref() {
        clusters = restrict_to_scope(clusters, graph, scope);
    }
    let clusters = clusters
        .iter()
        .map(|cluster| katu_cluster(cluster, req.members))
        .collect();
    Ok(QueryResult {
        outcome: QueryOutcome::Clusters(clusters),
        warnings: Vec::new(),
    })
}

/// Cluster do knudge → cluster da porta.
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "`members` é a flag `--members`"
)]
fn katu_cluster(cluster: &KdCluster, members: bool) -> Cluster {
    let _span = katu_core::trace_fn!("memory::query::map::katu_cluster");

    Cluster {
        axis: cluster.axis.axis().to_string(),
        key: cluster.axis.key().to_string(),
        members: if members {
            cluster
                .members
                .iter()
                .map(|member| NoteRef::new(member.clone()))
                .collect()
        } else {
            Vec::new()
        },
    }
}

/// Restringe os membros dos clusters aos que pertencem a `scope`.
fn restrict_to_scope(clusters: Vec<KdCluster>, graph: &Graph, scope: &str) -> Vec<KdCluster> {
    let _span = katu_core::trace_fn!("memory::query::map::restrict_to_scope");

    clusters
        .into_iter()
        .filter_map(|cluster| {
            let members: Vec<String> = cluster
                .members
                .into_iter()
                .filter(|member| belongs_to(graph, member, scope))
                .collect();
            (!members.is_empty()).then_some(KdCluster {
                axis: cluster.axis,
                members,
            })
        })
        .collect()
}
