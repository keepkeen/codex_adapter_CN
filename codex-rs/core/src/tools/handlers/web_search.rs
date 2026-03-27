use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use async_trait::async_trait;
use codex_protocol::config_types::WebSearchConfig;
use codex_protocol::config_types::WebSearchContextSize;
use codex_utils_string::take_bytes_at_char_boundary;
use rand::Rng;
use regex_lite::Regex;
use reqwest::header::ACCEPT;
use reqwest::header::ACCEPT_LANGUAGE;
use reqwest::header::CACHE_CONTROL;
use reqwest::header::CONNECTION;
use reqwest::header::CONTENT_TYPE;
use reqwest::header::DNT;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use reqwest::header::UPGRADE_INSECURE_REQUESTS;
use reqwest::header::USER_AGENT;
use serde::Deserialize;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::Duration;
use std::time::Instant;
use url::Url;

#[path = "web_search_processing.rs"]
mod processing;
#[path = "web_search_sources.rs"]
mod sources;

use processing::expand_search_queries;
use processing::extract_focused_excerpt;
use processing::fallback_search_results;
use processing::rerank_search_results;
use sources::build_fetch_plan;
use sources::extract_page;

const WEB_SEARCH_URL: &str = "https://html.duckduckgo.com/html/";
const DEFAULT_MAX_RESULTS: usize = 8;
const MAX_RESULTS: usize = 20;
const MAX_SEARCH_RESPONSE_BYTES: usize = 512 * 1024;
const MAX_PAGE_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_OPEN_PAGE_BYTES: usize = 12_000;
const MAX_FIND_IN_PAGE_MATCHES: usize = 10;
const MAX_MATCH_LINE_BYTES: usize = 400;

const USER_AGENTS: [&str; 6] = [
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:133.0) Gecko/20100101 Firefox/133.0",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:133.0) Gecko/20100101 Firefox/133.0",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.1 Safari/605.1.15",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0",
];

const ACCEPT_LANGUAGES: [&str; 4] = [
    "en-US,en;q=0.9",
    "en-US,en;q=0.9,zh;q=0.7",
    "en-GB,en;q=0.9,en-US;q=0.8",
    "zh-CN,zh;q=0.9,en;q=0.7",
];

static LAST_SEARCH_AT: Mutex<Option<Instant>> = Mutex::new(None);

pub struct WebSearchHandler {
    client: reqwest::Client,
    config: Option<WebSearchConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SearchResult {
    title: String,
    url: String,
    snippet: String,
}

#[derive(Debug)]
struct FetchedPage {
    final_url: String,
    title: Option<String>,
    content_type: String,
    text: String,
    truncated: bool,
    implicit_focus: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum WebSearchAction {
    Search,
    OpenPage,
    FindInPage,
}

#[derive(Deserialize)]
struct WebSearchArgs {
    action: WebSearchAction,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    pattern: Option<String>,
    #[serde(default)]
    max_results: Option<usize>,
    #[serde(default)]
    focus: Option<String>,
}

impl WebSearchHandler {
    pub fn new(config: Option<WebSearchConfig>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .unwrap_or_else(|err| {
                tracing::warn!(error = %err, "failed to build web search client");
                reqwest::Client::new()
            });

        Self { client, config }
    }

    async fn search(&self, query: &str, max_results: usize) -> Result<Vec<SearchResult>, String> {
        let allowed_domains = allowed_domains(self.config.as_ref());
        let queries = expand_search_queries(query);
        let per_query_max = max_results.saturating_mul(2).clamp(2, MAX_RESULTS);
        let mut merged_results = Vec::new();
        let mut first_error = None;

        for expanded_query in queries {
            match self
                .search_once(expanded_query.as_str(), per_query_max)
                .await
            {
                Ok(results) => merged_results.extend(results),
                Err(err) => {
                    tracing::debug!(
                        query,
                        expanded_query,
                        error = %err,
                        "web search variant failed"
                    );
                    if first_error.is_none() {
                        first_error = Some(err);
                    }
                }
            }
        }

        if let Some(allowed_domains) = allowed_domains {
            merged_results
                .retain(|result| url_matches_allowed_domains(result.url.as_str(), allowed_domains));
        }
        let mut results = rerank_search_results(query, merged_results, max_results);
        if results.is_empty() {
            results = fallback_search_results(query, max_results);
            if let Some(allowed_domains) = allowed_domains {
                results.retain(|result| {
                    url_matches_allowed_domains(result.url.as_str(), allowed_domains)
                });
            }
        }
        if results.is_empty()
            && let Some(err) = first_error
        {
            return Err(err);
        }
        Ok(results)
    }

    async fn search_once(
        &self,
        query: &str,
        max_results: usize,
    ) -> Result<Vec<SearchResult>, String> {
        maybe_delay_search().await;

        let search_query = apply_allowed_domain_hint(query, allowed_domains(self.config.as_ref()));
        let response = self
            .client
            .get(WEB_SEARCH_URL)
            .query(&[("q", search_query.as_str())])
            .headers(browser_headers())
            .send()
            .await
            .map_err(|err| format!("failed to execute search: {err}"))?;

        let status = response.status();
        if !status.is_success() {
            return Err(format!("search request failed with status {status}"));
        }

        let (body, truncated) = read_response_text(response, MAX_SEARCH_RESPONSE_BYTES)
            .await
            .map_err(|err| format!("failed to read search response: {err}"))?;
        if truncated {
            tracing::debug!(
                query,
                "web search response body truncated while parsing search results"
            );
        }
        Ok(parse_search_results(&body, max_results))
    }

    async fn fetch_page(&self, url: &str) -> Result<FetchedPage, String> {
        validate_http_url(url)?;
        enforce_allowed_domains(url, allowed_domains(self.config.as_ref()))?;
        let fetch_plan = build_fetch_plan(url)?;

        let response = self
            .client
            .get(fetch_plan.request_url.as_str())
            .headers(browser_headers())
            .send()
            .await
            .map_err(|err| format!("failed to fetch {url}: {err}"))?;

        let status = response.status();
        if !status.is_success() {
            return Err(format!("request for {url} failed with status {status}"));
        }

        let response_url = response.url().clone();
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let (body, truncated) = read_response_text(response, MAX_PAGE_RESPONSE_BYTES)
            .await
            .map_err(|err| format!("failed to read page body: {err}"))?;
        Ok(extract_page(
            &fetch_plan,
            &response_url,
            content_type.as_str(),
            &body,
            truncated,
        ))
    }
}

#[async_trait]
impl ToolHandler for WebSearchHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation { payload, .. } = invocation;

        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "web_search handler received unsupported payload".to_string(),
                ));
            }
        };

        let args: WebSearchArgs = parse_arguments(&arguments)?;

        match args.action {
            WebSearchAction::Search => {
                let query = args
                    .query
                    .as_deref()
                    .map(str::trim)
                    .filter(|query| !query.is_empty())
                    .ok_or_else(|| {
                        FunctionCallError::RespondToModel(
                            "query is required when action is `search`".to_string(),
                        )
                    })?;
                let max_results = args
                    .max_results
                    .unwrap_or_else(|| default_max_results(self.config.as_ref()));
                if max_results == 0 {
                    return Err(FunctionCallError::RespondToModel(
                        "max_results must be greater than zero".to_string(),
                    ));
                }
                let results = self
                    .search(query, max_results.min(MAX_RESULTS))
                    .await
                    .map_err(FunctionCallError::RespondToModel)?;
                let success = !results.is_empty();
                Ok(FunctionToolOutput::from_text(
                    format_search_results(query, &results, allowed_domains(self.config.as_ref())),
                    Some(success),
                ))
            }
            WebSearchAction::OpenPage => {
                let url = args
                    .url
                    .as_deref()
                    .map(str::trim)
                    .filter(|url| !url.is_empty())
                    .ok_or_else(|| {
                        FunctionCallError::RespondToModel(
                            "url is required when action is `open_page`".to_string(),
                        )
                    })?;
                let page = self
                    .fetch_page(url)
                    .await
                    .map_err(FunctionCallError::RespondToModel)?;
                let success = !page.text.is_empty();
                Ok(FunctionToolOutput::from_text(
                    format_open_page(&page, args.focus.as_deref()),
                    Some(success),
                ))
            }
            WebSearchAction::FindInPage => {
                let url = args
                    .url
                    .as_deref()
                    .map(str::trim)
                    .filter(|url| !url.is_empty())
                    .ok_or_else(|| {
                        FunctionCallError::RespondToModel(
                            "url is required when action is `find_in_page`".to_string(),
                        )
                    })?;
                let pattern = args
                    .pattern
                    .as_deref()
                    .map(str::trim)
                    .filter(|pattern| !pattern.is_empty())
                    .ok_or_else(|| {
                        FunctionCallError::RespondToModel(
                            "pattern is required when action is `find_in_page`".to_string(),
                        )
                    })?;
                let page = self
                    .fetch_page(url)
                    .await
                    .map_err(FunctionCallError::RespondToModel)?;
                let matches = find_in_page(page.text.as_str(), pattern);
                let success = !matches.is_empty();
                Ok(FunctionToolOutput::from_text(
                    format_find_in_page(&page, pattern, &matches),
                    Some(success),
                ))
            }
        }
    }
}

fn default_max_results(config: Option<&WebSearchConfig>) -> usize {
    match config.and_then(|cfg| cfg.search_context_size) {
        Some(WebSearchContextSize::Low) => 5,
        Some(WebSearchContextSize::Medium) => DEFAULT_MAX_RESULTS,
        Some(WebSearchContextSize::High) => 12,
        None => DEFAULT_MAX_RESULTS,
    }
}

fn allowed_domains(config: Option<&WebSearchConfig>) -> Option<&[String]> {
    config
        .and_then(|cfg| cfg.filters.as_ref())
        .and_then(|filters| filters.allowed_domains.as_deref())
}

fn apply_allowed_domain_hint(query: &str, allowed_domains: Option<&[String]>) -> String {
    let Some([domain]) = allowed_domains else {
        return query.to_string();
    };
    let domain = domain.trim();
    if domain.is_empty() || query.contains("site:") {
        query.to_string()
    } else {
        format!("site:{domain} {query}")
    }
}

fn browser_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    let mut rng = rand::rng();

    insert_header(
        &mut headers,
        USER_AGENT,
        USER_AGENTS[rng.random_range(0..USER_AGENTS.len())],
    );
    insert_header(
        &mut headers,
        ACCEPT,
        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
    );
    insert_header(
        &mut headers,
        ACCEPT_LANGUAGE,
        ACCEPT_LANGUAGES[rng.random_range(0..ACCEPT_LANGUAGES.len())],
    );
    insert_header(&mut headers, CONNECTION, "keep-alive");
    insert_header(&mut headers, CACHE_CONTROL, "max-age=0");
    insert_header(&mut headers, UPGRADE_INSECURE_REQUESTS, "1");
    if rng.random_range(0..2) == 0 {
        insert_header(&mut headers, DNT, "1");
    }

    headers
}

fn insert_header(headers: &mut HeaderMap, name: reqwest::header::HeaderName, value: &str) {
    if let Ok(value) = HeaderValue::from_str(value) {
        headers.insert(name, value);
    }
}

async fn maybe_delay_search() {
    let delay = {
        let mut guard = LAST_SEARCH_AT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let min_gap = Duration::from_millis(rand::rng().random_range(500..1_501));
        let now = Instant::now();
        let delay = guard
            .and_then(|last_at| last_at.checked_add(min_gap))
            .and_then(|next_allowed| next_allowed.checked_duration_since(now));
        let scheduled_at = delay.map_or(now, |wait| now + wait);
        *guard = Some(scheduled_at);
        delay
    };

    if let Some(delay) = delay
        && !delay.is_zero()
    {
        tokio::time::sleep(delay).await;
    }

    let mut guard = LAST_SEARCH_AT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *guard = Some(Instant::now());
}

async fn read_response_text(
    mut response: reqwest::Response,
    max_bytes: usize,
) -> Result<(String, bool), reqwest::Error> {
    let mut bytes = Vec::new();
    let mut truncated = false;

    while let Some(chunk) = response.chunk().await? {
        let remaining = max_bytes.saturating_sub(bytes.len());
        if remaining == 0 {
            truncated = true;
            break;
        }
        if chunk.len() > remaining {
            bytes.extend_from_slice(&chunk[..remaining]);
            truncated = true;
            break;
        }
        bytes.extend_from_slice(&chunk);
    }

    Ok((String::from_utf8_lossy(&bytes).into_owned(), truncated))
}

fn validate_http_url(url: &str) -> Result<(), String> {
    let parsed = Url::parse(url).map_err(|err| format!("invalid URL `{url}`: {err}"))?;
    match parsed.scheme() {
        "http" | "https" => Ok(()),
        scheme => Err(format!(
            "unsupported URL scheme `{scheme}`; only http and https are allowed"
        )),
    }
}

fn enforce_allowed_domains(url: &str, allowed_domains: Option<&[String]>) -> Result<(), String> {
    if let Some(allowed_domains) = allowed_domains
        && !url_matches_allowed_domains(url, allowed_domains)
    {
        return Err(format!(
            "URL `{url}` is outside the configured allowed_domains filter"
        ));
    }
    Ok(())
}

fn url_matches_allowed_domains(url: &str, allowed_domains: &[String]) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host = host.to_ascii_lowercase();

    allowed_domains.iter().any(|allowed_domain| {
        let allowed_domain = allowed_domain
            .trim()
            .trim_start_matches('.')
            .to_ascii_lowercase();
        !allowed_domain.is_empty()
            && (host == allowed_domain || host.ends_with(&format!(".{allowed_domain}")))
    })
}

fn parse_search_results(html: &str, max_results: usize) -> Vec<SearchResult> {
    let anchors = anchor_regex()
        .captures_iter(html)
        .filter_map(|captures| {
            let matched = captures.get(0)?;
            let href = captures.get(1)?.as_str();
            let title = html_fragment_to_text(captures.get(2)?.as_str());
            if title.is_empty() {
                return None;
            }
            Some((
                matched.start(),
                matched.end(),
                clean_duckduckgo_url(href),
                title,
            ))
        })
        .collect::<Vec<_>>();

    let mut results = Vec::new();
    for (index, (_, anchor_end, url, title)) in anchors.iter().enumerate() {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            continue;
        }
        let window_end = anchors
            .get(index + 1)
            .map_or(html.len(), |(next_start, ..)| *next_start);
        let snippet_window = &html[*anchor_end..window_end];
        let snippet = snippet_regex()
            .captures(snippet_window)
            .and_then(|captures| captures.get(1).or_else(|| captures.get(2)))
            .map(|capture| html_fragment_to_text(capture.as_str()))
            .unwrap_or_default();

        results.push(SearchResult {
            title: title.clone(),
            url: url.clone(),
            snippet,
        });
        if results.len() == max_results {
            break;
        }
    }

    results
}

fn anchor_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<a[^>]*class="result__a"[^>]*href="([^"]+)"[^>]*>(.*?)</a>"#)
            .unwrap_or_else(|err| panic!("invalid anchor regex: {err}"))
    })
}

fn snippet_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?is)<(?:a|div)[^>]*class="result__snippet"[^>]*>(.*?)</a>|<div[^>]*class="result__snippet"[^>]*>(.*?)</div>"#,
        )
        .unwrap_or_else(|err| panic!("invalid snippet regex: {err}"))
    })
}

fn title_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<title[^>]*>(.*?)</title>"#)
            .unwrap_or_else(|err| panic!("invalid title regex: {err}"))
    })
}

fn script_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<script[^>]*>.*?</script>"#)
            .unwrap_or_else(|err| panic!("invalid script regex: {err}"))
    })
}

fn style_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<style[^>]*>.*?</style>"#)
            .unwrap_or_else(|err| panic!("invalid style regex: {err}"))
    })
}

fn noscript_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<noscript[^>]*>.*?</noscript>"#)
            .unwrap_or_else(|err| panic!("invalid noscript regex: {err}"))
    })
}

fn block_tag_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<\s*/?\s*(br|p|div|li|ul|ol|section|article|tr|td|th|h[1-6])[^>]*>"#)
            .unwrap_or_else(|err| panic!("invalid block tag regex: {err}"))
    })
}

fn tag_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<[^>]+>"#).unwrap_or_else(|err| panic!("invalid tag regex: {err}"))
    })
}

fn extract_title(html: &str) -> Option<String> {
    title_regex()
        .captures(html)
        .and_then(|captures| captures.get(1))
        .map(|capture| html_fragment_to_text(capture.as_str()))
        .filter(|title| !title.is_empty())
}

fn looks_like_html(content_type: &str, body: &str) -> bool {
    content_type.contains("text/html")
        || body.contains("<html")
        || body.contains("<body")
        || body.contains("<!DOCTYPE html")
}

fn html_page_to_text(html: &str) -> String {
    let without_scripts = script_regex().replace_all(html, " ");
    let without_styles = style_regex().replace_all(&without_scripts, " ");
    let without_noscript = noscript_regex().replace_all(&without_styles, " ");
    let with_breaks = block_tag_regex().replace_all(&without_noscript, "\n");
    let without_tags = tag_regex().replace_all(&with_breaks, " ");
    let decoded = decode_html_entities_minimal(without_tags.as_ref());
    collapse_whitespace_preserving_newlines(&decoded)
}

fn html_fragment_to_text(fragment: &str) -> String {
    let without_tags = tag_regex().replace_all(fragment, " ");
    let decoded = decode_html_entities_minimal(without_tags.as_ref());
    collapse_inline_whitespace(&decoded)
}

fn normalize_plain_text(text: &str) -> String {
    collapse_whitespace_preserving_newlines(text)
}

fn collapse_inline_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn collapse_whitespace_preserving_newlines(text: &str) -> String {
    text.lines()
        .map(collapse_inline_whitespace)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn decode_html_entities_minimal(text: &str) -> String {
    [
        ("&nbsp;", " "),
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&#x27;", "'"),
        ("&#x2F;", "/"),
    ]
    .into_iter()
    .fold(text.to_string(), |acc, (entity, replacement)| {
        acc.replace(entity, replacement)
    })
}

fn clean_duckduckgo_url(raw_url: &str) -> String {
    let raw_url = raw_url.trim();
    if raw_url.starts_with("//duckduckgo.com/l/?uddg=")
        && let Some((_, after)) = raw_url.split_once("uddg=")
    {
        let encoded = after.split('&').next().unwrap_or(after);
        if let Ok(decoded) = urlencoding_fallback(encoded) {
            return decoded;
        }
    }
    raw_url.to_string()
}

fn urlencoding_fallback(value: &str) -> Result<String, ()> {
    Url::parse(&format!("https://example.invalid/?uddg={value}"))
        .map_err(|_| ())?
        .query_pairs()
        .find_map(|(key, value)| (key == "uddg").then(|| value.into_owned()))
        .ok_or(())
}

fn format_search_results(
    query: &str,
    results: &[SearchResult],
    allowed_domains: Option<&[String]>,
) -> String {
    if results.is_empty() {
        let mut message = format!("No web results found for `{query}`.");
        if let Some(allowed_domains) = allowed_domains {
            message.push_str("\nAllowed domains: ");
            message.push_str(&allowed_domains.join(", "));
        }
        return message;
    }

    let mut output = format!("Search results for `{query}`:\n");
    if let Some(allowed_domains) = allowed_domains {
        output.push_str("Allowed domains: ");
        output.push_str(&allowed_domains.join(", "));
        output.push('\n');
    }
    output.push('\n');

    for (index, result) in results.iter().enumerate() {
        output.push_str(&format!("{}. {}\n", index + 1, result.title));
        output.push_str(&format!("   URL: {}\n", result.url));
        if !result.snippet.is_empty() {
            output.push_str(&format!("   Snippet: {}\n", result.snippet));
        }
        output.push('\n');
    }

    output
}

fn format_open_page(page: &FetchedPage, focus: Option<&str>) -> String {
    let mut output = format!("Opened page: {}\n", page.final_url);
    if let Some(title) = page.title.as_deref() {
        output.push_str(&format!("Title: {title}\n"));
    }
    if !page.content_type.is_empty() {
        output.push_str(&format!("Content-Type: {}\n", page.content_type));
    }
    output.push('\n');

    if page.text.is_empty() {
        output.push_str("No readable text was extracted from the page.");
        return output;
    }

    let effective_focus = focus
        .map(str::trim)
        .filter(|focus| !focus.is_empty())
        .or(page.implicit_focus.as_deref());
    let focused_excerpt = effective_focus
        .and_then(|focus| extract_focused_excerpt(page.text.as_str(), focus, MAX_OPEN_PAGE_BYTES));
    let used_focused_excerpt = focused_excerpt.is_some();
    let excerpt = focused_excerpt.unwrap_or_else(|| {
        take_bytes_at_char_boundary(page.text.as_str(), MAX_OPEN_PAGE_BYTES).to_string()
    });
    if let Some(focus) = effective_focus
        && used_focused_excerpt
    {
        output.push_str(&format!("Focused excerpts for `{focus}`:\n"));
    }
    output.push_str(&excerpt);
    if page.truncated || excerpt.len() < page.text.len() {
        output.push_str("\n\n[truncated]");
    }
    output
}

fn find_in_page(text: &str, pattern: &str) -> Vec<String> {
    let pattern_lower = pattern.to_ascii_lowercase();
    text.lines()
        .filter_map(|line| {
            let normalized = collapse_inline_whitespace(line);
            (!normalized.is_empty()
                && normalized
                    .to_ascii_lowercase()
                    .contains(pattern_lower.as_str()))
            .then(|| {
                take_bytes_at_char_boundary(normalized.as_str(), MAX_MATCH_LINE_BYTES).to_string()
            })
        })
        .take(MAX_FIND_IN_PAGE_MATCHES)
        .collect()
}

fn format_find_in_page(page: &FetchedPage, pattern: &str, matches: &[String]) -> String {
    if matches.is_empty() {
        return format!("No matches found for `{pattern}` in {}.", page.final_url);
    }

    let mut output = format!(
        "Found {} matches for `{pattern}` in {}:\n\n",
        matches.len(),
        page.final_url
    );
    for (index, matched_line) in matches.iter().enumerate() {
        output.push_str(&format!("{}. {}\n", index + 1, matched_line));
    }
    output
}

#[cfg(test)]
#[path = "web_search_tests.rs"]
mod tests;
