//! Runs inside the worker Linux image: wraps `git` / `gh` with guardrails.

use ai_worker_core::guard_exec::EffectiveGitGuardrails;
use std::process::Command;

fn guardrails() -> EffectiveGitGuardrails {
    let path = std::env::var("GUARDRAILS_PATH")
        .unwrap_or_else(|_| "/workspace/guardrails.effective.json".into());
    EffectiveGitGuardrails::load_path(&path).unwrap_or_default()
}

fn real_git() -> String {
    std::env::var("REAL_GIT").unwrap_or_else(|_| "/usr/bin/git".into())
}

fn real_gh() -> String {
    std::env::var("REAL_GH").unwrap_or_else(|_| "/usr/bin/gh".into())
}

fn collect_after_double_dash() -> (String, Vec<String>) {
    let mut it = std::env::args().skip(1);
    let mode = it.next().unwrap_or_default();
    let mut rest = vec![];
    let mut after = false;
    for a in it {
        if after {
            rest.push(a);
        } else if a == "--" {
            after = true;
        }
    }
    (mode, rest)
}

fn exit_code(st: std::io::Result<std::process::ExitStatus>) -> i32 {
    match st {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => {
            eprintln!("exec: {e}");
            127
        }
    }
}

fn main() {
    let (mode, args) = collect_after_double_dash();
    let gr = guardrails();

    let code = match mode.as_str() {
        "run-git" => {
            if let Err(e) = ai_worker_core::guard_exec::git_precheck(&args, &gr) {
                eprintln!("{e}");
                1
            } else {
                let st = Command::new(real_git()).args(&args).status();
                let c = exit_code(st);
                ai_worker_core::guard_exec::git_postcheck(&args, c, &gr);
                c
            }
        }
        "run-gh" => {
            if let Err(e) = ai_worker_core::guard_exec::gh_precheck(&args, &gr) {
                eprintln!("{e}");
                1
            } else {
                let st = Command::new(real_gh()).args(&args).status();
                let c = exit_code(st);
                ai_worker_core::guard_exec::gh_postcheck(&args, c, &gr);
                c
            }
        }
        _ => {
            eprintln!("usage: worker-guard run-git -- <git args>");
            eprintln!("       worker-guard run-gh -- <gh args>");
            2
        }
    };
    std::process::exit(code);
}
