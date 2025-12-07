use crate::routes::v0::endpoints::{USER_TAGGERS_ROUTE, USER_TAGS_ROUTE};
use crate::routes::v0::TaggersInfoResponse;
use crate::{Error, Result};
use axum::extract::{Path, Query};
use axum::Json;
use nexus_common::models::tag::traits::{TagCollection, TaggersCollection};
use nexus_common::models::tag::user::TagUser;
use nexus_common::models::tag::TagDetails;
use nexus_common::types::Pagination;
use serde::Deserialize;
use tracing::debug;
use utoipa::{IntoParams, OpenApi, ToSchema};

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub struct UserTagsQuery {
    /// Skip N tags
    #[param(default = 0)]
    pub skip_tags: Option<usize>,

    /// Upper limit on the number of tags for the user
    #[param(default = 5)]
    pub limit_tags: Option<usize>,

    /// Upper limit on the number of taggers per tag
    #[param(default = 5)]
    pub limit_taggers: Option<usize>,

    /// Viewer Pubky ID
    pub viewer_id: Option<String>,

    /// User trusted network depth, user following users distance. Numbers bigger than 4 will be ignored
    #[param(maximum = 4)]
    pub depth: Option<u8>,
}

#[utoipa::path(
    get,
    path = USER_TAGS_ROUTE,
    description = "User Tags",
    tag = "User",
    params(
        ("user_id" = String, Path, description = "User Pubky ID"),
        UserTagsQuery
    ),
    responses(
        (status = 200, description = "User tags", body = TagDetails),
        (status = 404, description = "User not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn user_tags_handler(
    Path(user_id): Path<String>,
    Query(query): Query<UserTagsQuery>,
) -> Result<Json<Vec<TagDetails>>> {
    debug!(
        "GET {USER_TAGS_ROUTE} user_id:{}, skip_tags:{:?}, limit_tags:{:?}, limit_taggers:{:?}, viewer_id:{:?}, depth:{:?}",
        user_id, query.skip_tags, query.limit_tags, query.limit_taggers, query.viewer_id, query.depth
    );

    match TagUser::get_by_id(
        &user_id,
        None,
        query.skip_tags,
        query.limit_tags,
        query.limit_taggers,
        query.viewer_id.as_deref(),
        query.depth,
    )
    .await
    {
        Ok(Some(tags)) => Ok(Json(tags)),
        Ok(None) => Err(Error::UserNotFound { user_id }),
        Err(source) => Err(Error::InternalServerError { source }),
    }
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub struct UserTaggersQuery {
    /// Number of taggers to skip for pagination
    pub skip: Option<usize>,

    /// Number of taggers to return for pagination
    pub limit: Option<usize>,

    /// Viewer Pubky ID
    pub viewer_id: Option<String>,

    /// User trusted network depth, user following users distance. Numbers bigger than 4 will be ignored
    #[param(maximum = 4)]
    pub depth: Option<u8>,
}

#[utoipa::path(
    get,
    path = USER_TAGGERS_ROUTE,
    description = "User label taggers",
    tag = "User",
    params(
        ("user_id" = String, Path, description = "User Pubky ID"),
        ("label" = String, Path, description = "Tag name"),
        UserTaggersQuery
    ),
    responses(
        (status = 200, description = "User tags", body = TaggersInfoResponse),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn user_taggers_handler(
    Path((user_id, label)): Path<(String, String)>,
    Query(query): Query<UserTaggersQuery>,
) -> Result<Json<TaggersInfoResponse>> {
    debug!(
        "GET {USER_TAGGERS_ROUTE} user_id:{}, label: {}, skip:{:?}, limit:{:?}, viewer_id:{:?}, depth:{:?}",
        user_id, label, query.skip, query.limit, query.viewer_id, query.depth
    );

    let pagination = Pagination {
        skip: query.skip,
        limit: query.limit,
        start: None,
        end: None,
    };

    match TagUser::get_tagger_by_id(
        &user_id,
        None,
        &label,
        pagination,
        query.viewer_id.as_deref(),
        query.depth,
    )
    .await
    {
        Ok(tags) => Ok(Json(TaggersInfoResponse::from(tags))),
        Err(source) => Err(Error::InternalServerError { source }),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(user_tags_handler, user_taggers_handler),
    components(schemas(TagDetails, TaggersInfoResponse, UserTagsQuery, UserTaggersQuery))
)]
pub struct UserTagsApiDoc;
