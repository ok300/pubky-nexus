use nexus_common::models::tag::Taggers;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// DTO (Data Transfer Object) is used to transfer structured data between API layers,
// ensuring clear separation between internal models and external representations
#[derive(Serialize, ToSchema, Deserialize)]
pub struct TaggersInfoResponse {
    pub users: Taggers,
    pub relationship: bool,
}

impl From<(Taggers, bool)> for TaggersInfoResponse {
    fn from(tuple: (Taggers, bool)) -> Self {
        Self {
            users: tuple.0,
            relationship: tuple.1,
        }
    }
}
