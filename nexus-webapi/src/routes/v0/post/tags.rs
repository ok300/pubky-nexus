use crate::routes::v0::endpoints::{POST_TAGGERS_ROUTE, POST_TAGS_ROUTE};
use crate::routes::v0::TaggersInfoResponse;
use crate::{Error, Result};
use axum::extract::{Path, Query};
use axum::Json;
use nexus_common::models::tag::post::TagPost;
use nexus_common::models::tag::traits::{TagCollection, TaggersCollection};
use nexus_common::models::tag::TagDetails;
use nexus_common::types::Pagination;
use serde::Deserialize;
use tracing::debug;
use utoipa::{IntoParams, OpenApi, ToSchema};

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub struct PostTagsQuery {
    /// Viewer Pubky ID
    pub viewer_id: Option<String>,

    /// Skip N tags
    #[param(default = 0)]
    pub skip_tags: Option<usize>,

    /// Upper limit on the number of tags for the posts
    #[param(default = 5)]
    pub limit_tags: Option<usize>,

    /// Upper limit on the number of taggers per tag
    #[param(default = 5)]
    pub limit_taggers: Option<usize>,
}

#[utoipa::path(
    get,
    path = POST_TAGS_ROUTE,
    description = "Post tags",
    tag = "Post",
    params(
        ("author_id" = String, Path, description = "Author Pubky ID"),
        ("post_id" = String, Path, description = "Post ID"),
        PostTagsQuery
    ),
    responses(
        (status = 404, description = "Post not found"),
        (status = 200, description = "Post tags", body = Vec<TagDetails>),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn post_tags_handler(
    Path((author_id, post_id)): Path<(String, String)>,
    Query(query): Query<PostTagsQuery>,
) -> Result<Json<Vec<TagDetails>>> {
    debug!(
        "GET {POST_TAGS_ROUTE} author_id:{}, post_id: {}, skip_tags:{:?}, limit_tags:{:?}, limit_taggers:{:?}",
        author_id, post_id, query.limit_tags, query.skip_tags, query.limit_taggers
    );
    match TagPost::get_by_id(
        &author_id,
        Some(&post_id),
        query.skip_tags,
        query.limit_tags,
        query.limit_taggers,
        query.viewer_id.as_deref(),
        None, // Avoid by default WoT tags in a Post
    )
    .await
    {
        Ok(Some(tags)) => Ok(Json(tags)),
        Ok(None) => Err(Error::PostNotFound { author_id, post_id }),
        Err(source) => Err(Error::InternalServerError { source }),
    }
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub struct PostTaggersQuery {
    /// Viewer Pubky ID
    pub viewer_id: Option<String>,

    /// Number of taggers to skip for pagination
    #[param(default = 0)]
    pub skip: Option<usize>,

    /// Number of taggers to return for pagination
    #[param(default = 40)]
    pub limit: Option<usize>,
}

#[utoipa::path(
    get,
    path = POST_TAGGERS_ROUTE,
    description = "Post specific label Taggers",
    tag = "Post",
    params(
        ("author_id" = String, Path, description = "Author Pubky ID"),
        ("label" = String, Path, description = "Tag name"),
        ("post_id" = String, Path, description = "Post ID"),
        PostTaggersQuery
    ),
    responses(
        (status = 200, description = "Post tags", body = TaggersInfoResponse),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn post_taggers_handler(
    Path((author_id, post_id, label)): Path<(String, String, String)>,
    Query(query): Query<PostTaggersQuery>,
) -> Result<Json<TaggersInfoResponse>> {
    debug!(
        "GET {POST_TAGGERS_ROUTE} author_id:{}, post_id: {}, label: {}, viewer_id:{:?}, skip:{:?}, limit:{:?}",
        author_id, post_id, label, query.viewer_id, query.skip, query.limit
    );

    let pagination = Pagination {
        skip: query.skip,
        limit: query.limit,
        start: None,
        end: None,
    };

    match TagPost::get_tagger_by_id(
        &author_id,
        Some(&post_id),
        &label,
        pagination,
        query.viewer_id.as_deref(),
        None,
    )
    .await
    {
        Ok(tags) => Ok(Json(TaggersInfoResponse::from(tags))),
        Err(source) => Err(Error::InternalServerError { source }),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(post_tags_handler, post_taggers_handler),
    components(schemas(TagDetails, TaggersInfoResponse, PostTagsQuery, PostTaggersQuery))
)]
pub struct PostTagsApiDoc;
