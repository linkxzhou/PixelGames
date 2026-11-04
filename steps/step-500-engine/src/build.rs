use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use pxl_contract::error::{HarnessError, Result};
use pxl_contract::{
    DigestEntry, GateCheck, GateResult, Gates, Scope, StepEnvelope, StepStatus, Verdict,
};
use pxl_store::digest::{digest_bytes, digest_file};

use crate::check::write_json;
use crate::{BuildArgs, STEP_ID, VERSION};

pub fn build(args: BuildArgs) -> Result<i32> {
    if args.mode() == "rust" {
        return Err(HarnessError::usage(
            "step-500 build handles the JS static bundle; use the existing cargo runtime for Rust mode",
        )
        .with_hint("cargo build -p battle-city-runtime; see libs/pxl-agent/skills/macroquad/SKILL.md"));
    }
    if args.out.exists() && !args.overwrite {
        return Err(
            HarnessError::exists(format!("output {} already exists", args.out.display()))
                .with_hint("choose another --out or pass --overwrite"),
        );
    }
    let rejected = rejected_path(&args.out);
    if rejected.exists() {
        fs::remove_dir_all(&rejected).map_err(io("cannot clear rejected output"))?;
    }
    fs::create_dir_all(&rejected).map_err(io("cannot create rejected output"))?;
    let result = build_staging(&args, &rejected);
    match result {
        Ok(()) => {
            if args.out.exists() {
                fs::remove_dir_all(&args.out).map_err(io("cannot replace existing output"))?;
            }
            if let Some(parent) = args.out.parent() {
                fs::create_dir_all(parent).map_err(io("cannot create output parent"))?;
            }
            fs::rename(&rejected, &args.out).map_err(io("cannot publish engine build"))?;
            let manifest = args.out.join("engine-build.json");
            println!(
                "{}",
                serde_json::to_string(&json!({
                    "ok": true,
                    "cmd": "step-500 build",
                    "mode": "js",
                    "files": count_files(&args.out.join("web"))?,
                    "manifest": digest_file(&manifest)?,
                    "bytes": directory_bytes(&args.out.join("web"))?,
                }))?
            );
            Ok(0)
        }
        Err(error) => Err(error),
    }
}

fn build_staging(args: &BuildArgs, staging: &Path) -> Result<()> {
    let web = staging.join("web");
    fs::create_dir_all(&web).map_err(io("cannot create web output"))?;
    let runtime = args.runtime_dir.clone().unwrap_or_else(default_runtime);
    sync_runtime(&runtime, &web)?;
    if let Some(game) = &args.game {
        sync_game(game, &web)?;
    } else {
        sync_showcase(&runtime, &web)?;
    }
    let synced = sync_assets(&args.input, &web.join("assets"))?;
    let mut files = Vec::new();
    collect_files(&web, &web, &mut files)?;
    files.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    let total_bytes = files
        .iter()
        .map(|file| file["bytes"].as_u64().unwrap_or(0))
        .sum::<u64>();
    let mut manifest = json!({
        "schema_version": 1,
        "contract": crate::CONTRACT_ID,
        "runtime": { "mode": "js", "kind": "wasm+js", "kernel": "pxl_web.wasm", "loader": "mq_js_bundle.js", "entry": "game.js", "version": 2 },
        "mode_was_default": args.mode_was_default(),
        "visual": args.game.is_none(),
        "input_package": { "path": "asset-package.json", "digest": synced.package_digest },
        "files": files,
        "total_bytes": total_bytes,
    });
    if let Some(audio) = synced.audio {
        manifest["audio"] = audio;
    }
    write_json(&staging.join("engine-build.json"), &manifest)?;
    let envelope = StepEnvelope {
        schema_version: 1,
        step: STEP_ID.to_string(),
        step_version: VERSION.to_string(),
        contract: crate::CONTRACT_ID.to_string(),
        status: StepStatus::Ok,
        scope: Scope::All,
        inputs: vec![DigestEntry {
            path: "asset-package.json".to_string(),
            digest: digest_file(&args.input.join("asset-package.json"))?,
        }],
        outputs: vec![DigestEntry {
            path: "engine-build.json".to_string(),
            digest: digest_file(&staging.join("engine-build.json"))?,
        }],
        verdict: Verdict::Pass,
        gates: Gates {
            required_artifacts: vec![GateCheck {
                id: "build.web".to_string(),
                result: GateResult::Pass,
                detail: None,
            }],
            quality_checks: Vec::new(),
            not_assessed: Vec::new(),
        },
        error: None,
    };
    write_json(&staging.join("step.json"), &serde_json::to_value(envelope)?)?;
    Ok(())
}

fn default_runtime() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime-js")
}

fn rejected_path(out: &Path) -> PathBuf {
    let name = out
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    out.with_file_name(format!(".{name}.rejected"))
}

fn sync_runtime(runtime: &Path, web: &Path) -> Result<()> {
    if !runtime.is_dir() {
        return Err(HarnessError::not_found(format!(
            "web runtime {} is missing",
            runtime.display()
        )));
    }
    let dist = runtime.join("dist");
    let kernel_dir = if dist.join("pxl_web.wasm").is_file() {
        dist
    } else {
        runtime.to_path_buf()
    };
    let wasm = kernel_dir.join("pxl_web.wasm");
    if !wasm.is_file() {
        return Err(
            HarnessError::not_found("pxl_web.wasm is missing".to_string()).with_hint(
                "run scripts/build-web-engine.sh or pass --runtime-dir with the skill web-runtime",
            ),
        );
    }
    copy(&wasm, &web.join("pxl_web.wasm"))?;
    for entry in fs::read_dir(runtime)
        .map_err(io("cannot read web runtime"))?
        .flatten()
    {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("js") {
            copy(&path, &web.join(entry.file_name()))?;
        }
    }
    if !web.join("mq_js_bundle.js").is_file() {
        copy(
            &kernel_dir.join("mq_js_bundle.js"),
            &web.join("mq_js_bundle.js"),
        )?;
    }
    if runtime.join("index.d.ts").is_file() {
        copy(&runtime.join("index.d.ts"), &web.join("index.d.ts"))?;
    }
    if runtime.join("browser-playtest.py").is_file() {
        copy(
            &runtime.join("browser-playtest.py"),
            &web.join("browser-playtest.py"),
        )?;
    }
    let mut template = runtime.join("template/index.html");
    if !template.is_file() {
        template = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("template/index.html");
    }
    if !template.is_file() {
        return Err(HarnessError::not_found(format!(
            "{} is missing",
            template.display()
        )));
    }
    copy(&template, &web.join("index.html"))?;
    Ok(())
}

fn sync_showcase(runtime: &Path, web: &Path) -> Result<()> {
    let showcase = runtime.join("showcase.js");
    if !showcase.is_file() {
        return Err(HarnessError::not_found(format!(
            "{} is missing; the generic visual runtime is required",
            showcase.display()
        )));
    }
    copy(&showcase, &web.join("showcase.js"))?;
    fs::write(
        web.join("game.js"),
        "import { run } from \"./pxl_runtime.js\";\nimport { loadPackage } from \"./assets.js\";\nimport { createShowcase } from \"./showcase.js\";\nconst pkg = await loadPackage(\"assets\");\nconst showcase = createShowcase(pkg);\nrun(showcase.game, showcase.config);\n",
    )
    .map_err(io("cannot write showcase entry"))
}

fn sync_game(game: &Path, web: &Path) -> Result<()> {
    let main = game.join("main.js");
    if !main.is_file() {
        return Err(
            HarnessError::not_found(format!("{} is missing", main.display()))
                .with_hint("pass --game games/<name>/web"),
        );
    }
    // main.js 落到 web/game.js：同样要把 runtime 源码路径改写成同目录相对路径。
    let entry = fs::read_to_string(&main).map_err(io("cannot read game entry"))?;
    let entry = entry.replace("../../../steps/step-500-engine/runtime-js/", "./");
    fs::write(web.join("game.js"), entry).map_err(io("cannot write game entry"))?;
    sync_game_scripts(game, game, web)?;
    if game.join("replay").is_dir() {
        copy_tree(&game.join("replay"), &web.join("replay"))?;
    }
    let index = game.join("index.html");
    if index.is_file() {
        copy(&index, &web.join("index.html"))?;
    }
    Ok(())
}

/// 递归复制游戏脚本（保留子目录），并把 runtime 的源码路径改写成构建产物里的相对路径。
fn sync_game_scripts(game: &Path, dir: &Path, web: &Path) -> Result<()> {
    for entry in fs::read_dir(dir).map_err(io("cannot read game source"))?.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            if dir == game && name == "replay" {
                continue;
            }
            sync_game_scripts(game, &path, web)?;
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("js")
            || (dir == game && name == "main.js")
            || name.ends_with(".test.js")
        {
            continue;
        }
        let relative = path.strip_prefix(game).unwrap_or(&path);
        let depth = relative.components().count().saturating_sub(1);
        let target = web.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(io("cannot create script directory"))?;
        }
        let text = fs::read_to_string(&path).map_err(io("cannot read game script"))?;
        let runtime_prefix = if depth == 0 {
            "./".to_string()
        } else {
            "../".repeat(depth)
        };
        let text = text.replace("../../../steps/step-500-engine/runtime-js/", &runtime_prefix);
        fs::write(&target, text).map_err(io("cannot write game script"))?;
    }
    Ok(())
}

struct SyncSummary {
    package_digest: String,
    audio: Option<Value>,
}

fn sync_assets(input: &Path, out: &Path) -> Result<SyncSummary> {
    let package_path = input.join("asset-package.json");
    let text = fs::read_to_string(&package_path).map_err(io("cannot read asset-package.json"))?;
    let package_digest = digest_bytes(text.as_bytes());
    let mut package: Value = serde_json::from_str(&text)
        .map_err(|e| HarnessError::schema(format!("asset-package.json: {e}")))?;
    let package_version = package["schema_version"].as_u64().unwrap_or(0);
    if package_version != 3 && package_version != 4 {
        return Err(HarnessError::schema(format!(
            "unsupported asset-package schema_version {package_version}; regenerate it with asset-package/v4"
        )));
    }
    fs::create_dir_all(out.join("sheets")).map_err(io("cannot create sheets output"))?;
    let palette_rel = package["palette"]["path"]
        .as_str()
        .ok_or_else(|| HarnessError::schema("asset package has no palette path".to_string()))?;
    copy(&input.join(palette_rel), &out.join("palette.json"))?;
    package["palette"]["path"] = Value::String("palette.json".to_string());

    for sheet in package["sheets"].as_array_mut().into_iter().flatten() {
        let id = sheet["id"].as_str().unwrap_or("sheet").to_string();
        let rel = sheet["path"]
            .as_str()
            .ok_or_else(|| HarnessError::schema(format!("sheet {id} has no path")))?;
        let source = input.join(rel);
        let mut document: Value = serde_json::from_str(
            &fs::read_to_string(&source).map_err(io("cannot read sheet json"))?,
        )
        .map_err(|e| HarnessError::schema(format!("{}: {e}", source.display())))?;
        let image = document["meta"]["image"]
            .as_str()
            .ok_or_else(|| HarnessError::schema(format!("sheet {id} has no meta.image")))?;
        let image_name = format!("{id}.sheet.png");
        copy(
            &source.parent().unwrap_or(input).join(image),
            &out.join("sheets").join(&image_name),
        )?;
        document["meta"]["image"] = Value::String(image_name.clone());
        let json_name = format!("{id}.sheet.json");
        write_json(&out.join("sheets").join(&json_name), &document)?;
        sheet["path"] = Value::String(format!("sheets/{json_name}"));
    }

    if let Some(levels) = package["levels"]["path"].as_str() {
        let source = input.join(levels);
        let mut files = Vec::new();
        for entry in fs::read_dir(&source)
            .map_err(io("cannot read levels"))?
            .flatten()
        {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                let name = entry.file_name().to_string_lossy().into_owned();
                copy(&path, &out.join("levels").join(&name))?;
                files.push(name);
            }
        }
        files.sort();
        write_json(&out.join("levels/index.json"), &json!({ "files": files }))?;
        package["levels"]["path"] = Value::String("levels".to_string());
    }

    sync_visual(input, out, &mut package)?;

    // P06: compiled scenes ride along into the web bundle; the runtime
    // consumes them in P07, but the bundle must already be self-contained.
    for scene in package["scenes"].as_array().cloned().unwrap_or_default() {
        let id = scene["id"].as_str().unwrap_or_default().to_string();
        let rel = scene["path"].as_str().unwrap_or_default().to_string();
        let expected = scene["digest"].as_str().unwrap_or_default().to_string();
        if rel.is_empty() || expected.is_empty() {
            return Err(HarnessError::schema(format!("scene {id} has no path or digest")));
        }
        if rel.split('/').any(|part| part.is_empty() || part == "." || part == "..") {
            return Err(HarnessError::schema(format!("scene {id} has an unsafe path")));
        }
        copy_verified(&input.join(&rel), &out.join(&rel), &expected)?;
    }

    let mut audio_summary = None;
    let audio_entries = package["audio"].as_array().cloned().unwrap_or_default();
    if !audio_entries.is_empty() {
        let manifest_rel = package["audio_manifest"]["path"]
            .as_str()
            .and_then(|path| path.strip_prefix("assets/audio/"))
            .ok_or_else(|| {
                HarnessError::schema("audio manifest must be below assets/audio/".to_string())
            })?;
        let manifest_expected = package["audio_manifest"]["digest"]
            .as_str()
            .ok_or_else(|| HarnessError::schema("audio manifest has no digest".to_string()))?;
        copy_verified(
            &input.join(format!("assets/audio/{manifest_rel}")),
            &out.join("audio").join(manifest_rel),
            manifest_expected,
        )?;
        package["audio_manifest"]["path"] = Value::String(format!("audio/{manifest_rel}"));
        let mut decoded_bytes = 0u64;
        let mut required = 0usize;
        for entry in package["audio"].as_array_mut().into_iter().flatten() {
            let id = entry["id"].as_str().unwrap_or("audio").to_string();
            let rel = entry["path"]
                .as_str()
                .and_then(|path| path.strip_prefix("assets/audio/"))
                .ok_or_else(|| {
                    HarnessError::schema(format!("audio {id} must be below assets/audio/"))
                })?
                .to_string();
            if rel
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
            {
                return Err(HarnessError::schema(format!(
                    "audio {id} has an unsafe path"
                )));
            }
            let expected = entry["digest"]
                .as_str()
                .ok_or_else(|| HarnessError::schema(format!("audio {id} has no digest")))?;
            copy_verified(
                &input.join(format!("assets/audio/{rel}")),
                &out.join("audio").join(&rel),
                expected,
            )?;
            entry["path"] = Value::String(format!("audio/{rel}"));
            decoded_bytes +=
                entry["frames"].as_u64().unwrap_or(0) * entry["channels"].as_u64().unwrap_or(0) * 4;
            required += usize::from(entry["required"].as_bool() == Some(true));
        }
        audio_summary = Some(json!({
            "entries": audio_entries.len(),
            "required": required,
            "decoded_bytes": decoded_bytes,
            "cache_budget_bytes": 33554432,
            "preload_within_budget": decoded_bytes <= 33554432,
        }));
    } else if package.get("audio_manifest").is_some() {
        return Err(HarnessError::schema(
            "audio_manifest is present but the audio collection is empty".to_string(),
        ));
    }
    write_json(&out.join("asset-package.json"), &package)?;
    Ok(SyncSummary {
        package_digest,
        audio: audio_summary,
    })
}

fn sync_visual(input: &Path, out: &Path, package: &mut Value) -> Result<()> {
    let primary_recipe = package["visual"]["builds"][0]["recipe"]
        .as_str()
        .unwrap_or("package")
        .to_string();
    for build in package["visual"]["builds"].as_array_mut().into_iter().flatten() {
        let recipe = build["recipe"].as_str().unwrap_or("package").to_string();
        for key in ["report", "metadata", "preview_manifest", "effective_style"] {
            build[key] = sync_visual_entry(input, out, &recipe, &build[key])?;
        }
    }
    for key in ["report", "metadata", "preview_manifest", "effective_style"] {
        package["visual"][key] = sync_visual_entry(
            input,
            out,
            &primary_recipe,
            &package["visual"][key].clone(),
        )?;
    }
    for sheet in package["sheets"].as_array_mut().into_iter().flatten() {
        let recipe = sheet["id"].as_str().unwrap_or("package").to_string();
        if !sheet["palette"]["path"].as_str().unwrap_or_default().is_empty() {
            sheet["palette"] = sync_visual_entry(input, out, &recipe, &sheet["palette"].clone())?;
        }
    }
    for asset in package["assets"].as_array_mut().into_iter().flatten() {
        let recipe = asset["sheet"].as_str().unwrap_or("package").to_string();
        for channel in asset["channels"].as_array_mut().into_iter().flatten() {
            for key in ["document", "sheet", "image", "palette"] {
                channel[key] = sync_visual_entry(input, out, &recipe, &channel[key].clone())?;
            }
        }
    }
    Ok(())
}

fn sync_visual_entry(
    input: &Path,
    out: &Path,
    recipe: &str,
    entry: &Value,
) -> Result<Value> {
    let source_rel = entry["path"]
        .as_str()
        .ok_or_else(|| HarnessError::schema("visual file entry has no path".to_string()))?;
    let expected = entry["digest"]
        .as_str()
        .ok_or_else(|| HarnessError::schema(format!("visual file '{source_rel}' has no digest")))?;
    let name = Path::new(source_rel)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| HarnessError::schema(format!("visual file '{source_rel}' has no filename")))?;
    let relative = format!("visual/{recipe}/{name}");
    copy_verified(
        &input.join(source_rel),
        &out.join(&relative),
        expected,
    )?;
    Ok(json!({"path": relative, "digest": expected}))
}

fn copy_verified(source: &Path, destination: &Path, expected: &str) -> Result<()> {
    let actual = digest_file(source)?;
    if actual != expected {
        return Err(HarnessError::integrity(format!(
            "{}: {actual} != {expected}",
            source.display()
        )));
    }
    copy(source, destination)
}

fn copy(source: &Path, destination: &Path) -> Result<()> {
    if !source.is_file() {
        return Err(HarnessError::not_found(format!(
            "{} is missing",
            source.display()
        )));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(io("cannot create output directory"))?;
    }
    fs::copy(source, destination)
        .map(|_| ())
        .map_err(io("cannot copy file"))
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    for entry in fs::read_dir(source)
        .map_err(io("cannot read directory"))?
        .flatten()
    {
        let path = entry.path();
        let target = destination.join(entry.file_name());
        if path.is_dir() {
            copy_tree(&path, &target)?;
        } else {
            copy(&path, &target)?;
        }
    }
    Ok(())
}

fn collect_files(root: &Path, dir: &Path, files: &mut Vec<Value>) -> Result<()> {
    for entry in fs::read_dir(dir)
        .map_err(io("cannot list build output"))?
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, files)?;
        } else {
            files.push(json!({
                "path": path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"),
                "bytes": fs::metadata(&path).map_err(io("cannot stat build output"))?.len(),
                "digest": digest_file(&path)?,
            }));
        }
    }
    Ok(())
}

fn count_files(dir: &Path) -> Result<usize> {
    let mut files = Vec::new();
    collect_files(dir, dir, &mut files)?;
    Ok(files.len())
}

fn directory_bytes(dir: &Path) -> Result<u64> {
    let mut files = Vec::new();
    collect_files(dir, dir, &mut files)?;
    Ok(files
        .iter()
        .map(|file| file["bytes"].as_u64().unwrap_or(0))
        .sum())
}

fn io(message: &'static str) -> impl FnOnce(std::io::Error) -> HarnessError {
    move |error| HarnessError::io(format!("{message}: {error}"))
}
