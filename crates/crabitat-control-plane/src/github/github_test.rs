#[cfg(test)]
mod tests {
    use crate::github;

    #[tokio::test]
    async fn test_check_status_exists() {
        // This test assumes 'gh' is installed on the machine running the tests.
        // It's more of an integration check.
        let status = github::check_status().await;

        // We can't guarantee auth status in CI/test env, but we can check if it attempted
        // to run and populated at least the installation status if gh is present.
        println!("GH Installed: {}", status.gh_installed);
        if status.gh_installed {
            assert!(status.gh_version.is_some());
        }
    }
}
