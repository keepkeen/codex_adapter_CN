use super::SearchResult;
use codex_utils_string::take_bytes_at_char_boundary;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::OnceLock;
use url::Url;

const MAX_QUERY_VARIANTS: usize = 2;
const MAX_KEYWORD_TERMS: usize = 6;
const MAX_RESULTS_PER_HOST: usize = 2;
const TARGET_CHUNK_BYTES: usize = 700;
const MAX_EXCERPTS: usize = 3;

pub(super) fn expand_search_queries(query: &str) -> Vec<String> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }

    let mut queries = vec![query.to_string()];
    let keywords = distilled_keywords(query);
    if !keywords.is_empty() {
        let keyword_query = keywords.join(" ");
        if !query.eq_ignore_ascii_case(keyword_query.as_str()) {
            queries.push(keyword_query);
        }
    }
    queries.truncate(MAX_QUERY_VARIANTS);
    queries
}

pub(super) fn rerank_search_results(
    query: &str,
    results: Vec<SearchResult>,
    max_results: usize,
) -> Vec<SearchResult> {
    let query_lower = query.to_lowercase();
    let query_terms = tokenized_terms(query);
    let mut deduped = HashMap::<String, RankedSearchResult>::new();

    for result in results {
        let normalized_url =
            normalize_url(result.url.as_str()).unwrap_or_else(|| result.url.clone());
        let score = score_search_result(&query_lower, &query_terms, &result);
        let candidate = RankedSearchResult::new(result, score);

        match deduped.get_mut(&normalized_url) {
            Some(existing) => existing.merge(candidate),
            None => {
                deduped.insert(normalized_url, candidate);
            }
        }
    }

    let mut ranked = deduped
        .into_values()
        .map(|ranked| ranked.result)
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        score_search_result(&query_lower, &query_terms, right)
            .cmp(&score_search_result(&query_lower, &query_terms, left))
            .then_with(|| left.url.cmp(&right.url))
    });

    let mut host_counts = HashMap::<String, usize>::new();
    let mut selected = Vec::new();
    let mut overflow = Vec::new();

    for result in ranked {
        let host = normalized_host(result.url.as_str());
        let seen = host
            .as_ref()
            .and_then(|hostname| host_counts.get(hostname).copied())
            .unwrap_or_default();

        if host.is_some() && seen >= MAX_RESULTS_PER_HOST {
            overflow.push(result);
            continue;
        }

        if let Some(host) = host {
            *host_counts.entry(host).or_default() += 1;
        }
        selected.push(result);
        if selected.len() == max_results {
            return selected;
        }
    }

    for result in overflow {
        selected.push(result);
        if selected.len() == max_results {
            break;
        }
    }

    selected
}

pub(super) fn fallback_search_results(query: &str, max_results: usize) -> Vec<SearchResult> {
    let mut results = Vec::new();

    if let Some((owner, repo)) = github_repo_slug_from_query(query) {
        results.push(SearchResult {
            title: format!("GitHub - {owner}/{repo}"),
            url: format!("https://github.com/{owner}/{repo}"),
            snippet: "Canonical GitHub repository inferred directly from the query.".to_string(),
        });
    }

    results.truncate(max_results);
    results
}

pub(super) fn extract_focused_excerpt(text: &str, focus: &str, max_bytes: usize) -> Option<String> {
    let focus = focus.trim();
    if focus.is_empty() {
        return None;
    }

    let units = split_text_into_units(text);
    if units.is_empty() {
        return None;
    }

    let focus_lower = focus.to_lowercase();
    let focus_terms = tokenized_terms(focus);
    let mut ranked = units
        .iter()
        .enumerate()
        .filter_map(|(index, chunk)| {
            let score = score_chunk(chunk.as_str(), &focus_lower, &focus_terms);
            (score > 0).then_some((index, score))
        })
        .collect::<Vec<_>>();
    if ranked.is_empty() {
        return None;
    }

    ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));

    let mut selected = Vec::new();
    let mut used_indices = HashSet::new();
    for (index, _) in ranked {
        if !used_indices.insert(index) {
            continue;
        }

        let mut block = units[index].clone();
        if let Some(next) = units.get(index + 1)
            && !used_indices.contains(&(index + 1))
            && block.len() + next.len() < TARGET_CHUNK_BYTES
        {
            block.push('\n');
            block.push_str(next);
            used_indices.insert(index + 1);
        }

        selected.push((index, block));
        if selected.len() == MAX_EXCERPTS {
            break;
        }
    }
    selected.sort_by_key(|(index, ..)| *index);

    let mut excerpt = String::new();
    for (index, chunk) in selected {
        let separator = if index == 0 || excerpt.is_empty() {
            ""
        } else {
            "\n\n...\n\n"
        };
        let remaining = max_bytes.saturating_sub(excerpt.len());
        if remaining == 0 {
            break;
        }
        let addition = format!("{separator}{chunk}");
        excerpt.push_str(take_bytes_at_char_boundary(addition.as_str(), remaining));
    }

    let excerpt = excerpt.trim().to_string();
    (!excerpt.is_empty()).then_some(excerpt)
}

fn distilled_keywords(query: &str) -> Vec<String> {
    let mut seen = HashSet::new();

    tokenized_terms(query)
        .into_iter()
        .filter(|term| {
            (is_cjk_term(term) || term.chars().count() > 2)
                && !is_stopword(term)
                && seen.insert(term.clone())
        })
        .take(MAX_KEYWORD_TERMS)
        .collect()
}

fn github_repo_slug_from_query(query: &str) -> Option<(String, String)> {
    let query_lower = query.to_ascii_lowercase();
    let has_repo_hint = query_lower.contains("github")
        || query_lower.contains("repo")
        || query_lower.contains("repository");
    if !has_repo_hint {
        return None;
    }

    github_repo_slug_regex()
        .captures_iter(query)
        .find_map(|captures| {
            let owner = captures.get(1)?.as_str();
            let repo = captures.get(2)?.as_str().trim_end_matches(".git");
            if owner.eq_ignore_ascii_case("github") || repo.eq_ignore_ascii_case("github") {
                return None;
            }
            Some((owner.to_string(), repo.to_string()))
        })
}

fn github_repo_slug_regex() -> &'static regex_lite::Regex {
    static RE: OnceLock<regex_lite::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex_lite::Regex::new(
            r"(?i)\b([a-z0-9](?:[a-z0-9_.-]{0,99}[a-z0-9])?)/([a-z0-9](?:[a-z0-9_.-]{0,99}[a-z0-9])?(?:\.git)?)\b",
        )
        .unwrap_or_else(|err| panic!("invalid GitHub repo slug regex: {err}"))
    })
}

fn tokenized_terms(text: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        if is_term_char(ch) {
            current.extend(ch.to_lowercase());
        } else if !current.is_empty() {
            terms.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        terms.push(current);
    }

    terms
}

fn is_term_char(ch: char) -> bool {
    ch.is_alphanumeric() || is_cjk(ch)
}

fn is_cjk_term(term: &str) -> bool {
    term.chars().all(is_cjk)
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0x3040..=0x30FF
            | 0xAC00..=0xD7AF
    )
}

fn is_stopword(term: &str) -> bool {
    matches!(
        term,
        "a" | "an"
            | "and"
            | "are"
            | "for"
            | "from"
            | "how"
            | "in"
            | "into"
            | "latest"
            | "of"
            | "on"
            | "or"
            | "the"
            | "to"
            | "use"
            | "using"
            | "with"
            | "怎么"
            | "如何"
            | "使用"
            | "以及"
            | "最新"
            | "最好"
            | "有关"
            | "关于"
            | "和"
            | "或"
    )
}

fn normalize_url(raw_url: &str) -> Option<String> {
    let mut url = Url::parse(raw_url.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }

    let host = url
        .host_str()?
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    url.set_host(Some(host.as_str())).ok()?;

    if (url.scheme() == "http" && url.port() == Some(80))
        || (url.scheme() == "https" && url.port() == Some(443))
    {
        url.set_port(None).ok()?;
    }

    let normalized_path = url
        .path()
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("/");
    if normalized_path.is_empty() {
        url.set_path("/");
    } else {
        url.set_path(&format!("/{normalized_path}"));
    }

    let mut query_pairs = url
        .query_pairs()
        .filter(|(key, _)| !is_tracking_query_param(key.as_ref()))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    query_pairs.sort();

    if query_pairs.is_empty() {
        url.set_query(None);
    } else {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (key, value) in query_pairs {
            serializer.append_pair(key.as_str(), value.as_str());
        }
        url.set_query(Some(serializer.finish().as_str()));
    }

    url.set_fragment(None);

    let normalized = url.to_string();
    Some(
        normalized
            .trim_end_matches('/')
            .to_string()
            .replace(":///", "://"),
    )
}

fn is_tracking_query_param(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.starts_with("utm_")
        || matches!(
            key.as_str(),
            "aspsessionid"
                | "campaign"
                | "cid"
                | "content"
                | "fbclid"
                | "gclid"
                | "jsessionid"
                | "mcid"
                | "medium"
                | "phpsessid"
                | "ref"
                | "referrer"
                | "s"
                | "sc_rid"
                | "session"
                | "sessionid"
                | "sid"
                | "source"
                | "term"
        )
}

fn normalized_host(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()?
        .host_str()
        .map(|host| host.trim_start_matches("www.").to_ascii_lowercase())
}

fn score_search_result(query_lower: &str, query_terms: &[String], result: &SearchResult) -> i32 {
    let title_lower = result.title.to_lowercase();
    let snippet_lower = result.snippet.to_lowercase();
    let url_lower = result.url.to_lowercase();
    let mut score = source_quality_boost(result.url.as_str());

    if !query_lower.is_empty() {
        if title_lower.contains(query_lower) {
            score += 15;
        }
        if snippet_lower.contains(query_lower) {
            score += 10;
        }
    }

    let mut distinct_matches = 0;
    for term in query_terms {
        if title_lower.contains(term.as_str()) {
            score += 5;
            distinct_matches += 1;
        } else if snippet_lower.contains(term.as_str()) {
            score += 3;
            distinct_matches += 1;
        } else if url_lower.contains(term.as_str()) {
            score += 1;
            distinct_matches += 1;
        }
    }

    score + distinct_matches * 2 + i32::from(!result.snippet.is_empty())
}

fn source_quality_boost(url: &str) -> i32 {
    let Ok(parsed) = Url::parse(url) else {
        return 0;
    };
    let host = parsed
        .host_str()
        .map(|host| host.trim_start_matches("www.").to_ascii_lowercase())
        .unwrap_or_default();
    let path = parsed.path().to_ascii_lowercase();

    let mut score = 0;
    if host == "github.com" {
        score += 3;
        if path.contains("/blob/") {
            score += 4;
        }
        if path.contains("/releases") {
            score += 4;
        }
        if looks_like_github_repo_root(path.as_str()) {
            score += 3;
        }
    }
    if host.starts_with("docs.")
        || host.starts_with("developer.")
        || matches!(
            host.as_str(),
            "developers.openai.com" | "docs.github.com" | "platform.openai.com"
        )
    {
        score += 5;
    }
    if path.starts_with("/docs") || path.contains("/reference") || path.contains("/api") {
        score += 4;
    }
    if path.contains("/guides") || path.contains("/manual") || path.contains("/usage") {
        score += 2;
    }
    if path.contains("/blog") || path.contains("/news") || path.contains("/changelog") {
        score -= 2;
    }
    score
}

fn looks_like_github_repo_root(path: &str) -> bool {
    let segments = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    matches!(segments.as_slice(), [_, _] | [_, _, "tree", ..])
}

fn split_text_into_units(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .flat_map(split_line_into_units)
        .collect()
}

fn split_line_into_units(line: &str) -> Vec<String> {
    if line.len() <= TARGET_CHUNK_BYTES / 2 {
        return vec![line.to_string()];
    }

    let mut units = Vec::new();
    let mut current = String::new();

    for ch in line.chars() {
        current.push(ch);
        let should_cut = matches!(ch, '.' | '!' | '?' | ';' | '。' | '！' | '？' | '；')
            || current.len() >= TARGET_CHUNK_BYTES / 2;
        if should_cut {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                units.push(trimmed.to_string());
            }
            current.clear();
        }
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        units.push(trimmed.to_string());
    }

    if units.is_empty() {
        vec![line.to_string()]
    } else {
        units
    }
}

fn score_chunk(chunk: &str, focus_lower: &str, focus_terms: &[String]) -> i32 {
    let chunk_lower = chunk.to_lowercase();
    let chunk_terms = tokenized_terms(chunk);
    let mut score = 0;

    if chunk_lower.contains(focus_lower) {
        score += 20;
    }

    let matched_terms = focus_terms
        .iter()
        .filter(|term| {
            chunk_lower.contains(term.as_str())
                || chunk_terms
                    .iter()
                    .any(|chunk_term| terms_match(chunk_term, term))
        })
        .count() as i32;
    if matched_terms == 0 && !chunk_lower.contains(focus_lower) {
        return 0;
    }

    score + matched_terms * 6
}

fn terms_match(chunk_term: &str, focus_term: &str) -> bool {
    if chunk_term == focus_term {
        return true;
    }

    let min_shared_chars = if is_cjk_term(chunk_term) || is_cjk_term(focus_term) {
        2
    } else {
        4
    };

    let chunk_len = chunk_term.chars().count();
    let focus_len = focus_term.chars().count();

    (chunk_term.starts_with(focus_term) && focus_len >= min_shared_chars)
        || (focus_term.starts_with(chunk_term) && chunk_len >= min_shared_chars)
}

struct RankedSearchResult {
    score: i32,
    result: SearchResult,
}

impl RankedSearchResult {
    fn new(result: SearchResult, score: i32) -> Self {
        Self { score, result }
    }

    fn merge(&mut self, other: Self) {
        if other.score > self.score
            || (other.score == self.score && other.result.snippet.len() > self.result.snippet.len())
        {
            let fallback_snippet = self.result.snippet.clone();
            *self = other;
            if self.result.snippet.is_empty() {
                self.result.snippet = fallback_snippet;
            }
            return;
        }

        if self.result.snippet.is_empty() && !other.result.snippet.is_empty() {
            self.result.snippet = other.result.snippet;
        }
    }
}
