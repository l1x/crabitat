use axum::Router;
use axum::routing::{delete, get, post};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::AppState;
use crate::handlers;

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .nest("/v1/repos", repos_routes())
        .nest("/v1/workflows", workflows_routes())
        .nest("/v1/prompts", prompts_routes())
        .nest("/v1/missions", missions_routes())
        .nest("/v1/tasks", tasks_routes())
        .nest("/v1/github", github_routes())
        .nest("/v1/settings", settings_routes())
        .nest("/v1/system", system_routes())
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

fn repos_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            post(handlers::repos::create_repo).get(handlers::repos::list_repos),
        )
        .route(
            "/{repo_id}",
            get(handlers::repos::get_repo).delete(handlers::repos::delete_repo),
        )
        .route("/{repo_id}/issues", get(handlers::issues::list_repo_issues))
        .route(
            "/{repo_id}/issues/refresh",
            post(handlers::issues::refresh_repo_issues),
        )
}

fn workflows_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(handlers::workflows::list_all_workflows))
        .route("/{name}", get(handlers::workflows::get_workflow))
        .route("/{name}/flavors", post(handlers::workflows::create_flavor))
        .route(
            "/{name}/flavors/{flavor_id}",
            delete(handlers::workflows::delete_flavor).patch(handlers::workflows::update_flavor),
        )
}

fn prompts_routes() -> Router<AppState> {
    Router::new()
        .route("/files", get(handlers::workflows::list_prompt_files))
        .route("/content", post(handlers::workflows::get_prompts_content))
}

fn missions_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            post(handlers::missions::create_mission).get(handlers::missions::list_missions),
        )
        .route("/{mission_id}", get(handlers::missions::get_mission))
}

fn tasks_routes() -> Router<AppState> {
    Router::new()
        .route("/next", get(handlers::tasks::get_next_task))
        .route(
            "/{task_id}/status",
            post(handlers::tasks::update_task_status),
        )
        .route("/{task_id}/retry", post(handlers::tasks::retry_task))
        .route("/{task_id}/runs", post(handlers::tasks::create_run))
}

fn github_routes() -> Router<AppState> {
    Router::new().route("/repos", get(handlers::github::search_repos))
}

fn settings_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(handlers::settings::list_settings))
        .route(
            "/{key}",
            get(handlers::settings::get_setting).post(handlers::settings::update_setting),
        )
}

fn system_routes() -> Router<AppState> {
    Router::new()
        .route("/status", get(handlers::system::get_status))
        .route("/dirs", get(handlers::system::list_dirs))
        .route(
            "/env-path/{env}/{type}/{name}",
            get(handlers::system::get_environment_path),
        )
        .route("/env-paths", get(handlers::system::list_environment_paths))
}
