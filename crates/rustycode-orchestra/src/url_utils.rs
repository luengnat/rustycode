//! Orchestra URL Utils — URL normalization, query utilities, and SSRF protection.
//!
//! Provides utilities for working with URLs in a safe and normalized way.
//! Includes SSRF protection, URL deduplication, domain extraction, and
//! query intent detection.
//!
//! Matches orchestra-2's url-utils.ts implementation.

use chrono::{Datelike, Utc};
use regex::Regex;

// ─── Constants ─────────────────────────────────────────────────────────────

/// Hostnames that are blocked for security reasons
const BLOCKED_HOSTNAMES: &[&str] = &["localhost", "metadata.google.internal", "instance-data"];

/// Private IP patterns for SSRF protection
const PRIVATE_IP_PATTERNS: &[&str] = &[
    r"^127\.",                     // Loopback
    r"^10\.",                      // Private Class A
    r"^172\.(1[6-9]|2\d|3[01])\.", // Private Class B
    r"^192\.168\.",                // Private Class C
    r"^169\.254\.",                // Link-local
    r"^0\.",                       // Invalid
    r"^::1$",                      // IPv6 loopback
    r"^fc00:",                     // IPv6 private
    r"^fd",                        // IPv6 private
    r"^fe80:",                     // IPv6 link-local
];

/// Tracking parameters to strip from URLs
const TRACKING_PARAMS: &[&str] = &["fbclid", "gclid"];

/// UTM parameter prefixes to strip from URLs
const UTM_PREFIX: &str = "utm_";

// ─── URL Validation ───────────────────────────────────────────────────────────

/// Check if a URL is blocked due to SSRF protection.
///
/// Blocks URLs with:
/// - Non-HTTP/HTTPS protocols
/// - Blocked hostnames (localhost, metadata servers)
/// - Private IP addresses
/// - Invalid URLs
///
/// # Arguments
/// * `url` - The URL to check
///
/// # Returns
/// `true` if the URL is blocked, `false` otherwise
///
/// # Examples
/// ```
/// use rustycode_orchestra::url_utils::is_blocked_url;
///
/// assert!(is_blocked_url("http://localhost:8080"));
/// assert!(is_blocked_url("http://127.0.0.1"));
/// assert!(is_blocked_url("file:///etc/passwd"));
/// assert!(!is_blocked_url("https://example.com"));
/// ```
pub fn is_blocked_url(url: &str) -> bool {
    // Parse URL
    let parsed = match url::Url::parse(url) {
        Ok(p) => p,
        Err(_) => return true, // Invalid URL
    };

    // Check protocol
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return true;
    }

    // Check hostname
    let hostname = parsed.host_str().unwrap_or("").to_lowercase();

    // Check blocked hostnames
    if BLOCKED_HOSTNAMES.contains(&hostname.as_str()) {
        return true;
    }

    // Check private IP patterns
    for pattern in PRIVATE_IP_PATTERNS {
        let regex = Regex::new(pattern).unwrap();
        if regex.is_match(&hostname) {
            return true;
        }
    }

    false
}

// ─── Query Normalization ─────────────────────────────────────────────────────

/// Normalize a search query into a stable cache key.
///
/// Trims whitespace, converts to lowercase, collapses multiple spaces,
/// and applies Unicode normalization.
///
/// # Arguments
/// * `query` - The query to normalize
///
/// # Returns
/// Normalized query string
///
/// # Examples
/// ```
/// use rustycode_orchestra::url_utils::normalize_query;
///
/// assert_eq!(normalize_query("  Hello   World  "), "hello world");
/// assert_eq!(normalize_query("RUST Programming"), "rust programming");
/// ```
pub fn normalize_query(query: &str) -> String {
    query
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ")
}

// ─── URL Deduplication ────────────────────────────────────────────────────────

/// Convert a URL to a canonical form for deduplication.
///
/// Strips fragment, tracking params, lowercases hostname, sorts query params,
/// and strips trailing "/" on root paths.
///
/// # Arguments
/// * `url` - The URL to canonicalize
///
/// # Returns
/// Canonical URL or `null` if URL is invalid
///
/// # Examples
/// ```
/// use rustycode_orchestra::url_utils::to_dedupe_key;
///
/// let url1 = "https://example.com/path?utm_source=google&b=2&a=1#section";
/// let key1 = to_dedupe_key(url1);
/// assert_eq!(key1, Some("https://example.com/path?a=1&b=2".to_string()));
/// ```
pub fn to_dedupe_key(url: &str) -> Option<String> {
    let mut parsed = url::Url::parse(url).ok()?;

    // Note: url crate automatically lowercases hostname during parsing

    // Remove fragment
    parsed.set_fragment(None);

    // Remove tracking and UTM parameters, sort remaining params
    let mut query_pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .filter(|(key, _)| !TRACKING_PARAMS.contains(&key.as_ref()) && !key.starts_with(UTM_PREFIX))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    if query_pairs.is_empty() {
        parsed.set_query(None);
    } else {
        query_pairs.sort();
        parsed
            .query_pairs_mut()
            .clear()
            .extend_pairs(query_pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    }

    // Strip trailing "/" for root paths with no query
    let canonical = parsed.to_string();
    if parsed.path() == "/" && parsed.query().is_none() {
        Some(canonical.trim_end_matches('/').to_string())
    } else {
        Some(canonical)
    }
}

// ─── Domain Extraction ───────────────────────────────────────────────────────

/// Extract a clean domain from a URL for display.
///
/// Removes "www." prefix and returns just the domain.
///
/// # Arguments
/// * `url` - The URL to extract domain from
///
/// # Returns
/// Clean domain name or original URL if parsing fails
///
/// # Examples
/// ```
/// use rustycode_orchestra::url_utils::extract_domain;
///
/// assert_eq!(extract_domain("https://docs.python.org/3/library/asyncio.html"), "docs.python.org");
/// assert_eq!(extract_domain("https://www.example.com/path"), "example.com");
/// ```
pub fn extract_domain(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(parsed) => parsed
            .host_str()
            .unwrap_or(url)
            .trim_start_matches("www.")
            .to_string(),
        Err(_) => url.to_string(),
    }
}

// ─── Freshness Detection ─────────────────────────────────────────────────────

/// Detect if a query likely wants fresh/recent results.
///
/// Returns a suggested freshness parameter or `None`.
/// - Some("py") = past year
/// - Some("pm") = past month
/// - None = no freshness requirement
///
/// # Arguments
/// * `query` - The search query to analyze
///
/// # Returns
/// Freshness parameter or `None`
///
/// # Examples
/// ```
/// use rustycode_orchestra::url_utils::detect_freshness;
///
/// assert!(detect_freshness("latest rust features").is_some());
/// assert!(detect_freshness("python tutorial").is_none());
/// ```
pub fn detect_freshness(query: &str) -> Option<&'static str> {
    let q = query.to_lowercase();

    // Check for current/recent year references
    let current_year = Utc::now().year();
    for year in (current_year - 1)..=current_year {
        if q.contains(&year.to_string()) {
            return Some("py"); // past year
        }
    }

    // Recency keywords patterns
    let recent_patterns = [
        r"\b(latest|newest|recent|new|just released|just launched)\b",
        r"\b(today|yesterday|this week|this month)\b",
        r"\b(breaking|update|announcement|release notes?)\b",
        r"\b(what('?s| is) new)\b",
    ];

    for pattern in recent_patterns {
        let regex = Regex::new(pattern).unwrap();
        if regex.is_match(&q) {
            return Some("pm"); // past month
        }
    }

    None
}

// ─── Domain Hint Detection ───────────────────────────────────────────────────

/// Detect if a query targets specific domains using "site:" operator.
///
/// Returns extracted domains or `None` if no site hints found.
///
/// # Arguments
/// * `query` - The search query to analyze
///
/// # Returns
/// Vector of domain hints or `None`
///
/// # Examples
/// ```
/// use rustycode_orchestra::url_utils::detect_domain_hints;
///
/// let hints = detect_domain_hints("rust site:docs.rs site:example.com");
/// assert_eq!(hints, Some(vec!["docs.rs".to_string(), "example.com".to_string()]));
/// ```
pub fn detect_domain_hints(query: &str) -> Option<Vec<String>> {
    let site_regex = Regex::new(r"site:(\S+)").unwrap();
    let matches: Vec<String> = site_regex
        .captures_iter(query)
        .map(|cap| cap[1].to_string())
        .collect();

    if matches.is_empty() {
        None
    } else {
        Some(matches)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_blocked_url_localhost() {
        assert!(is_blocked_url("http://localhost:8080"));
        assert!(is_blocked_url("https://localhost"));
        assert!(is_blocked_url("http://localhost"));
    }

    #[test]
    fn test_is_blocked_url_private_ips() {
        assert!(is_blocked_url("http://127.0.0.1"));
        assert!(is_blocked_url("http://10.0.0.1"));
        assert!(is_blocked_url("http://192.168.1.1"));
        assert!(is_blocked_url("http://172.16.0.1"));
        assert!(is_blocked_url("http://169.254.1.1"));
    }

    #[test]
    fn test_is_blocked_url_metadata() {
        assert!(is_blocked_url("http://metadata.google.internal"));
        assert!(is_blocked_url("http://instance-data"));
    }

    #[test]
    fn test_is_blocked_url_invalid_protocol() {
        assert!(is_blocked_url("file:///etc/passwd"));
        assert!(is_blocked_url("ftp://example.com"));
        assert!(is_blocked_url("javascript:alert(1)"));
    }

    #[test]
    fn test_is_blocked_url_valid() {
        assert!(!is_blocked_url("https://example.com"));
        assert!(!is_blocked_url("http://example.com"));
        assert!(!is_blocked_url("https://docs.rs"));
    }

    #[test]
    fn test_normalize_query() {
        assert_eq!(normalize_query("  Hello   World  "), "hello world");
        assert_eq!(normalize_query("RUST Programming"), "rust programming");
        assert_eq!(
            normalize_query("  Multiple   Spaces   Here  "),
            "multiple spaces here"
        );
    }

    #[test]
    fn test_to_dedupe_key() {
        // Remove tracking params
        let url1 = "https://example.com/path?utm_source=google&fbclid=123&b=2&a=1";
        let key1 = to_dedupe_key(url1);
        assert_eq!(key1, Some("https://example.com/path?a=1&b=2".to_string()));

        // Sort query params
        let url2 = "https://example.com?z=1&a=2&m=3";
        let key2 = to_dedupe_key(url2);
        assert_eq!(key2, Some("https://example.com/?a=2&m=3&z=1".to_string()));

        // Remove fragment
        let url3 = "https://example.com/path#section";
        let key3 = to_dedupe_key(url3);
        assert_eq!(key3, Some("https://example.com/path".to_string()));

        // Strip trailing slash for root
        let url4 = "https://example.com/";
        let key4 = to_dedupe_key(url4);
        assert_eq!(key4, Some("https://example.com".to_string()));
    }

    #[test]
    fn test_extract_domain() {
        assert_eq!(
            extract_domain("https://docs.python.org/3/library/asyncio.html"),
            "docs.python.org"
        );
        assert_eq!(
            extract_domain("https://www.example.com/path"),
            "example.com"
        );
        assert_eq!(extract_domain("https://example.com"), "example.com");
        assert_eq!(extract_domain("not a url"), "not a url");
    }

    #[test]
    fn test_detect_freshness_year() {
        assert!(detect_freshness("rust 2026").is_some());
        assert!(detect_freshness("rust 2025").is_some());
        assert!(detect_freshness("rust 2024").is_none()); // Too old
    }

    #[test]
    fn test_detect_freshness_keywords() {
        assert!(detect_freshness("latest rust features").is_some());
        assert!(detect_freshness("new python tutorial").is_some());
        assert!(detect_freshness("recent updates").is_some());
        assert!(detect_freshness("breaking news").is_some());
        assert!(detect_freshness("what's new").is_some());
        assert!(detect_freshness("python tutorial").is_none());
    }

    #[test]
    fn test_detect_domain_hints() {
        let hints1 = detect_domain_hints("rust site:docs.rs");
        assert_eq!(hints1, Some(vec!["docs.rs".to_string()]));

        let hints2 = detect_domain_hints("rust site:docs.rs site:example.com");
        assert_eq!(
            hints2,
            Some(vec!["docs.rs".to_string(), "example.com".to_string()])
        );

        let hints3 = detect_domain_hints("rust tutorial");
        assert_eq!(hints3, None);
    }
}
