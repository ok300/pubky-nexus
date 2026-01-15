use crate::routes::v0::endpoints::SEARCH_POSTS_BY_TAG_ROUTE;
use crate::{Error, Result};
use axum::extract::{Path, Query};
use axum::Json;
use nexus_common::models::post::search::PostsByTagSearch;
use nexus_common::types::StreamSorting;
use serde::Deserialize;
use tracing::debug;
use utoipa::{IntoParams, OpenApi, ToSchema};

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub struct SearchPostsQuery {
    /// StreamSorting method
    pub sorting: Option<StreamSorting>,

    /// The start of the stream timeframe. Posts with a timestamp greater than this value will be excluded from the results
    pub start: Option<f64>,

    /// The end of the stream timeframe. Posts with a timestamp less than this value will be excluded from the results
    pub end: Option<f64>,

    /// Skip N results
    pub skip: Option<usize>,

    /// Limit the number of results
    pub limit: Option<usize>,
}

#[utoipa::path(
    get,
    path = SEARCH_POSTS_BY_TAG_ROUTE,
    description = "Search Posts by Tag",
    tag = "Search",
    params(
        ("tag" = String, Path, description = "Tag name"),
        SearchPostsQuery
    ),
    responses(
        (status = 200, description = "Search results", body = Vec<PostsByTagSearch>),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn search_posts_by_tag_handler(
    Path(tag): Path<String>,
    Query(query): Query<SearchPostsQuery>,
) -> Result<Json<Vec<PostsByTagSearch>>> {
    debug!(
        "GET {SEARCH_POSTS_BY_TAG_ROUTE} tag:{}, sort_by: {:?}, start: {:?}, end: {:?}, skip: {:?}, limit: {:?}",
        tag, query.sorting, query.start, query.end, query.skip, query.limit
    );

    let skip = query.skip.unwrap_or(0);
    let limit = query.limit.unwrap_or(20);

    let pagination = nexus_common::types::Pagination {
        skip: Some(skip),
        limit: Some(limit),
        start: query.start,
        end: query.end,
    };

    match PostsByTagSearch::get_by_label(&tag, query.sorting, pagination).await {
        Ok(Some(posts_list)) => Ok(Json(posts_list)),
        Ok(None) => Ok(Json(vec![])),
        Err(source) => Err(Error::InternalServerError { source }),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(search_posts_by_tag_handler),
    components(schemas(PostsByTagSearch, SearchPostsQuery))
)]
pub struct SearchPostsByTagApiDocs;
