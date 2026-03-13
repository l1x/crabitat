use crabitat_control_plane::github;

#[tokio::test]
async fn test_check_status_exists() {
    let status = github::check_status().await;

    println!("GH Installed: {}", status.gh_installed);
    if status.gh_installed {
        assert!(status.gh_version.is_some());
    }
}
