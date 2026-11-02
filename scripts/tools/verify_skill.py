#!/usr/bin/env python3
"""Verify the bundled `pxl` skill (plan/v5.0/README.md section 7).

Usage:
    python3 scripts/tools/verify_skill.py [--skill DIR] [--json] [--keep]

Deterministic and offline: it never calls a real model. Checks, in order:

  structure    skill-creator quick_validate, openai.yaml, stray docs, line counts
  links        relative markdown links resolve
  drift        `step-N <subcommand>` in code spans exists in that binary's --help
  binaries     manifest sha256, --version, --contract on the host platform
  e2e          fake-provider pipeline 100 -> 200 -> 300 -> 400 -> 500 via the launcher
  portability  copy the skill to a temp dir, clean PATH, rerun binaries + e2e
  web-runtime  bundled wasm/JS sha256 and miniquad interface drift
  audio        toolchain sha256, deterministic render, portable render and dependency negatives
  negative     unknown OS, missing binary/Node/adapter, version drift, step-500 build usage

Each check ends PASS, FAIL or NOT_ASSESSED. A check that could not run is never
PASS. Exit 1 only when something FAILs.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

PASS, FAIL, NA = "PASS", "FAIL", "NOT_ASSESSED"
STEPS = ["step-100", "step-200", "step-300", "step-400", "step-500"]
PLATFORMS = ["darwin-arm64", "darwin-x64", "linux-x64", "linux-arm64", "windows-x64"]
STRAY_DOCS = re.compile(r"^(readme|changelog|installation|quick_?reference)", re.I)

REPO = Path(__file__).resolve().parents[2]


class Report:
    def __init__(self) -> None:
        self.rows: list[tuple[str, str, str]] = []

    def add(self, check: str, status: str, detail: str = "") -> None:
        self.rows.append((check, status, detail))

    def worst(self) -> str:
        order = [FAIL, NA, PASS]
        for status in order:
            if any(r[1] == status for r in self.rows):
                return status
        return PASS


class StepFailed(Exception):
    pass


def host_platform() -> str:
    machine = os.uname().machine.lower() if hasattr(os, "uname") else os.environ.get("PROCESSOR_ARCHITECTURE", "").lower()
    arch = "arm64" if machine in ("arm64", "aarch64") else "x64" if machine in ("x86_64", "amd64") else machine
    if sys.platform == "darwin":
        return f"darwin-{arch}"
    if sys.platform.startswith("linux"):
        return f"linux-{arch}"
    if os.name == "nt":
        return f"windows-{arch}"
    return f"{sys.platform}-{arch}"


def exe(name: str) -> str:
    return name + (".exe" if os.name == "nt" else "")


def launcher(skill: Path) -> list[str]:
    if os.name == "nt":
        return ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(skill / "scripts" / "run.ps1")]
    return [str(skill / "scripts" / "run.sh")]


def run(cmd: list[str], env: dict[str, str] | None = None, cwd: Path | None = None) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, env=env, cwd=cwd, timeout=300)


def skill_dirs(skill: Path) -> list[Path]:
    subs = sorted(p for p in (skill / "skills").glob("*") if (p / "SKILL.md").is_file())
    return [skill, *subs]


def md_files(skill: Path) -> list[Path]:
    return sorted(p for p in skill.rglob("*.md") if "bin" not in p.relative_to(skill).parts[:1])


# ---------------------------------------------------------------- structure

def load_quick_validate():
    base = Path(os.environ.get("SKILL_CREATOR_DIR", Path.home() / ".codebuddy/skills/.system/skill-creator"))
    path = base / "scripts" / "quick_validate.py"
    if not path.is_file():
        return None
    spec = importlib.util.spec_from_file_location("quick_validate", path)
    mod = importlib.util.module_from_spec(spec)
    try:
        spec.loader.exec_module(mod)
    except Exception:
        return None
    return mod


def check_structure(skill: Path, r: Report) -> None:
    qv = load_quick_validate()
    for d in skill_dirs(skill):
        tag = f"structure:{d.name}"
        if qv is None:
            r.add(f"{tag}:quick_validate", NA, "skill-creator quick_validate.py not found (set SKILL_CREATOR_DIR)")
        else:
            ok, msg = qv.validate_skill(d)
            r.add(f"{tag}:quick_validate", PASS if ok else FAIL, msg)

        text = (d / "SKILL.md").read_text(encoding="utf-8")
        m = re.search(r"^name:\s*(\S+)", text, re.M)
        name = m.group(1) if m else ""
        r.add(f"{tag}:name-matches-dir", PASS if name == d.name else FAIL, f"name={name!r} dir={d.name!r}")

        lines = text.count("\n") + 1
        r.add(f"{tag}:lines", PASS if lines < 500 else FAIL, f"{lines} lines")

        yml = d / "agents" / "openai.yaml"
        if not yml.is_file():
            r.add(f"{tag}:openai.yaml", FAIL, "agents/openai.yaml is missing")
        else:
            ytext = yml.read_text(encoding="utf-8")
            pm = re.search(r'default_prompt:\s*"([^"]*)"', ytext)
            ok = bool(pm) and f"${d.name}" in pm.group(1)
            r.add(f"{tag}:openai.yaml", PASS if ok else FAIL, "default_prompt must quote the skill as $name")

        stray = [
            str(p.relative_to(d))
            for p in d.rglob("*")
            if p.is_file() and STRAY_DOCS.match(p.name) and "bin" not in p.relative_to(d).parts[:1]
            and "skills" not in p.relative_to(d).parts[:1]
        ]
        r.add(f"{tag}:no-stray-docs", PASS if not stray else FAIL, ", ".join(stray))


# -------------------------------------------------------------------- links

LINK = re.compile(r"\]\(([^)\s]+)\)")


def check_links(skill: Path, r: Report) -> None:
    bad: list[str] = []
    total = 0
    for md in md_files(skill):
        for target in LINK.findall(md.read_text(encoding="utf-8")):
            if re.match(r"^[a-z]+:", target) or target.startswith("#"):
                continue
            total += 1
            path = target.split("#", 1)[0]
            if not (md.parent / path).exists():
                bad.append(f"{md.relative_to(skill)} -> {target}")
    r.add("links", PASS if not bad else FAIL, f"{total} links" if not bad else "; ".join(bad))


# -------------------------------------------------------------------- drift

CMD = re.compile(r"(?<![\w-])(step-[1-5]00) ([a-z][a-z-]*)(?:\|[a-z-]+)*(?: ([a-z][a-z-]*)(?![\w./=-]))?")
FENCE = re.compile(r"```.*?```", re.S)
INLINE = re.compile(r"`([^`\n]+)`")


def code_chunks(text: str) -> list[str]:
    """Code text with trailing `# comments` removed, so prose is never read as a command."""
    chunks = FENCE.findall(text)
    rest = FENCE.sub("", text)
    chunks += INLINE.findall(rest)
    return ["\n".join(line.split(" #", 1)[0].split("\t#", 1)[0] for line in c.splitlines()) for c in chunks]


def help_commands(binary: Path, path: tuple[str, ...], cache: dict) -> set[str] | None:
    key = (binary.name, path)
    if key in cache:
        return cache[key]
    p = run([str(binary), *path, "--help"])
    cmds: set[str] | None = None
    if p.returncode == 0:
        cmds = set()
        in_cmds = False
        for line in p.stdout.splitlines():
            if line.startswith("Commands:"):
                in_cmds = True
                continue
            if in_cmds:
                if not line.strip():
                    break
                cmds.add(line.split()[0])
    cache[key] = cmds
    return cmds


def check_drift(skill: Path, r: Report) -> None:
    plat = host_platform()
    bindir = skill / "bin" / plat
    if not bindir.is_dir():
        r.add("drift", NA, f"no bundled binaries for host platform {plat}")
        return
    seen: set[tuple[str, str, str | None]] = set()
    for md in md_files(skill):
        for chunk in code_chunks(md.read_text(encoding="utf-8")):
            for step, sub, sub2 in CMD.findall(chunk):
                seen.add((step, sub, sub2 or None))
    cache: dict = {}
    bad: list[str] = []
    for step, sub, sub2 in sorted(seen, key=lambda t: (t[0], t[1], t[2] or "")):
        binary = bindir / exe(step)
        top = help_commands(binary, (), cache)
        if top is None:
            bad.append(f"{step}: --help failed")
            continue
        if sub not in top:
            bad.append(f"{step} {sub}: not in --help")
            continue
        if sub2:
            nested = help_commands(binary, (sub,), cache)
            if nested and sub2 not in nested:
                bad.append(f"{step} {sub} {sub2}: not in --help")
    r.add("drift", PASS if not bad else FAIL, f"{len(seen)} command forms" if not bad else "; ".join(bad))


# ----------------------------------------------------------------- binaries

def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return "sha256:" + h.hexdigest()


def check_binaries(skill: Path, r: Report, tag: str = "binaries", env: dict[str, str] | None = None) -> None:
    manifest_path = skill / "bin" / "manifest.json"
    if not manifest_path.is_file():
        r.add(f"{tag}:manifest", FAIL, "bin/manifest.json is missing")
        return
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    host = host_platform()
    for plat in PLATFORMS:
        entry = manifest["platforms"].get(plat)
        if plat != host:
            where = "bundled" if entry else "not bundled"
            r.add(f"{tag}:{plat}", NA, f"{where}; cannot run on host {host}")
            continue
        if not entry:
            r.add(f"{tag}:{plat}", FAIL, "host platform missing from manifest")
            continue
        problems: list[str] = []
        for name, want in entry.items():
            path = skill / "bin" / plat / name
            if not path.is_file():
                problems.append(f"{name} missing")
            elif sha256(path) != want:
                problems.append(f"{name} sha256 mismatch")
        r.add(f"{tag}:{plat}:sha256", PASS if not problems else FAIL, ", ".join(problems) or f"{len(entry)} files")

        launch = launcher(skill)
        probes: list[str] = []
        for step in STEPS:
            p = run([*launch, step, "--version"], env=env)
            if p.returncode != 0 or manifest["workspace_version"] not in p.stdout:
                probes.append(f"{step} --version exit {p.returncode}")
            args = ["schema", "recipe"] if step == "step-300" else ["--contract"]
            p = run([*launch, step, *args], env=env)
            if p.returncode != 0:
                probes.append(f"{step} {' '.join(args)} exit {p.returncode}")
        r.add(f"{tag}:{plat}:run", PASS if not probes else FAIL, "; ".join(probes) or "version + contract ok")


# ------------------------------------------------------------- web runtime

def check_web_runtime(skill: Path, r: Report) -> None:
    manifest_path = skill / "bin/manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    entry = manifest.get("web_runtime", {})
    root = skill / "assets/web-runtime"
    problems = []
    for name, want in entry.get("files", {}).items():
        path = root / name
        if not path.is_file():
            problems.append(f"{name} missing")
        elif sha256(path) != want:
            problems.append(f"{name} sha256 mismatch")
    if not entry.get("files"):
        problems.append("bin/manifest.json web_runtime entry missing")
    r.add("web-runtime:sha256", PASS if not problems else FAIL, "; ".join(problems) or f"{len(entry['files'])} files")

    runtime_manifest = root / "manifest.json"
    problems = []
    if runtime_manifest.is_file():
        for name, item in json.loads(runtime_manifest.read_text(encoding="utf-8"))["files"].items():
            path = root / name
            if not path.is_file() or sha256(path) != item["sha256"]:
                problems.append(f"{name} missing or stale")
    else:
        problems.append("manifest.json missing")
    r.add("web-runtime:manifest", PASS if not problems else FAIL, "; ".join(problems))

    mini = skill / "skills/pxl-miniquad-wasm"
    for script in ["extract_interfaces.py", "extract_events.py"]:
        p = run([sys.executable, str(mini / "scripts" / script), "--vendor", str(REPO / "vendor/miniquad"),
                 "--out", str(mini / "references"), "--check"])
        r.add(f"miniquad:{script}", PASS if p.returncode == 0 else FAIL, (p.stdout + p.stderr).strip())


# ---------------------------------------------------------- audio toolchain

def check_audio_toolchain(skill: Path, r: Report, audio_spec: Path, tag: str = "audio-toolchain",
                          env: dict[str, str] | None = None) -> None:
    manifest_path = skill / "bin/manifest.json"
    if not manifest_path.is_file():
        r.add(f"{tag}:manifest", FAIL, "bin/manifest.json is missing")
        return
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    entry = manifest.get("audio_toolchain", {})
    root = skill / "assets/audio-toolchain"
    problems = []
    for name, want in entry.get("files", {}).items():
        path = root / name
        if not path.is_file():
            problems.append(f"{name} missing")
        elif sha256(path) != want:
            problems.append(f"{name} sha256 mismatch")
    if not entry.get("files"):
        problems.append("audio_toolchain entry missing")
    r.add(f"{tag}:sha256", PASS if not problems else FAIL, "; ".join(problems) or f"{len(entry['files'])} files")
    if problems or not audio_spec.is_file():
        r.add(f"{tag}:render", NA if not audio_spec.is_file() else FAIL,
              "audio fixture missing" if not audio_spec.is_file() else "toolchain digest failed")
        return
    path_env = (env if env is not None else os.environ).get("PATH")
    if shutil.which("node", path=path_env) is None:
        r.add(f"{tag}:render", NA, "Node 22+ is unavailable")
        return
    work = Path(tempfile.mkdtemp(prefix="pxl-audio-skill-"))
    try:
        outputs = []
        for name in ["a", "b"]:
            out = work / name
            p = run([*launcher(skill), "step-300", "--json", "--strict", "audio", "render",
                     str(audio_spec), "-o", str(out)], env=env, cwd=work)
            if p.returncode != 0:
                r.add(f"{tag}:render", FAIL, f"render {name} exit {p.returncode}: {(p.stdout + p.stderr).strip()[:400]}")
                return
            p = run([*launcher(skill), "step-300", "--json", "audio", "inspect", str(out)], env=env, cwd=work)
            if p.returncode != 0:
                r.add(f"{tag}:render", FAIL, f"inspect {name} exit {p.returncode}: {(p.stdout + p.stderr).strip()[:400]}")
                return
            manifest = json.loads((out / "audio-manifest.json").read_text(encoding="utf-8"))
            if len(manifest["entries"]) != 4 or any(item["qa"]["status"] != "pass" for item in manifest["entries"]):
                r.add(f"{tag}:render", FAIL, "expected four audio entries with passing QA")
                return
            outputs.append(out)
        files = sorted(path.relative_to(outputs[0]) for path in outputs[0].rglob("*") if path.is_file())
        different = [str(rel) for rel in files if (outputs[0] / rel).read_bytes() != (outputs[1] / rel).read_bytes()]
        r.add(f"{tag}:render", PASS if not different else FAIL,
              "4 assets, two identical PCM/WAV/manifest renders" if not different else f"different: {different}")
    finally:
        shutil.rmtree(work, ignore_errors=True)


# ---------------------------------------------------------------------- e2e

def write_levels(work: Path) -> Path:
    levels = work / "levels"
    levels.mkdir(parents=True, exist_ok=True)
    level = {
        "schema_version": 1, "id": "level_0", "name": "e2e", "tile_size_px": 16,
        "grid_w": 4, "grid_h": 4, "grid": ["ffff"] * 4,
        "tile_semantics": [{"tile_id": "f", "behaviour": "floor", "blocks": False}],
        "placements": [], "spawn": [0, 0], "exit": [3, 3], "objective": "walk",
        "difficulty": 1, "seed": 1, "notes": "",
    }
    (levels / "level_0.json").write_text(json.dumps(level, indent=2), encoding="utf-8")
    return levels


def write_game(work: Path) -> Path:
    game = work / "game"
    (game / "replay").mkdir(parents=True, exist_ok=True)
    (game / "main.js").write_text("// verification game\n", encoding="utf-8")
    (game / "replay/playtest.json").write_text("{}\n", encoding="utf-8")
    (game / "headless.js").write_text("""import fs from 'node:fs';
const a=process.argv; const out=a[a.indexOf('--out')+1]; const report=a[a.indexOf('--report')+1];
fs.mkdirSync(out,{recursive:true});
fs.writeFileSync(report, JSON.stringify({panicked:false,frames:1,expected_frames:1,palette_mismatches:0,player_moved:true,shots:1,bricks_destroyed:1,enemies_destroyed:0,state_hash:'fixed',ok:true}));
""", encoding="utf-8")
    return game


def e2e(skill: Path, fx100: Path, fx200: Path, work: Path, env: dict[str, str] | None) -> str:
    launch = launcher(skill)
    levels = write_levels(work)
    design = fx200 / "design"

    def call(step: str, args: list[str], extra: dict[str, str] | None = None, want: int = 0) -> subprocess.CompletedProcess:
        e = dict(env if env is not None else os.environ)
        e.update(extra or {})
        p = run([*launch, step, *args], env=e, cwd=work)
        if p.returncode != want:
            raise StepFailed(f"{step} {' '.join(args[:2])}: exit {p.returncode}, want {want}: {p.stderr.strip()[:300]}")
        return p

    def read(path: Path):
        return json.loads(path.read_text(encoding="utf-8"))

    w100 = work / "100-design"
    call("step-100", ["init", "--brief", str(fx100 / "brief.json"), "--out", str(w100)])
    call("step-100", ["run", "--in", str(w100)], {"STEP_100_FAKE_TURNS": str(fx100 / "fake_turns.json")}, want=10)
    digest = read(w100 / "revision.manifest.json")["manifest_digest"]
    # Test-only approval; the skill itself tells agents never to approve on their own.
    call("step-100", ["approve", "--in", str(w100), "--digest", digest])
    if not (w100 / "asset-requirements.json").is_file():
        raise StepFailed("step-100 approve did not write asset-requirements.json")

    fake200 = {"STEP_200_FAKE_TURNS": str(fx200 / "fake_turns_full.json")}
    s200, s300, s400, s500 = (work / "200-art-spec/.slice", work / "300-build/.slice",
                              work / "400-package/.slice", work / "500-engine/.slice")
    call("step-200", ["run", "--in", str(design), "--out", str(s200), "--scope", "slice"], fake200)
    call("step-300", ["plan", "--in", str(s200), "--out", str(s300)])
    call("step-400", ["run", "--in", str(s300), "--requirements", str(design / "asset-requirements.json"),
                      "--out", str(s400), "--game-id", "e2e", "--levels", str(levels), "--scope", "slice",
                      "--pipeline", str(design / "pipeline.json")])
    call("step-500", ["check", "--in", str(s400), "--out", str(s500), "--scope", "slice"])
    call("step-500", ["build", "--in", str(s400), "--game", str(write_game(work)),
                      "--out", str(s500), "--overwrite"])
    call("step-500", ["check", "--in", str(s400), "--out", str(s500), "--scope", "slice"])
    path_env = (env if env is not None else os.environ).get("PATH")
    has_node = shutil.which("node", path=path_env) is not None
    call("step-500", ["playtest", "--in", str(s500), "--mode", "headless"], want=0 if has_node else 11)
    gate = work / "650-slice-gate"
    call("step-500", ["slice", "--work", str(work), "--out", str(gate)], want=0 if has_node else 11)
    verdict = read(gate / "slice-report.json")["verdict"]
    expected_verdict = "PROCEED" if has_node else "NOT_ASSESSED"
    if verdict != expected_verdict:
        raise StepFailed(f"slice verdict {verdict}, want {expected_verdict}")

    w200, w300, w400 = work / "200-art-spec", work / "300-build", work / "400-package"
    call("step-200", ["run", "--in", str(design), "--out", str(w200)], fake200)
    call("step-300", ["plan", "--in", str(w200), "--out", str(w300)])
    call("step-400", ["run", "--in", str(w300), "--requirements", str(design / "asset-requirements.json"),
                      "--out", str(w400), "--game-id", "e2e", "--levels", str(levels)])
    cov = read(w400 / "coverage.json")
    if cov["delivered"] != cov["required"] or cov["required"] != 2:
        raise StepFailed(f"coverage {cov['delivered']}/{cov['required']}, want 2/2")
    call("step-500", ["check", "--in", str(w400)])
    report = read(w400 / "engine-check.json")
    if report["ok"] is not True or len(report["checks"]) != 7:
        raise StepFailed("engine-check.json: ok must be true with 7 checks")
    return f"100 -> 200 -> 300 -> 400 -> 500, slice {expected_verdict}, 2/2 assets"


def check_e2e(skill: Path, r: Report, tag: str = "e2e", env: dict[str, str] | None = None,
              fx100: Path | None = None, fx200: Path | None = None) -> None:
    fx100 = fx100 or REPO / "steps/step-100-design/tests/fixtures"
    fx200 = fx200 or REPO / "steps/step-200-art-spec/tests/fixtures"
    if not (fx100 / "brief.json").is_file() or not (fx200 / "design").is_dir():
        r.add(tag, NA, "pipeline fixtures not found")
        return
    if (skill / "bin" / host_platform()).is_dir() is False:
        r.add(tag, NA, f"no bundled binaries for {host_platform()}")
        return
    work = Path(tempfile.mkdtemp(prefix="pxl-skill-e2e-"))
    try:
        r.add(tag, PASS, e2e(skill, fx100, fx200, work, env))
    except (StepFailed, KeyError, FileNotFoundError, subprocess.TimeoutExpired) as e:
        r.add(tag, FAIL, str(e))
    finally:
        shutil.rmtree(work, ignore_errors=True)


# -------------------------------------------------- portability + negatives

def check_portable_and_negative(skill: Path, r: Report, keep: bool) -> None:
    if os.name == "nt":
        r.add("portability", NA, "clean-PATH copy test is implemented for POSIX hosts only")
        r.add("negative", NA, "POSIX only")
        return
    tmp = Path(tempfile.mkdtemp(prefix="pxl-skill-portable-"))
    try:
        copy = tmp / "pxl"
        shutil.copytree(skill, copy, symlinks=True)
        fx100, fx200 = tmp / "fx/100", tmp / "fx/200"
        shutil.copytree(REPO / "steps/step-100-design/tests/fixtures", fx100)
        shutil.copytree(REPO / "steps/step-200-art-spec/tests/fixtures", fx200)
        audio_fx = tmp / "fx/audio"
        shutil.copytree(REPO / "games/battle-city/audio", audio_fx)
        env = {"PATH": "/usr/bin:/bin", "HOME": str(tmp / "home"), "TMPDIR": str(tmp)}
        node = shutil.which("node")
        if node:
            env["PATH"] = f"{Path(node).parent}:{env['PATH']}"
        (tmp / "home").mkdir()
        leaks = [t for t in ("cargo", "rustc") if shutil.which(t, path=env["PATH"])]
        if leaks:
            r.add("portability:clean-path", NA, f"{', '.join(leaks)} on the clean PATH; result not conclusive")

        check_binaries(copy, r, tag="portability", env=env)
        check_audio_toolchain(copy, r, audio_fx / "audio.spec.json", tag="portability:audio", env=env)
        check_e2e(copy, r, tag="portability:e2e", env=env, fx100=fx100, fx200=fx200)

        # Negatives, run on the copy so nothing in the repo is touched.
        fake = tmp / "fakebin"
        fake.mkdir()
        uname = fake / "uname"
        uname.write_text("#!/bin/sh\necho Plan9\n", encoding="utf-8")
        uname.chmod(0o755)
        p = run([*launcher(copy), "step-300", "--version"], env={**env, "PATH": f"{fake}:{env['PATH']}"})
        ok = p.returncode == 127 and "unsupported OS" in p.stderr
        r.add("negative:unknown-os", PASS if ok else FAIL, f"exit {p.returncode}: {p.stderr.strip()[:120]}")

        if node:
            p = run([*launcher(copy), "step-300", "--json", "audio", "render", str(audio_fx / "audio.spec.json"),
                     "-o", str(tmp / "missing-node-out")], env={**env, "PXL_AUDIO_NODE": str(tmp / "no-node")})
            ok = p.returncode == 1 and '"audio_error":"blocked"' in p.stdout
            r.add("negative:missing-node", PASS if ok else FAIL, f"exit {p.returncode}: {(p.stdout + p.stderr).strip()[:200]}")

            adapter = copy / "assets/audio-toolchain/cli.mjs"
            adapter.rename(copy / "assets/audio-toolchain/cli.mjs.missing")
            try:
                p = run([*launcher(copy), "step-300", "--json", "audio", "render", str(audio_fx / "audio.spec.json"),
                         "-o", str(tmp / "missing-adapter-out")], env=env)
                ok = p.returncode == 1 and '"audio_error":"blocked"' in p.stdout
                r.add("negative:missing-audio-adapter", PASS if ok else FAIL,
                      f"exit {p.returncode}: {(p.stdout + p.stderr).strip()[:200]}")
            finally:
                (copy / "assets/audio-toolchain/cli.mjs.missing").rename(adapter)
        else:
            r.add("negative:missing-node", NA, "Node unavailable")
            r.add("negative:missing-audio-adapter", NA, "Node unavailable")

        package_json = copy / "assets/audio-toolchain/package.json"
        original = package_json.read_bytes()
        try:
            document = json.loads(original)
            document["version"] = "99.0.0"
            package_json.write_text(json.dumps(document), encoding="utf-8")
            expected = json.loads((copy / "bin/manifest.json").read_text(encoding="utf-8"))["audio_toolchain"]["files"]["package.json"]
            ok = sha256(package_json) != expected
            r.add("negative:audio-version-drift", PASS if ok else FAIL, "toolchain manifest detects version/file drift")
        finally:
            package_json.write_bytes(original)

        p = run([*launcher(copy), "step-500", "build"], env=env)
        r.add("negative:step-500-build-usage", PASS if p.returncode == 2 else FAIL, f"exit {p.returncode}, want 2")

        victim = copy / "bin" / host_platform() / exe("step-300")
        victim.unlink()
        p = run([*launcher(copy), "step-300", "--version"], env=env)
        ok = p.returncode == 127 and "no step-300 binary" in p.stderr
        r.add("negative:missing-binary-no-fallback", PASS if ok else FAIL, f"exit {p.returncode}: {p.stderr.strip()[:120]}")
    finally:
        if keep:
            print(f"kept {tmp}", file=sys.stderr)
        else:
            shutil.rmtree(tmp, ignore_errors=True)


# --------------------------------------------------------------------- main

def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--skill", type=Path, default=REPO / "scripts/skills/pxl", help="entry skill directory")
    ap.add_argument("--json", action="store_true", help="print the report as JSON")
    ap.add_argument("--keep", action="store_true", help="keep the temp directory of the portability test")
    args = ap.parse_args()
    skill = args.skill.resolve()
    if not (skill / "SKILL.md").is_file():
        sys.exit(f"FAIL: {skill}/SKILL.md not found")

    r = Report()
    check_structure(skill, r)
    check_links(skill, r)
    check_drift(skill, r)
    check_binaries(skill, r)
    check_web_runtime(skill, r)
    check_audio_toolchain(skill, r, REPO / "games/battle-city/audio/audio.spec.json")
    check_e2e(skill, r)
    check_portable_and_negative(skill, r, args.keep)

    if args.json:
        print(json.dumps({"verdict": r.worst(), "checks": [dict(zip(("check", "status", "detail"), row)) for row in r.rows]}, indent=2))
    else:
        for check, status, detail in r.rows:
            print(f"{status:<13} {check}" + (f"  ({detail})" if detail else ""))
        counts = {s: sum(1 for x in r.rows if x[1] == s) for s in (PASS, FAIL, NA)}
        print(f"\nverdict: {r.worst()}  PASS={counts[PASS]} FAIL={counts[FAIL]} NOT_ASSESSED={counts[NA]}")
    sys.exit(1 if any(x[1] == FAIL for x in r.rows) else 0)


if __name__ == "__main__":
    main()
