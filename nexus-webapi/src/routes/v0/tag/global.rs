use crate::routes::v0::endpoints::{TAGS_HOT_ROUTE, TAG_TAGGERS_ROUTE};
use crate::{Error, Result};
use axum::extract::{Path, Query};
use axum::Json;
use nexus_common::models::tag::global::Taggers;
use nexus_common::models::tag::stream::{HotTag, HotTags};
use nexus_common::models::tag::TaggedType;
use nexus_common::models::tag::Taggers as TaggersType;
use nexus_common::types::routes::HotTagsInputDTO;
use nexus_common::types::{StreamReach, Timeframe};
use serde::Deserialize;
use tracing::{debug, error};
use utoipa::{IntoParams, OpenApi, ToSchema};

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub struct HotTagsQuery {
    /// User Pubky ID
    user_id: Option<String>,

    /// Reach type: `follower` | `following` | `friends` | `wot`. To apply that, user_id is required
    reach: Option<StreamReach>,

    /// Retrieve N user_id for each tag
    #[param(default = 20, maximum = 20)]
    taggers_limit: Option<usize>,

    /// Retrieve hot tags for this specific timeframe
    #[param(default = "all_time")]
    timeframe: Option<Timeframe>,

    /// Skip N tags
    #[param(default = 0)]
    skip: Option<usize>,

    /// Retrieve N tag
    #[param(default = 40, maximum = 40)]
    limit: Option<usize>,

    /// The start of the stream timeframe
    start: Option<f64>,

    /// The end of the stream timeframe
    end: Option<f64>,
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub struct TagTaggersQuery {
    /// Skip N taggers
    #[param(default = 0)]
    skip: Option<usize>,

    /// Retrieve N taggers
    #[param(default = 20, maximum = 20)]
    limit: Option<usize>,

    /// The start of the stream timeframe
    start: Option<f64>,

    /// The end of the stream timeframe
    end: Option<f64>,

    /// User ID to base reach on
    user_id: Option<String>,

    /// Reach type: `follower` | `following` | `friends` | `wot`. To apply that, user_id is required
    reach: Option<StreamReach>,

    /// Retrieve taggers for this specific timeframe (not applied for reach)
    #[param(default = "all_time")]
    timeframe: Option<Timeframe>,
}

#[utoipa::path(
    get,
    path = TAG_TAGGERS_ROUTE,
    description = "Global tag Taggers",
    tag = "Tags",
    params(
        ("label" = String, Path, description = "Tag name"),
        TagTaggersQuery
    ),
    responses(
        (status = 200, description = "Taggers", body = TaggersType),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn tag_taggers_handler(
    Path(label): Path<String>,
    Query(query): Query<TagTaggersQuery>,
) -> Result<Json<TaggersType>> {
    debug!(
        "GET {TAG_TAGGERS_ROUTE} label:{}, query: {:?}",
        label, query
    );

    // Check if user_id and reach are provided together
    if query.user_id.is_some() ^ query.reach.is_some() {
        return Err(Error::InvalidInput {
            message: String::from("user_id and reach should be both provided together"),
        });
    }

    let skip = query.skip.unwrap_or(0);
    let limit = query.limit.unwrap_or(20).min(20);
    let timeframe = query.timeframe.unwrap_or(Timeframe::AllTime);

    match Taggers::get_global_taggers(
        label.clone(),
        query.user_id,
        query.reach,
        skip,
        limit,
        timeframe,
    )
    .await
    {
        Ok(Some(post)) => Ok(Json(post)),
        Ok(None) => Ok(Json(vec![])),
        Err(source) => Err(Error::InternalServerError { source }),
    }
}

#[utoipa::path(
    get,
    path = TAGS_HOT_ROUTE,
    description = "Global Tags by reach",
    tag = "Tags",
    params(
        HotTagsQuery
    ),
    responses(
        (status = 200, description = "Retrieve tags by reach cluster", body = Vec<HotTag>),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn hot_tags_handler(Query(query): Query<HotTagsQuery>) -> Result<Json<HotTags>> {
    debug!("GET {TAGS_HOT_ROUTE}, query: {:?}", query);

    // Check if user_id and reach are provided together
    if query.user_id.is_some() ^ query.reach.is_some() {
        return Err(Error::InvalidInput {
            message: String::from("user_id and reach should be both provided together"),
        });
    }

    let skip = query.skip.unwrap_or(0);
    let limit = query.limit.unwrap_or(40).min(40);
    let taggers_limit = query.taggers_limit.unwrap_or(20).min(20);
    let timeframe = query.timeframe.unwrap_or(Timeframe::AllTime);

    let input = HotTagsInputDTO {
        timeframe,
        skip,
        limit,
        taggers_limit,
        tagged_type: Some(TaggedType::Post),
    };

    match HotTags::get_hot_tags(query.user_id, query.reach, &input).await {
        Ok(Some(hot_tags)) => Ok(Json(hot_tags)),
        Ok(None) => Ok(Json(HotTags::default())),
        Err(source) => {
            error!("Internal Server ERROR: {:?}", source);
            Err(Error::InternalServerError { source })
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(hot_tags_handler, tag_taggers_handler),
    components(schemas(HotTags, HotTag, Taggers, StreamReach, Timeframe, HotTagsQuery, TagTaggersQuery))
)]
pub struct TagGlobalApiDoc;
