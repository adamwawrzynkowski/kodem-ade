use super::models::Commit;

/// Layout data for one history row.
#[derive(Debug, Clone, Default)]
pub struct GraphRow {
    /// Lane index of the commit node.
    pub lane: usize,
    /// Color index of the commit node.
    pub color: usize,
    /// Edges drawn between this row and the next row.
    pub edges: Vec<GraphEdge>,
    /// Number of lanes that are active at this row.
    pub lane_count: usize,
}

/// A line from a lane in one history row to a lane in the next row.
#[derive(Debug, Clone, Copy)]
pub struct GraphEdge {
    pub from_lane: usize,
    pub to_lane: usize,
    pub color: usize,
}

#[derive(Debug, Clone)]
struct Lane {
    expected: String,
    color: usize,
}

/// Compute lane positions and edges for a topologically ordered commit list.
pub fn layout(commits: &[Commit]) -> Vec<GraphRow> {
    let mut rows = Vec::with_capacity(commits.len());
    let mut lanes: Vec<Lane> = Vec::new();
    let mut next_color = 0usize;

    for commit in commits {
        let mut edges = Vec::new();

        // Find the lane that expects this commit.
        let node_lane = lanes
            .iter()
            .position(|l| l.expected == commit.hash)
            .unwrap_or_else(|| {
                lanes.push(Lane {
                    expected: commit.hash.clone(),
                    color: {
                        let c = next_color;
                        next_color += 1;
                        c
                    },
                });
                lanes.len() - 1
            });
        let node_color = lanes[node_lane].color;
        let lane_count_before = lanes.len();

        // Lanes that also expect this commit merge into the node.
        let merged: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter(|(i, l)| *i != node_lane && l.expected == commit.hash)
            .map(|(i, _)| i)
            .collect();

        // Build the next lane arrangement.
        let mut next_lanes: Vec<Lane> = Vec::with_capacity(lanes.len());
        let mut from_positions: Vec<(usize, usize, usize)> = Vec::new(); // (old index, new index, color)
        for (i, lane) in lanes.iter().enumerate() {
            if i == node_lane {
                if let Some(first_parent) = commit.parents.first() {
                    next_lanes.push(Lane {
                        expected: first_parent.clone(),
                        color: lane.color,
                    });
                    from_positions.push((i, next_lanes.len() - 1, lane.color));
                }
                // A commit with no parents ends its lane.
            } else if merged.contains(&i) {
                // This lane merges into the node; edge goes to the node lane.
                edges.push(GraphEdge {
                    from_lane: i,
                    to_lane: node_lane.min(next_lanes.len()),
                    color: lane.color,
                });
            } else {
                next_lanes.push(Lane {
                    expected: lane.expected.clone(),
                    color: lane.color,
                });
                from_positions.push((i, next_lanes.len() - 1, lane.color));
            }
        }

        // Continuation edges for lanes that survive.
        for (old, new, color) in &from_positions {
            edges.push(GraphEdge {
                from_lane: *old,
                to_lane: *new,
                color: *color,
            });
        }

        // Extra parents of a merge commit fork out of the node.
        for parent in commit.parents.iter().skip(1) {
            if let Some(existing) = next_lanes.iter().position(|l| &l.expected == parent) {
                edges.push(GraphEdge {
                    from_lane: node_lane,
                    to_lane: existing,
                    color: next_lanes[existing].color,
                });
            } else {
                let color = next_color;
                next_color += 1;
                next_lanes.push(Lane {
                    expected: parent.clone(),
                    color,
                });
                edges.push(GraphEdge {
                    from_lane: node_lane,
                    to_lane: next_lanes.len() - 1,
                    color,
                });
            }
        }

        let lane_count = lane_count_before.max(next_lanes.len());
        rows.push(GraphRow {
            lane: node_lane,
            color: node_color,
            edges,
            lane_count,
        });
        lanes = next_lanes;
    }
    rows
}
