use crate::routes::v0::endpoints::{SEARCH_USERS_BY_ID_ROUTE, SEARCH_USERS_BY_NAME_ROUTE};
use crate::routes::v0::search::USER_ID_SEARCH_MIN_PREFIX_LEN;
use crate::{Error, Result};
use axum::extract::{Path, Query};
use axum::Json;
use nexus_common::models::user::UserSearch;
use serde::Deserialize;
use tracing::debug;
use utoipa::{IntoParams, OpenApi, ToSchema};

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub struct SearchQuery {
    /// Skip N results
    skip: Option<usize>,

    /// Limit the number of results
    limit: Option<usize>,
}

#[utoipa::path(
    get,
    path = SEARCH_USERS_BY_NAME_ROUTE,
    description = "Search user id by username prefix",
    tag = "Search",
    params(
        ("prefix" = String, Path, description = "Username prefix to search for"),
        SearchQuery
    ),
    responses(
        (status = 200, description = "Search results", body = UserSearch),
        (status = 400, description = "Invalid input"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn search_users_by_name_handler(
    Path(prefix): Path<String>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<UserSearch>> {
    let username = prefix;
    if username.trim().is_empty() {
        return Err(Error::invalid_input("Username cannot be empty"));
    }

    debug!("GET {SEARCH_USERS_BY_NAME_ROUTE} username:{}", username);

    let skip = query.skip.unwrap_or(0);
    let limit = query.limit.unwrap_or(200);

    match UserSearch::get_by_name(&username, Some(skip), Some(limit)).await {
        Ok(Some(user_search)) => Ok(Json(user_search)),
        Ok(None) => Ok(Json(UserSearch::default())),
        Err(source) => Err(Error::InternalServerError { source }),
    }
}

#[utoipa::path(
    get,
    path = SEARCH_USERS_BY_ID_ROUTE,
    description = "Search user IDs by ID prefix",
    tag = "Search",
    params(
        ("prefix" = String, Path, description = format!("User ID prefix to search for (at least {USER_ID_SEARCH_MIN_PREFIX_LEN} characters)")),
        SearchQuery
    ),
    responses(
        (status = 200, description = "Search results", body = UserSearch),
        (status = 400, description = "Invalid input"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn search_users_by_id_handler(
    Path(prefix): Path<String>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<UserSearch>> {
    let id_prefix = prefix;
    if id_prefix.trim().chars().count() < USER_ID_SEARCH_MIN_PREFIX_LEN {
        return Err(Error::invalid_input(&format!(
            "ID prefix must be at least {USER_ID_SEARCH_MIN_PREFIX_LEN} chars"
        )));
    }

    debug!("GET {SEARCH_USERS_BY_ID_ROUTE} ID:{}", id_prefix);

    let skip = query.skip.unwrap_or(0);
    let limit = query.limit.unwrap_or(200);

    match UserSearch::get_by_id(&id_prefix, Some(skip), Some(limit)).await {
        Ok(Some(user_search)) => Ok(Json(user_search)),
        Ok(None) => Ok(Json(UserSearch::default())),
        Err(source) => Err(Error::InternalServerError { source }),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(search_users_by_name_handler, search_users_by_id_handler),
    components(schemas(UserSearch, SearchQuery))
)]
pub struct SearchUsersApiDocs;
