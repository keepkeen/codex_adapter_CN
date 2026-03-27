use super::FetchedPage;
use super::decode_html_entities_minimal;
use super::extract_title;
use super::html_page_to_text;
use super::looks_like_html;
use super::normalize_plain_text;
use regex_lite::Regex;
use std::sync::OnceLock;
use url::Url;

const DOCS_ATTR_MARKERS: &[&str] = &[
    "api-reference",
    "doc-item",
    "doc-content",
    "documentation",
    "docs-content",
    "gitbook-markdown",
    "markdown",
    "markdown-body",
    "md-content",
    "mintlify",
    "prose",
    "readme",
    "rm-markdown",
    "sl-markdown-content",
    "theme-doc-markdown",
    "vp-doc",
];

const NOISE_ATTR_MARKERS: &[&str] = &[
    "announcement",
    "breadcrumb",
    "cookie",
    "footer",
    "menu",
    "nav",
    "navbar",
    "pagination",
    "sidebar",
    "table-of-contents",
    "toc",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SourceKind {
    DocsPage,
    Generic,
    GitHubBlob,
    GitHubRelease,
    GitHubRepo,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct FetchPlan {
    pub request_url: String,
    pub display_url: String,
    pub implicit_focus: Option<String>,
    pub source_kind: SourceKind,
}

pub(super) fn build_fetch_plan(url: &str) -> Result<FetchPlan, String> {
    let parsed = Url::parse(url).map_err(|err| format!("invalid URL `{url}`: {err}"))?;
    let source_kind = classify_source(&parsed);
    let request_url = github_blob_raw_url(&parsed).unwrap_or_else(|| url_without_fragment(&parsed));

    Ok(FetchPlan {
        request_url,
        display_url: url.to_string(),
        implicit_focus: fragment_to_focus(&parsed),
        source_kind,
    })
}

pub(super) fn extract_page(
    fetch_plan: &FetchPlan,
    response_url: &Url,
    content_type: &str,
    body: &str,
    truncated: bool,
) -> FetchedPage {
    let title = if looks_like_html(content_type, body) {
        extract_title(body)
    } else {
        plain_text_title(fetch_plan)
    };
    let text = if looks_like_html(content_type, body) {
        extract_html_page_text(fetch_plan.source_kind, body)
    } else {
        normalize_non_html_text(fetch_plan.source_kind, body)
    };

    FetchedPage {
        final_url: display_url(fetch_plan, response_url),
        title,
        content_type: content_type.to_string(),
        text,
        truncated,
        implicit_focus: fetch_plan.implicit_focus.clone(),
    }
}

pub(super) fn github_blob_raw_url(url: &Url) -> Option<String> {
    (classify_source(url) == SourceKind::GitHubBlob).then(|| {
        let mut raw_url = url.clone();
        raw_url.set_fragment(None);
        let mut query_pairs = raw_url
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .filter(|(key, _)| key != "raw")
            .collect::<Vec<_>>();
        query_pairs.push(("raw".to_string(), "1".to_string()));
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (key, value) in query_pairs {
            serializer.append_pair(key.as_str(), value.as_str());
        }
        raw_url.set_query(Some(serializer.finish().as_str()));
        raw_url.to_string()
    })
}

fn display_url(fetch_plan: &FetchPlan, response_url: &Url) -> String {
    if fetch_plan.source_kind == SourceKind::GitHubBlob {
        fetch_plan.display_url.clone()
    } else {
        response_url.to_string()
    }
}

fn plain_text_title(fetch_plan: &FetchPlan) -> Option<String> {
    let parsed = Url::parse(fetch_plan.display_url.as_str()).ok()?;
    parsed
        .path_segments()
        .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
        .map(str::to_string)
}

fn url_without_fragment(url: &Url) -> String {
    let mut url = url.clone();
    url.set_fragment(None);
    url.to_string()
}

fn fragment_to_focus(url: &Url) -> Option<String> {
    let fragment = url.fragment()?.trim();
    let fragment = fragment.trim_matches('/');
    if fragment.is_empty() {
        return None;
    }

    let normalized = fragment
        .replace("%20", " ")
        .replace(['-', '_', '.', '/'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!normalized.is_empty()).then_some(normalized)
}

fn classify_source(url: &Url) -> SourceKind {
    let host = url
        .host_str()
        .map(|host| host.trim_start_matches("www.").to_ascii_lowercase())
        .unwrap_or_default();
    let path = url.path().to_ascii_lowercase();

    if host == "github.com" {
        if path.contains("/blob/") {
            return SourceKind::GitHubBlob;
        }
        if path.contains("/releases") {
            return SourceKind::GitHubRelease;
        }
        if looks_like_github_repo_path(path.as_str()) {
            return SourceKind::GitHubRepo;
        }
    }

    if host.starts_with("docs.")
        || host.starts_with("developer.")
        || matches!(
            host.as_str(),
            "developers.openai.com"
                | "docs.github.com"
                | "docs.anthropic.com"
                | "platform.openai.com"
                | "platform.moonshot.cn"
        )
        || path.starts_with("/docs")
        || path.contains("/reference")
        || path.contains("/api")
    {
        return SourceKind::DocsPage;
    }

    SourceKind::Generic
}

fn looks_like_github_repo_path(path: &str) -> bool {
    let segments = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    segments.len() >= 2
        && !matches!(
            segments[0],
            "apps"
                | "codespaces"
                | "collections"
                | "customers"
                | "enterprise"
                | "events"
                | "explore"
                | "features"
                | "issues"
                | "marketplace"
                | "orgs"
                | "organizations"
                | "search"
                | "settings"
                | "site"
                | "sponsors"
                | "topics"
                | "users"
        )
}

fn extract_html_page_text(source_kind: SourceKind, html: &str) -> String {
    let sanitized = strip_noise_sections(html);
    let extracted = match source_kind {
        SourceKind::DocsPage => docs_article_html(sanitized.as_str()),
        SourceKind::GitHubRelease | SourceKind::GitHubRepo => {
            github_article_html(sanitized.as_str())
        }
        SourceKind::Generic | SourceKind::GitHubBlob => None,
    };
    let text = extracted
        .as_deref()
        .map(structured_html_to_text)
        .unwrap_or_else(|| html_page_to_text(sanitized.as_str()));
    if text.is_empty() {
        html_page_to_text(html)
    } else {
        text
    }
}

fn normalize_non_html_text(source_kind: SourceKind, body: &str) -> String {
    if source_kind == SourceKind::GitHubBlob {
        normalize_code_text(body)
    } else {
        normalize_plain_text(body)
    }
}

fn github_article_html(html: &str) -> Option<String> {
    extract_element_with_markers(html, "article", DOCS_ATTR_MARKERS)
        .or_else(|| extract_element_with_markers(html, "div", &["markdown-body", "readme"]))
        .or_else(|| extract_element(html, "article"))
        .or_else(|| extract_element(html, "main"))
}

fn docs_article_html(html: &str) -> Option<String> {
    extract_element_with_markers(html, "article", DOCS_ATTR_MARKERS)
        .or_else(|| extract_element_with_markers(html, "main", DOCS_ATTR_MARKERS))
        .or_else(|| extract_element_with_markers(html, "div", DOCS_ATTR_MARKERS))
        .or_else(|| extract_element(html, "article"))
        .or_else(|| extract_element(html, "main"))
}

fn extract_element(html: &str, tag: &str) -> Option<String> {
    extract_element_with_markers(html, tag, &[])
}

fn extract_element_with_markers(html: &str, tag: &str, markers: &[&str]) -> Option<String> {
    let regex = Regex::new(&format!(r"(?is)<{tag}\b([^>]*)>(.*?)</{tag}>")).ok()?;
    regex.captures_iter(html).find_map(|captures| {
        let attrs = captures
            .get(1)
            .map(|capture| capture.as_str().to_ascii_lowercase())
            .unwrap_or_default();
        let matches_markers =
            markers.is_empty() || markers.iter().any(|marker| attrs.contains(marker));
        matches_markers.then(|| captures.get(0).map(|capture| capture.as_str().to_string()))?
    })
}

fn strip_noise_sections(html: &str) -> String {
    let mut stripped = html.to_string();
    for tag in ["nav", "aside", "footer", "header", "dialog"] {
        let regex = Regex::new(&format!(r"(?is)<{tag}\b.*?</{tag}>"))
            .unwrap_or_else(|err| panic!("invalid strip regex for {tag}: {err}"));
        stripped = regex.replace_all(stripped.as_str(), " ").into_owned();
    }

    noise_container_regex()
        .replace_all(stripped.as_str(), " ")
        .into_owned()
}

fn noise_container_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        let markers = NOISE_ATTR_MARKERS.join("|");
        Regex::new(&format!(
            r#"(?is)<(?:div|section|aside|nav)[^>]*(?:class|id)\s*=\s*["'][^"']*(?:{markers})[^"']*["'][^>]*>.*?</(?:div|section|aside|nav)>"#
        ))
        .unwrap_or_else(|err| panic!("invalid noise container regex: {err}"))
    })
}

fn structured_html_to_text(html: &str) -> String {
    let html = pre_block_regex().replace_all(html, |captures: &regex_lite::Captures<'_>| {
        let block = captures
            .get(1)
            .map(|capture| capture.as_str())
            .unwrap_or_default();
        let block = html_page_to_text(block);
        if block.is_empty() {
            String::new()
        } else {
            format!("\nCode sample:\n{block}\n")
        }
    });
    let html = list_item_regex().replace_all(&html, "\n- ");
    let html = heading_regex().replace_all(&html, "\n");
    html_page_to_text(html.as_ref())
}

fn pre_block_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<pre\b[^>]*>(.*?)</pre>"#)
            .unwrap_or_else(|err| panic!("invalid pre regex: {err}"))
    })
}

fn list_item_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<li\b[^>]*>"#)
            .unwrap_or_else(|err| panic!("invalid list item regex: {err}"))
    })
}

fn heading_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)</?h[1-6]\b[^>]*>"#)
            .unwrap_or_else(|err| panic!("invalid heading regex: {err}"))
    })
}

fn normalize_code_text(text: &str) -> String {
    let normalized = decode_html_entities_minimal(text)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let lines = normalized.lines().map(str::trim_end).collect::<Vec<_>>();
    let Some(first_non_empty) = lines.iter().position(|line| !line.trim().is_empty()) else {
        return String::new();
    };
    let last_non_empty = lines
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .unwrap_or(first_non_empty);
    lines[first_non_empty..=last_non_empty].join("\n")
}
