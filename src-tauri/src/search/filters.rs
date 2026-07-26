#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedQuery {
    pub text: Option<String>,
    pub camera: Option<String>,
    pub min_rating: Option<i64>,
    pub before: Option<String>,
    pub after: Option<String>,
    pub media_type: Option<String>,
    pub favorite_only: bool,
}

impl ParsedQuery {
    pub fn is_empty_browse(&self) -> bool {
        self.text.is_none()
            && self.camera.is_none()
            && self.min_rating.is_none()
            && self.before.is_none()
            && self.after.is_none()
            && self.media_type.is_none()
            && !self.favorite_only
    }
}

/// Parse filter tokens: `camera:`, `rating>`, `before:`, `after:`, `type:`, `fav:true`
/// Remaining tokens become free-text FTS query.
pub fn parse_query(raw: &str) -> ParsedQuery {
    let mut parsed = ParsedQuery::default();
    let mut text_parts = Vec::new();

    for token in raw.split_whitespace() {
        if let Some(val) = token.strip_prefix("camera:") {
            parsed.camera = Some(val.to_string());
        } else if let Some(val) = token.strip_prefix("rating>") {
            if let Ok(n) = val.parse::<i64>() {
                parsed.min_rating = Some(n);
            }
        } else if let Some(val) = token.strip_prefix("rating>=") {
            if let Ok(n) = val.parse::<i64>() {
                parsed.min_rating = Some(n);
            }
        } else if let Some(val) = token.strip_prefix("before:") {
            parsed.before = Some(val.to_string());
        } else if let Some(val) = token.strip_prefix("after:") {
            parsed.after = Some(val.to_string());
        } else if let Some(val) = token.strip_prefix("type:") {
            let v = val.to_ascii_lowercase();
            if v == "video" || v == "image" {
                parsed.media_type = Some(v);
            }
        } else if token.eq_ignore_ascii_case("fav:true")
            || token.eq_ignore_ascii_case("favorite:true")
        {
            parsed.favorite_only = true;
        } else {
            text_parts.push(token.to_string());
        }
    }

    if !text_parts.is_empty() {
        parsed.text = Some(text_parts.join(" "));
    }
    parsed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_filters_and_text() {
        let q = parse_query("beach camera:iphone rating>3 before:2024-01-01 type:image fav:true");
        assert_eq!(q.text.as_deref(), Some("beach"));
        assert_eq!(q.camera.as_deref(), Some("iphone"));
        assert_eq!(q.min_rating, Some(3));
        assert_eq!(q.before.as_deref(), Some("2024-01-01"));
        assert_eq!(q.media_type.as_deref(), Some("image"));
        assert!(q.favorite_only);
    }

    #[test]
    fn empty_is_browse() {
        assert!(parse_query("").is_empty_browse());
        assert!(parse_query("   ").is_empty_browse());
    }
}
