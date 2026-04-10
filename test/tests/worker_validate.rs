use ai_worker_core::worker_config::WorkerDefinition;

#[test]
fn git_allowlist_enforced_empty_fails() {
    let w = WorkerDefinition {
        id: "a".into(),
        name: "n".into(),
        maintenance_domain: "git".into(),
        model_override: None,
        ollama_host: None,
        enabled: true,
        tasks: vec![],
        guardrail_overrides: Some(serde_json::json!({
            "scope": { "enforceRepositoryAllowlist": true, "allowedRepositories": [] }
        })),
        context_path: None,
        long_term_volume: None,
        docker_image: None,
    };
    assert!(w.validate().is_err());
}
