use crate::{CategoryId, LibraryItemId, UnixTimestampMilliseconds};

pub const MAX_CATEGORY_NAME_BYTES: usize = 128;
pub const MAX_CATEGORY_DESCRIPTION_BYTES: usize = 1_024;
/// Guards the taxonomy against becoming a second, unusable library. A user
/// who wants more lenses than this wants search, not categories.
pub const MAX_CATEGORIES: usize = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, uniffi::Record)]
pub struct CategoryRevision {
    pub value: u64,
}

impl CategoryRevision {
    pub const INITIAL: Self = Self { value: 1 };

    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self { value }
    }
}

/// Who put a category there. The distinction is not cosmetic: a rebuild of
/// the machine-generated taxonomy must not silently discard groupings the
/// user or the agent curated deliberately.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum CategoryOrigin {
    /// Produced by a bulk categorization pass over the library.
    Generated,
    /// Created by the agent through `write_category`.
    Agent,
    /// Created by the person using the app.
    User,
    Unsupported {
        wire_code: u32,
    },
}

/// What a category holds. Podcasts and episodes share `LibraryItemId` so the
/// membership primitive needs no per-kind verb, but the resolved kind is
/// recorded once the kernel has looked the id up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum CategoryItemKind {
    Podcast,
    Episode,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct CategoryMember {
    pub item_id: LibraryItemId,
    pub kind: CategoryItemKind,
    /// When this item entered the category. Ordering membership by recency
    /// is what makes a category page feel alive rather than alphabetical.
    pub added_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct CategoryRecord {
    pub category_id: CategoryId,
    pub revision: CategoryRevision,
    pub name: String,
    /// Lowercased, hyphenated form of `name`. Derived by the kernel rather
    /// than accepted from a caller so it cannot drift from the name.
    pub slug: String,
    pub description: String,
    /// `#RRGGBB` or `#RRGGBBAA`, or `None` to let presentation derive a tint.
    pub color_hex: Option<String>,
    pub origin: CategoryOrigin,
    pub members: Vec<CategoryMember>,
    pub created_at: UnixTimestampMilliseconds,
    pub updated_at: UnixTimestampMilliseconds,
    pub deleted: bool,
}

impl CategoryRecord {
    #[must_use]
    pub fn podcast_ids(&self) -> Vec<LibraryItemId> {
        self.member_ids(CategoryItemKind::Podcast)
    }

    #[must_use]
    pub fn episode_ids(&self) -> Vec<LibraryItemId> {
        self.member_ids(CategoryItemKind::Episode)
    }

    fn member_ids(&self, kind: CategoryItemKind) -> Vec<LibraryItemId> {
        self.members
            .iter()
            .filter(|member| member.kind == kind)
            .map(|member| member.item_id)
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CategoryValidationError {
    EmptyName,
    NameTooLarge,
    EmptyDescription,
    DescriptionTooLarge,
    InvalidColorHex,
    UnsupportedOrigin,
    TooManyCategories,
}

pub fn validate_category(
    name: &str,
    description: &str,
    color_hex: Option<&str>,
    origin: CategoryOrigin,
) -> Result<(), CategoryValidationError> {
    if name.trim().is_empty() {
        return Err(CategoryValidationError::EmptyName);
    }
    if name.len() > MAX_CATEGORY_NAME_BYTES {
        return Err(CategoryValidationError::NameTooLarge);
    }
    if description.trim().is_empty() {
        return Err(CategoryValidationError::EmptyDescription);
    }
    if description.len() > MAX_CATEGORY_DESCRIPTION_BYTES {
        return Err(CategoryValidationError::DescriptionTooLarge);
    }
    if matches!(origin, CategoryOrigin::Unsupported { .. }) {
        return Err(CategoryValidationError::UnsupportedOrigin);
    }
    validate_color_hex(color_hex)
}

pub fn validate_color_hex(value: Option<&str>) -> Result<(), CategoryValidationError> {
    let Some(value) = value else { return Ok(()) };
    let Some(digits) = value.strip_prefix('#') else {
        return Err(CategoryValidationError::InvalidColorHex);
    };
    if !matches!(digits.len(), 6 | 8) || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CategoryValidationError::InvalidColorHex);
    }
    Ok(())
}

/// Lowercase, ASCII-alphanumeric, single hyphens, no leading or trailing
/// hyphen. Non-ASCII characters are dropped rather than transliterated, so a
/// name with no ASCII at all yields an empty slug; callers fall back to the
/// category id in that case rather than minting a colliding empty slug.
#[must_use]
pub fn category_slug(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut pending_hyphen = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            if pending_hyphen && !slug.is_empty() {
                slug.push('-');
            }
            pending_hyphen = false;
            slug.push(character.to_ascii_lowercase());
        } else {
            pending_hyphen = true;
        }
    }
    slug
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_reject_empty_unbounded_and_malformed_input() {
        assert_eq!(
            validate_category("  ", "desc", None, CategoryOrigin::Agent),
            Err(CategoryValidationError::EmptyName)
        );
        assert_eq!(
            validate_category(
                &"x".repeat(MAX_CATEGORY_NAME_BYTES + 1),
                "desc",
                None,
                CategoryOrigin::Agent,
            ),
            Err(CategoryValidationError::NameTooLarge)
        );
        assert_eq!(
            validate_category("Philosophy", " ", None, CategoryOrigin::Agent),
            Err(CategoryValidationError::EmptyDescription)
        );
        assert_eq!(
            validate_category("Philosophy", "desc", Some("ff0000"), CategoryOrigin::Agent),
            Err(CategoryValidationError::InvalidColorHex)
        );
        assert_eq!(
            validate_category("Philosophy", "desc", Some("#ff00"), CategoryOrigin::Agent),
            Err(CategoryValidationError::InvalidColorHex)
        );
        assert_eq!(
            validate_category(
                "Philosophy",
                "desc",
                Some("#ff0000"),
                CategoryOrigin::Unsupported { wire_code: 9 },
            ),
            Err(CategoryValidationError::UnsupportedOrigin)
        );
        assert!(
            validate_category(
                "Philosophy",
                "Long-form conversations about meaning.",
                Some("#4A90D9FF"),
                CategoryOrigin::Agent,
            )
            .is_ok()
        );
    }

    #[test]
    fn slugs_collapse_punctuation_without_leading_or_trailing_hyphens() {
        assert_eq!(
            category_slug("Technology Deep-Dives"),
            "technology-deep-dives"
        );
        assert_eq!(category_slug("  Marketing!!  "), "marketing");
        assert_eq!(category_slug("A  --  B"), "a-b");
        // No ASCII to slug: the caller must fall back to the id rather than
        // treat this as a usable routing key.
        assert_eq!(category_slug("哲学"), "");
    }
}
