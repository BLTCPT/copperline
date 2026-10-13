use std::{collections::BTreeMap, path::Path, process::Command};
fn main() {
    if let Err(e) = run() {
        eprintln!("quality gate: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let task = std::env::args().nth(1).unwrap_or_else(|| "ci".into());
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Workspace path missing")?;
    std::env::set_current_dir(root)?;
    match task.as_str() {
        "ci" => {
            layers()?;
            assets()?;
            cmd("cargo", &["fmt", "--all", "--", "--check"])?;
            cmd(
                "cargo",
                &[
                    "clippy",
                    "--workspace",
                    "--all-targets",
                    "--locked",
                    "--",
                    "-D",
                    "warnings",
                ],
            )?;
            cmd("cargo", &["test", "--workspace", "--locked"])?;
            println!("All quality gates passed");
        }
        "layers" => layers()?,
        "assets" => assets()?,
        "licenses" => licenses()?,
        _ => return Err("Use ci, layers, assets, or licenses".into()),
    }
    Ok(())
}
fn cmd(program: &str, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new(program).args(args).status()?;
    if !status.success() {
        return Err(format!("{program} {} failed: {status}", args.join(" ")).into());
    }
    Ok(())
}
fn metadata() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let result = Command::new("cargo")
        .args(["metadata", "--locked", "--format-version", "1"])
        .output()?;
    if !result.status.success() {
        return Err(String::from_utf8_lossy(&result.stderr).into_owned().into());
    }
    Ok(serde_json::from_slice(&result.stdout)?)
}
fn layers() -> Result<(), Box<dyn std::error::Error>> {
    let m = metadata()?;
    let levels: BTreeMap<_, _> = [
        ("copperline-money", 0),
        ("copperline-model", 1),
        ("copperline-pricing", 2),
        ("copperline-storage", 2),
        ("copperline-engine", 3),
        ("copperline-ui", 4),
        ("copperline-cli", 5),
        ("copperline-desktop", 5),
        ("xtask", 5),
    ]
    .into_iter()
    .collect();
    for p in m["packages"].as_array().ok_or("No packages")? {
        let name = p["name"].as_str().ok_or("No name")?;
        let Some(level) = levels.get(name) else {
            continue;
        };
        for d in p["dependencies"].as_array().ok_or("No dependency list")? {
            let dep = d["name"].as_str().ok_or("No dependency name")?;
            if let Some(other) = levels.get(dep)
                && other >= level
            {
                return Err(format!("Layer violation: {name} → {dep}").into());
            }
            if *level < 4 && ["egui", "eframe", "winit", "rfd"].contains(&dep) {
                return Err(
                    format!("UI dependency below presentation layer: {name} → {dep}").into(),
                );
            }
            if *level < 2 && ["copperline-storage", "tokio", "reqwest"].contains(&dep) {
                return Err(format!("IO dependency in deterministic model: {name} → {dep}").into());
            }
        }
    }
    println!("Layer gate passed: exact money → model → pricing/storage → engine → UI → apps");
    Ok(())
}
fn assets() -> Result<(), Box<dyn std::error::Error>> {
    let attribution = std::fs::read_to_string("ATTRIBUTION.md")?;
    fn visit(path: &Path, text: &str) -> Result<(), Box<dyn std::error::Error>> {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let p = entry.path();
            if p.is_dir() {
                if !["target", ".git", "data", "dist"]
                    .contains(&entry.file_name().to_string_lossy().as_ref())
                {
                    visit(&p, text)?;
                }
            } else if p
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| ["png", "jpg", "svg", "ttf", "woff", "ico"].contains(&s))
            {
                let name = p.file_name().ok_or("Asset name")?.to_string_lossy();
                if !text.contains(name.as_ref()) {
                    return Err(format!("Unattributed asset: {}", p.display()).into());
                }
            }
        }
        Ok(())
    }
    visit(Path::new("."), &attribution)?;
    println!("Asset attribution gate passed");
    Ok(())
}
fn licenses() -> Result<(), Box<dyn std::error::Error>> {
    let m = metadata()?;
    let mut lines = vec!["name\tversion\tlicense\trepository".to_string()];
    for p in m["packages"].as_array().ok_or("No packages")? {
        lines.push(format!(
            "{}\t{}\t{}\t{}",
            p["name"].as_str().unwrap_or("?"),
            p["version"].as_str().unwrap_or("?"),
            p["license"].as_str().unwrap_or("REVIEW REQUIRED"),
            p["repository"].as_str().unwrap_or("local")
        ));
    }
    lines[1..].sort();
    std::fs::write(
        "docs/dependency-license-inventory.tsv",
        lines.join("\n") + "\n",
    )?;
    println!("Wrote dependency license inventory");
    Ok(())
}
