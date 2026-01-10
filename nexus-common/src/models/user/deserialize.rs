use pubky_app_specs::PubkyAppUserLink;
use serde::{Deserialize, Deserializer};

#[derive(Deserialize)]
#[serde(untagged)]
enum UserLinks {
    String(String),
    Array(Vec<PubkyAppUserLink>),
}

impl TryFrom<UserLinks> for Option<Vec<PubkyAppUserLink>> {
    type Error = serde_json::Error;
    fn try_from(value: UserLinks) -> Result<Self, Self::Error> {
        match value {
            UserLinks::String(s) => serde_json::from_str(&s),
            UserLinks::Array(arr) => Ok(Some(arr)),
        }
    }
}

pub fn deserialize_user_links<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<PubkyAppUserLink>>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<UserLinks>::deserialize(deserializer)?;
    Ok(value
        .map(TryInto::try_into)
        .transpose()
        .map_err(serde::de::Error::custom)?
        .flatten())
}
