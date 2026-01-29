/// Canonical frontmatter key names used across markdown files.
///
/// Using these constants ensures consistency between file generation and parsing.
/// Always use these constants instead of string literals for frontmatter keys.
pub mod frontmatter {
    // Common identity fields
    pub const ID: &str = "id";
    pub const NAME: &str = "name";

    // Session-specific fields
    pub const STATUS: &str = "status";
    pub const CONTEXT_TOKENS: &str = "context_tokens";
    pub const CONTEXT_LIMIT: &str = "context_limit";

    // Timestamp fields
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";
    pub const LAST_ACTIVE: &str = "last_active";
    pub const CLOSED_AT: &str = "closed_at";

    // Signal-specific fields
    pub const SIGNAL_TYPE: &str = "signal_type";
    pub const PRIORITY: &str = "priority";
}
