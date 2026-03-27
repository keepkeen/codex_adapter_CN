use super::FetchedPage;
use super::SearchResult;
use super::clean_duckduckgo_url;
use super::find_in_page;
use super::format_open_page;
use super::format_search_results;
use super::html_page_to_text;
use super::parse_search_results;
use super::processing::expand_search_queries;
use super::processing::extract_focused_excerpt;
use super::processing::fallback_search_results;
use super::processing::rerank_search_results;
use super::sources::build_fetch_plan;
use super::sources::extract_page;
use super::sources::github_blob_raw_url;
use pretty_assertions::assert_eq;
use url::Url;

const SEARCH_RESULTS_HTML: &str = r#"
<div class="serp__results">
  <div class="result results_links results_links_deep web-result">
    <div class="links_main links_deep result__body">
      <h2 class="result__title">
        <a rel="nofollow" class="result__a" href="https://github.com/openai/codex">
          GitHub - openai/codex: Lightweight coding agent
        </a>
      </h2>
      <div class="result__extras">
        <a class="result__url" href="https://github.com/openai/codex">github.com/openai/codex</a>
      </div>
      <a class="result__snippet" href="https://github.com/openai/codex">
        <b>Codex</b> CLI is a coding agent from <b>OpenAI</b> that runs locally.
      </a>
    </div>
  </div>
  <div class="result results_links results_links_deep web-result">
    <div class="links_main links_deep result__body">
      <h2 class="result__title">
        <a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fdevelopers.openai.com%2Fcodex">
          Use Codex in GitHub | OpenAI Developers
        </a>
      </h2>
      <div class="result__snippet">
        Learn how to use Codex in GitHub and connect it to your repository.
      </div>
    </div>
  </div>
</div>
"#;

const PAGE_HTML: &str = r#"
<!DOCTYPE html>
<html>
  <head>
    <title>Example &amp; Test</title>
    <style>.hidden { display: none; }</style>
    <script>console.log("ignore me");</script>
  </head>
  <body>
    <h1>Heading</h1>
    <p>First paragraph with <b>important</b> details.</p>
    <div>Second line</div>
    <p>Pattern match here</p>
  </body>
</html>
"#;

const GITHUB_REPO_HTML: &str = r#"
<!DOCTYPE html>
<html>
  <body>
    <nav>Repository navigation</nav>
    <article class="markdown-body entry-content container-lg">
      <h1>README</h1>
      <p>Install codex-cn with cargo build --release.</p>
      <p>Use DeepSeek via the isolated CODEX_HOME wrapper.</p>
    </article>
    <aside>Stars and forks</aside>
  </body>
</html>
"#;

const DOCS_HTML: &str = r#"
<!DOCTYPE html>
<html>
  <body>
    <aside class="sidebar">Sidebar navigation</aside>
    <main class="theme-doc-markdown markdown">
      <article class="theme-doc-markdown markdown">
        <h1>Installation</h1>
        <p>Run cargo build --release -p codex-cli.</p>
        <pre><code>export DEEPSEEK_API_KEY=sk-test</code></pre>
      </article>
    </main>
    <footer>Footer links</footer>
  </body>
</html>
"#;

#[test]
fn parse_search_results_extracts_titles_urls_and_snippets() {
    let results = parse_search_results(SEARCH_RESULTS_HTML, 10);

    assert_eq!(
        results,
        vec![
            SearchResult {
                title: "GitHub - openai/codex: Lightweight coding agent".to_string(),
                url: "https://github.com/openai/codex".to_string(),
                snippet: "Codex CLI is a coding agent from OpenAI that runs locally.".to_string(),
            },
            SearchResult {
                title: "Use Codex in GitHub | OpenAI Developers".to_string(),
                url: "https://developers.openai.com/codex".to_string(),
                snippet: "Learn how to use Codex in GitHub and connect it to your repository."
                    .to_string(),
            },
        ]
    );
}

#[test]
fn clean_duckduckgo_url_decodes_redirect_targets() {
    assert_eq!(
        clean_duckduckgo_url(
            "//duckduckgo.com/l/?uddg=https%3A%2F%2Fdevelopers.openai.com%2Fcodex&rut=abc"
        ),
        "https://developers.openai.com/codex"
    );
}

#[test]
fn html_page_to_text_drops_tags_scripts_and_styles() {
    assert_eq!(
        html_page_to_text(PAGE_HTML),
        "Example & Test\nHeading\nFirst paragraph with important details.\nSecond line\nPattern match here"
    );
}

#[test]
fn github_blob_raw_url_appends_raw_query_parameter() {
    let raw_url = github_blob_raw_url(
        &Url::parse("https://github.com/openai/codex/blob/main/README.md#readme")
            .expect("parse GitHub blob URL"),
    );

    assert_eq!(
        raw_url,
        Some("https://github.com/openai/codex/blob/main/README.md?raw=1".to_string()),
    );
}

#[test]
fn find_in_page_returns_matching_lines() {
    assert_eq!(
        find_in_page(html_page_to_text(PAGE_HTML).as_str(), "pattern"),
        vec!["Pattern match here".to_string()]
    );
}

#[test]
fn format_search_results_mentions_allowed_domains_when_present() {
    let rendered = format_search_results(
        "codex",
        &[SearchResult {
            title: "openai/codex".to_string(),
            url: "https://github.com/openai/codex".to_string(),
            snippet: "Repository".to_string(),
        }],
        Some(&["github.com".to_string()]),
    );

    assert_eq!(
        rendered,
        "Search results for `codex`:\nAllowed domains: github.com\n\n1. openai/codex\n   URL: https://github.com/openai/codex\n   Snippet: Repository\n\n"
    );
}

#[test]
fn expand_search_queries_adds_keyword_variant_for_long_query() {
    assert_eq!(
        expand_search_queries("how to use codex with deepseek api and search tools"),
        vec![
            "how to use codex with deepseek api and search tools".to_string(),
            "codex deepseek api search tools".to_string(),
        ]
    );
}

#[test]
fn fallback_search_results_infers_github_repo_from_query() {
    assert_eq!(
        fallback_search_results("openai/codex GitHub repository", 5),
        vec![SearchResult {
            title: "GitHub - openai/codex".to_string(),
            url: "https://github.com/openai/codex".to_string(),
            snippet: "Canonical GitHub repository inferred directly from the query.".to_string(),
        }]
    );
}

#[test]
fn rerank_search_results_deduplicates_urls_and_spreads_hosts() {
    let ranked = rerank_search_results(
        "codex deepseek api",
        vec![
            SearchResult {
                title: "Codex DeepSeek API".to_string(),
                url: "https://www.example.com/docs/codex?utm_source=newsletter".to_string(),
                snippet: "DeepSeek API reference for Codex.".to_string(),
            },
            SearchResult {
                title: "Duplicate tracking URL".to_string(),
                url: "https://example.com/docs/codex#section".to_string(),
                snippet: "".to_string(),
            },
            SearchResult {
                title: "Example blog".to_string(),
                url: "https://example.com/blog/deepseek".to_string(),
                snippet: "A blog post.".to_string(),
            },
            SearchResult {
                title: "Official docs".to_string(),
                url: "https://docs.example.org/codex/deepseek".to_string(),
                snippet: "Official Codex and DeepSeek integration guide.".to_string(),
            },
            SearchResult {
                title: "GitHub".to_string(),
                url: "https://github.com/openai/codex".to_string(),
                snippet: "Repository".to_string(),
            },
        ],
        3,
    );

    assert_eq!(
        ranked,
        vec![
            SearchResult {
                title: "Codex DeepSeek API".to_string(),
                url: "https://www.example.com/docs/codex?utm_source=newsletter".to_string(),
                snippet: "DeepSeek API reference for Codex.".to_string(),
            },
            SearchResult {
                title: "Official docs".to_string(),
                url: "https://docs.example.org/codex/deepseek".to_string(),
                snippet: "Official Codex and DeepSeek integration guide.".to_string(),
            },
            SearchResult {
                title: "GitHub".to_string(),
                url: "https://github.com/openai/codex".to_string(),
                snippet: "Repository".to_string(),
            },
        ]
    );
}

#[test]
fn rerank_search_results_prefers_docs_and_github_reference_pages() {
    let ranked = rerank_search_results(
        "codex api reference",
        vec![
            SearchResult {
                title: "Blog post".to_string(),
                url: "https://example.com/blog/codex-api".to_string(),
                snippet: "Third-party summary.".to_string(),
            },
            SearchResult {
                title: "API reference".to_string(),
                url: "https://docs.example.com/reference/codex-api".to_string(),
                snippet: "Official API reference.".to_string(),
            },
            SearchResult {
                title: "README".to_string(),
                url: "https://github.com/openai/codex".to_string(),
                snippet: "Project repository.".to_string(),
            },
        ],
        3,
    );

    assert_eq!(
        ranked,
        vec![
            SearchResult {
                title: "API reference".to_string(),
                url: "https://docs.example.com/reference/codex-api".to_string(),
                snippet: "Official API reference.".to_string(),
            },
            SearchResult {
                title: "README".to_string(),
                url: "https://github.com/openai/codex".to_string(),
                snippet: "Project repository.".to_string(),
            },
            SearchResult {
                title: "Blog post".to_string(),
                url: "https://example.com/blog/codex-api".to_string(),
                snippet: "Third-party summary.".to_string(),
            },
        ]
    );
}

#[test]
fn extract_focused_excerpt_prefers_relevant_chunks() {
    let text = "\
Intro paragraph about the project history.
General notes that do not matter much here.
DeepSeek API setup requires setting DEEPSEEK_API_KEY and model deepseek-chat.
Tool calls are serialized through the local chat completions adapter.
Another unrelated paragraph about release notes.
Search quality improves after deduplicating URLs and extracting focused snippets.";

    assert_eq!(
        extract_focused_excerpt(text, "DeepSeek API setup", 160),
        Some(
            "DeepSeek API setup requires setting DEEPSEEK_API_KEY and model deepseek-chat.\nTool calls are serialized through the local chat completions adapter."
                .to_string(),
        )
    );
}

#[test]
fn extract_focused_excerpt_matches_installation_focus_to_install_lines() {
    let text = "\
Overview of the repository.
Install using npm install -g @openai/codex.
Homebrew users can run brew install --cask codex.
Additional release notes.";

    assert_eq!(
        extract_focused_excerpt(text, "installation instructions", 200),
        Some(
            "Install using npm install -g @openai/codex.\nHomebrew users can run brew install --cask codex."
                .to_string(),
        )
    );
}

#[test]
fn format_open_page_uses_focus_excerpt_when_present() {
    let page = FetchedPage {
        final_url: "https://example.com/docs".to_string(),
        title: Some("Example Docs".to_string()),
        content_type: "text/html".to_string(),
        text: "\
Intro text.
DeepSeek adapter setup requires configuring model_provider and model.
Search requests are deduplicated before results are returned.
Closing paragraph."
            .to_string(),
        truncated: false,
        implicit_focus: None,
    };

    assert_eq!(
        format_open_page(&page, Some("DeepSeek adapter setup")),
        "Opened page: https://example.com/docs\nTitle: Example Docs\nContent-Type: text/html\n\nFocused excerpts for `DeepSeek adapter setup`:\nDeepSeek adapter setup requires configuring model_provider and model.\nSearch requests are deduplicated before results are returned.\n\n[truncated]"
    );
}

#[test]
fn format_open_page_uses_fragment_focus_when_explicit_focus_is_absent() {
    let page = FetchedPage {
        final_url: "https://example.com/docs#installation".to_string(),
        title: Some("Example Docs".to_string()),
        content_type: "text/html".to_string(),
        text: "\
Overview.
Install codex-cn with cargo build --release.
Set DEEPSEEK_API_KEY before launching the wrapper.
Other notes."
            .to_string(),
        truncated: false,
        implicit_focus: Some("installation instructions".to_string()),
    };

    assert_eq!(
        format_open_page(&page, None),
        "Opened page: https://example.com/docs#installation\nTitle: Example Docs\nContent-Type: text/html\n\nFocused excerpts for `installation instructions`:\nInstall codex-cn with cargo build --release.\nSet DEEPSEEK_API_KEY before launching the wrapper.\n\n[truncated]"
    );
}

#[test]
fn extract_page_prefers_github_markdown_body_and_fragment_focus() {
    let fetch_plan = build_fetch_plan("https://github.com/openai/codex#installation")
        .expect("build GitHub fetch plan");
    let page = extract_page(
        &fetch_plan,
        &Url::parse("https://github.com/openai/codex").expect("parse response URL"),
        "text/html",
        GITHUB_REPO_HTML,
        false,
    );

    assert_eq!(page.implicit_focus, Some("installation".to_string()));
    assert_eq!(
        page.text,
        "README\nInstall codex-cn with cargo build --release.\nUse DeepSeek via the isolated CODEX_HOME wrapper."
    );
}

#[test]
fn extract_page_prefers_docs_article_and_strips_sidebar() {
    let fetch_plan =
        build_fetch_plan("https://docs.example.com/install").expect("build docs fetch plan");
    let page = extract_page(
        &fetch_plan,
        &Url::parse("https://docs.example.com/install").expect("parse response URL"),
        "text/html",
        DOCS_HTML,
        false,
    );

    assert_eq!(
        page.text,
        "Installation\nRun cargo build --release -p codex-cli.\nCode sample:\nexport DEEPSEEK_API_KEY=sk-test"
    );
}
