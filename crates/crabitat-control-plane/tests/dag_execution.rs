use crabitat_control_plane::handlers::missions::{compute_step_orders, topological_sort_steps};
use crabitat_control_plane::models::workflows::WorkflowStepFile;

fn step(id: &str, depends_on: Option<Vec<&str>>) -> WorkflowStepFile {
    WorkflowStepFile {
        id: id.to_string(),
        prompt_file: format!("{}.md", id),
        depends_on: depends_on.map(|deps| deps.into_iter().map(String::from).collect()),
        on_fail: None,
        max_retries: None,
    }
}

#[test]
fn test_linear_workflow_no_depends_on() {
    let steps = vec![step("plan", None), step("code", None), step("test", None)];

    let orders = compute_step_orders(&steps).unwrap();
    // Should fall back to sequential enumerate
    assert_eq!(orders, vec![(0, 0), (1, 1), (2, 2)]);
}

#[test]
fn test_diamond_dag() {
    // plan -> {kani, tla, proptest} -> report
    let steps = vec![
        step("plan", None),
        step("kani", Some(vec!["plan"])),
        step("tla", Some(vec!["plan"])),
        step("proptest", Some(vec!["plan"])),
        step("report", Some(vec!["kani", "tla", "proptest"])),
    ];

    let orders = topological_sort_steps(&steps).unwrap();

    // Build a map from step index to depth
    let depth_map: std::collections::HashMap<usize, usize> = orders.into_iter().collect();

    // plan at depth 0
    assert_eq!(depth_map[&0], 0);
    // kani, tla, proptest at depth 1
    assert_eq!(depth_map[&1], 1);
    assert_eq!(depth_map[&2], 1);
    assert_eq!(depth_map[&3], 1);
    // report at depth 2
    assert_eq!(depth_map[&4], 2);
}

#[test]
fn test_cycle_detection() {
    let steps = vec![step("a", Some(vec!["b"])), step("b", Some(vec!["a"]))];

    let result = topological_sort_steps(&steps);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("cycle detected"));
}

#[test]
fn test_unknown_dependency() {
    let steps = vec![step("plan", None), step("code", Some(vec!["nonexistent"]))];

    let result = topological_sort_steps(&steps);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("unknown step"));
}

#[test]
fn test_two_roots_fan_in() {
    // a and b are both roots, c depends on both
    let steps = vec![
        step("a", None),
        step("b", None),
        step("c", Some(vec!["a", "b"])),
    ];

    let orders = topological_sort_steps(&steps).unwrap();
    let depth_map: std::collections::HashMap<usize, usize> = orders.into_iter().collect();

    assert_eq!(depth_map[&0], 0); // a
    assert_eq!(depth_map[&1], 0); // b
    assert_eq!(depth_map[&2], 1); // c
}

#[test]
fn test_compute_step_orders_with_deps() {
    let steps = vec![
        step("plan", None),
        step("code", Some(vec!["plan"])),
        step("test", Some(vec!["code"])),
    ];

    let orders = compute_step_orders(&steps).unwrap();
    let depth_map: std::collections::HashMap<usize, usize> = orders.into_iter().collect();

    assert_eq!(depth_map[&0], 0);
    assert_eq!(depth_map[&1], 1);
    assert_eq!(depth_map[&2], 2);
}
