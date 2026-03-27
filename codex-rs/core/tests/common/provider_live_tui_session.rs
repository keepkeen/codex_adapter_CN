use std::path::PathBuf;

use serde_json::Value;

pub(crate) fn session_log_contains_compact_op(text: &str) -> bool {
    text.lines().filter_map(parse_json_value).any(|line| {
        line.get("kind").and_then(Value::as_str) == Some("op")
            && line.pointer("/payload/type").and_then(Value::as_str) == Some("compact")
    })
}

pub(crate) fn session_log_contains_context_compacted_event(text: &str) -> bool {
    text.lines().filter_map(parse_json_value).any(|line| {
        if line.get("kind").and_then(Value::as_str) != Some("codex_event") {
            return false;
        }
        matches!(
            line.pointer("/payload/msg/type").and_then(Value::as_str),
            Some("context_compacted")
        ) || line.pointer("/payload/type").and_then(Value::as_str) == Some("context_compacted")
    })
}

pub(crate) fn session_log_contains_startup_ready(text: &str) -> bool {
    let mut saw_session_configured = false;
    let mut saw_list_skills_response = false;

    for line in text.lines().filter_map(parse_json_value) {
        if line.get("kind").and_then(Value::as_str) != Some("codex_event") {
            continue;
        }

        match line.pointer("/payload/msg/type").and_then(Value::as_str) {
            Some("session_configured") => saw_session_configured = true,
            Some("list_skills_response") => saw_list_skills_response = true,
            _ => {}
        }
    }

    saw_session_configured && saw_list_skills_response
}

pub(crate) fn session_log_contains_app_server_startup_ready(text: &str) -> bool {
    let mut saw_session_start = false;
    let mut saw_history_cell = false;
    let mut saw_list_skills_op = false;

    for line in text.lines().filter_map(parse_json_value) {
        match line.get("kind").and_then(Value::as_str) {
            Some("session_start") => saw_session_start = true,
            Some("insert_history_cell") => saw_history_cell = true,
            Some("op")
                if line.pointer("/payload/type").and_then(Value::as_str) == Some("list_skills") =>
            {
                saw_list_skills_op = true;
            }
            _ => {}
        }
    }

    saw_session_start && saw_history_cell && saw_list_skills_op
}

pub(crate) fn session_log_rollout_path(text: &str) -> Option<PathBuf> {
    text.lines().filter_map(parse_json_value).find_map(|line| {
        if line.get("kind").and_then(Value::as_str) != Some("codex_event") {
            return None;
        }
        if line.pointer("/payload/msg/type").and_then(Value::as_str) != Some("session_configured") {
            return None;
        }
        line.pointer("/payload/msg/rollout_path")
            .and_then(Value::as_str)
            .or_else(|| {
                line.pointer("/payload/rollout_path")
                    .and_then(Value::as_str)
            })
            .map(PathBuf::from)
    })
}

fn parse_json_value(line: &str) -> Option<Value> {
    serde_json::from_str(line).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_helpers_match_expected_payloads() {
        let text = concat!(
            "{\"kind\":\"op\",\"payload\":{\"type\":\"compact\"}}\n",
            "{\"kind\":\"codex_event\",\"payload\":{\"msg\":{\"type\":\"context_compacted\"}}}\n"
        );

        assert!(session_log_contains_compact_op(text));
        assert!(session_log_contains_context_compacted_event(text));
    }

    #[test]
    fn startup_ready_requires_session_and_skills() {
        let text = concat!(
            "{\"kind\":\"codex_event\",\"payload\":{\"msg\":{\"type\":\"session_configured\"}}}\n",
            "{\"kind\":\"codex_event\",\"payload\":{\"msg\":{\"type\":\"list_skills_response\"}}}\n"
        );

        assert!(session_log_contains_startup_ready(text));
        assert!(!session_log_contains_startup_ready(
            "{\"kind\":\"codex_event\",\"payload\":{\"msg\":{\"type\":\"session_configured\"}}}\n"
        ));
    }

    #[test]
    fn app_server_startup_ready_uses_session_start_history_and_skill_op() {
        let text = concat!(
            "{\"dir\":\"meta\",\"kind\":\"session_start\"}\n",
            "{\"dir\":\"to_tui\",\"kind\":\"insert_history_cell\"}\n",
            "{\"dir\":\"from_tui\",\"kind\":\"op\",\"payload\":{\"type\":\"list_skills\",\"force_reload\":true}}\n"
        );

        assert!(session_log_contains_app_server_startup_ready(text));
        assert!(!session_log_contains_app_server_startup_ready(
            "{\"dir\":\"meta\",\"kind\":\"session_start\"}\n{\"dir\":\"to_tui\",\"kind\":\"insert_history_cell\"}\n"
        ));
    }

    #[test]
    fn rollout_path_comes_from_session_configured() {
        let text = concat!(
            "{\"kind\":\"codex_event\",\"payload\":{\"msg\":{\"type\":\"session_configured\",\"rollout_path\":\"/tmp/current-rollout.jsonl\"}}}\n",
            "{\"kind\":\"codex_event\",\"payload\":{\"msg\":{\"type\":\"list_skills_response\"}}}\n"
        );

        assert_eq!(
            session_log_rollout_path(text),
            Some(PathBuf::from("/tmp/current-rollout.jsonl"))
        );
    }
}
