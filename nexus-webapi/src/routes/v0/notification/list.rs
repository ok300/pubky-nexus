use crate::routes::v0::endpoints::NOTIFICATION_ROUTE;
use crate::{Error, Result};
use axum::extract::{Path, Query};
use axum::Json;
use nexus_common::models::notification::{Notification, NotificationBody, PostChangedSource};
use serde::Deserialize;
use tracing::debug;
use utoipa::{IntoParams, OpenApi, ToSchema};

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub struct NotificationQuery {
    /// Skip N notifications
    skip: Option<usize>,

    /// Retrieve N notifications
    limit: Option<usize>,

    /// Start timestamp for notification retrieval
    start: Option<f64>,

    /// End timestamp for notification retrieval
    end: Option<f64>,
}

#[utoipa::path(
    get,
    path = NOTIFICATION_ROUTE,
    tag = "User",
    description = "List of user notifications",
    params(
        ("user_id" = String, Path, description = "User Pubky ID"),
        NotificationQuery
    ),
    responses(
        (status = 200, description = "List of notifications", body = Vec<Notification>),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn list_notifications_handler(
    Path(user_id): axum::extract::Path<String>,
    Query(query): Query<NotificationQuery>,
) -> Result<Json<Vec<Notification>>> {
    debug!("GET {NOTIFICATION_ROUTE} for user_id: {}", user_id);

    let pagination = nexus_common::types::Pagination {
        skip: query.skip,
        limit: query.limit,
        start: query.start,
        end: query.end,
    };

    match Notification::get_by_id(&user_id, pagination).await {
        Ok(notifications) => Ok(Json(notifications)),
        Err(source) => Err(Error::InternalServerError { source }),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(list_notifications_handler,),
    components(schemas(Notification, NotificationBody, PostChangedSource, NotificationQuery))
)]
pub struct NotificationsApiDocs;
