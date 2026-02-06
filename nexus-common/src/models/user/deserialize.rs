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
            UserLinks::String(s) if s.is_empty() => Ok(None),
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

#[cfg(test)]
mod tests {
    use super::*;
    use pubky_app_specs::PubkyAppUserLink;
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    struct TestStruct {
        #[serde(deserialize_with = "deserialize_user_links")]
        links: Option<Vec<PubkyAppUserLink>>,
    }

    #[test]
    fn test_deserialize_from_string() {
        let json = r#"{"links":"[{\"url\":\"https://example.com\",\"title\":\"website\"}]"}"#;
        let result: TestStruct = serde_json::from_str(json).unwrap();
        let links = result.links.expect("links should be Some");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].url, "https://example.com");
        assert_eq!(links[0].title, "website");
    }

    #[test]
    fn test_deserialize_from_array() {
        let json = r#"{"links":[{"url":"https://example.com","title":"website"}]}"#;
        let result: TestStruct = serde_json::from_str(json).unwrap();
        let links = result.links.expect("links should be Some");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].url, "https://example.com");
        assert_eq!(links[0].title, "website");
    }

    #[test]
    fn test_deserialize_from_null() {
        let json = r#"{"links":null}"#;
        let result: TestStruct = serde_json::from_str(json).unwrap();
        assert!(result.links.is_none());
    }

    #[test]
    fn test_deserialize_from_empty_string() {
        let json = r#"{"links":""}"#;
        let result: TestStruct = serde_json::from_str(json).unwrap();
        assert!(result.links.is_none());
    }

    #[test]
    fn test_deserialize_from_invalid_string() {
        let json = r#"{"links":"invalid"}"#;
        let result: Result<TestStruct, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_from_object() {
        let json = r#"{"links":{}}"#;
        let result: Result<TestStruct, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }
}
