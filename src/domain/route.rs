use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Unlisted,
    Private,
}
impl Visibility {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "public" => Ok(Self::Public),
            "unlisted" => Ok(Self::Unlisted),
            "private" => Ok(Self::Private),
            _ => Err("Choose a visibility setting".into()),
        }
    }
}

/// A route's access policy is independent of HTTP and persistence.
pub fn may_view(
    visibility: Visibility,
    owner_id: &str,
    actor_id: Option<&str>,
    is_admin: bool,
) -> bool {
    visibility != Visibility::Private || actor_id == Some(owner_id) || is_admin
}
pub fn may_edit(owner_id: &str, actor_id: &str, is_admin: bool) -> bool {
    owner_id == actor_id || is_admin
}
/// Other riders may add photos to routes they can open by link, unless the owner turned that off.
pub fn may_add_photo(
    visibility: Visibility,
    photos_open: bool,
    owner_id: &str,
    actor_id: &str,
) -> bool {
    owner_id == actor_id || (visibility != Visibility::Private && photos_open)
}
pub fn may_like_photo(can_view: bool, uploader_id: &str, actor_id: &str) -> bool {
    can_view && uploader_id != actor_id
}

#[derive(Clone, Debug)]
pub struct RouteDraft {
    pub title: String,
    pub description: String,
    pub cycling_type: String,
    pub region: String,
    pub country: String,
    pub difficulty: String,
    pub tags: String,
    pub visibility: Visibility,
    /// Unchecked boxes are not submitted, so a missing field means contributions are off.
    pub photos_open: bool,
}
impl RouteDraft {
    pub fn from_fields(fields: &HashMap<String, String>) -> Result<Self, String> {
        let get = |key: &str| {
            fields
                .get(key)
                .map(String::as_str)
                .unwrap_or("")
                .trim()
                .to_owned()
        };
        let draft = Self {
            title: get("title"),
            description: get("description"),
            cycling_type: get("cycling_type"),
            region: get("region"),
            country: get("country"),
            difficulty: get("difficulty"),
            tags: get("tags"),
            visibility: Visibility::parse(&get("visibility"))?,
            photos_open: fields.contains_key("photos_open"),
        };
        if draft.title.len() < 4 || draft.title.len() > 100 {
            return Err("Title must be 4–100 characters".into());
        }
        if draft.description.len() > 2_000 {
            return Err("Description must be 2,000 characters or less".into());
        }
        if draft.region.is_empty() || draft.region.len() > 80 {
            return Err("Enter a region (up to 80 characters)".into());
        }
        if draft.country.is_empty() || draft.country.len() > 80 {
            return Err("Enter a country (up to 80 characters)".into());
        }
        if draft.tags.len() > 160 {
            return Err("Tags must be 160 characters or less".into());
        }
        if ![
            "road",
            "gravel",
            "mountain",
            "touring",
            "bikepacking",
            "urban",
            "mixed",
        ]
        .contains(&draft.cycling_type.as_str())
        {
            return Err("Choose a cycling type".into());
        }
        if !["easy", "moderate", "hard"].contains(&draft.difficulty.as_str()) {
            return Err("Choose a difficulty".into());
        }
        Ok(draft)
    }
    pub fn visibility_str(&self) -> &'static str {
        match self.visibility {
            Visibility::Public => "public",
            Visibility::Unlisted => "unlisted",
            Visibility::Private => "private",
        }
    }
}

pub fn slugify(title: &str, id: &str) -> String {
    let mut slug = String::new();
    for c in title.to_ascii_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    format!(
        "{}-{}",
        if slug.is_empty() { "route" } else { slug },
        id.replace('-', "")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_routes_need_owner_or_admin() {
        assert!(!may_view(Visibility::Private, "owner", None, false));
        assert!(!may_view(
            Visibility::Private,
            "owner",
            Some("other"),
            false
        ));
        assert!(may_view(Visibility::Private, "owner", Some("owner"), false));
        assert!(may_view(Visibility::Private, "owner", None, true));
    }
    #[test]
    fn photo_rules_follow_visibility_switch_and_authorship() {
        assert!(may_add_photo(Visibility::Private, false, "owner", "owner"));
        assert!(may_add_photo(Visibility::Public, true, "owner", "rider"));
        assert!(may_add_photo(Visibility::Unlisted, true, "owner", "rider"));
        assert!(!may_add_photo(Visibility::Public, false, "owner", "rider"));
        assert!(!may_add_photo(
            Visibility::Unlisted,
            false,
            "owner",
            "rider"
        ));
        assert!(!may_add_photo(Visibility::Private, true, "owner", "rider"));
        assert!(!may_like_photo(true, "a", "a"));
        assert!(may_like_photo(true, "a", "b"));
        assert!(!may_like_photo(false, "a", "b"));
    }
    #[test]
    fn public_and_unlisted_are_link_viewable() {
        assert!(may_view(Visibility::Public, "owner", None, false));
        assert!(may_view(Visibility::Unlisted, "owner", None, false));
    }
}
