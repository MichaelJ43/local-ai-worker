fn main() {
    println!("cargo:rerun-if-env-changed=LOCAL_AI_DEFAULT_WORKER_AGENT_IMAGE");
    let image = std::env::var("LOCAL_AI_DEFAULT_WORKER_AGENT_IMAGE")
        .unwrap_or_else(|_| "local-ai-worker-agent:latest".into());
    let image = image.trim().to_string();
    if image.is_empty() {
        panic!("LOCAL_AI_DEFAULT_WORKER_AGENT_IMAGE, if set, must not be empty");
    }
    println!("cargo:rustc-env=DEFAULT_WORKER_AGENT_IMAGE={image}");
    tauri_build::build()
}
