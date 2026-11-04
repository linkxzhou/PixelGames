#!/usr/bin/env python3
import argparse
import base64
import functools
import hashlib
import json
from pathlib import Path
import threading
import time
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer


def write_report(path, report):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


def data_url_bytes(value):
    return base64.b64decode(value.split(",", 1)[1])


def save_data_url(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data_url_bytes(value))


def stable_digest(value):
    """sha256 of the decoded image bytes; None when there is no image.

    Python's built-in hash() is salted per process and must never be used for
    evidence that has to compare across runs.
    """
    if not value:
        return None
    return "sha256:" + hashlib.sha256(data_url_bytes(value)).hexdigest()


def percentile(samples, fraction):
    """Linear-interpolated percentile of a non-empty list; None if empty."""
    if not samples:
        return None
    ordered = sorted(samples)
    if len(ordered) == 1:
        return round(ordered[0], 3)
    position = (len(ordered) - 1) * fraction
    low = int(position)
    high = min(low + 1, len(ordered) - 1)
    value = ordered[low] + (ordered[high] - ordered[low]) * (position - low)
    return round(value, 3)


def summarize(samples):
    """Distribution summary that states how little it knows when n is small."""
    return {
        "samples": len(samples),
        "min": round(min(samples), 3) if samples else None,
        "p50": percentile(samples, 0.5),
        "p95": percentile(samples, 0.95),
        "max": round(max(samples), 3) if samples else None,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dir", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    (args.out / "frames").mkdir(parents=True, exist_ok=True)
    report_path = args.out / "browser-report.json"
    try:
        from playwright.sync_api import sync_playwright
    except Exception as error:
        write_report(report_path, {"status": "NOT_ASSESSED", "ok": False, "reason": f"Playwright unavailable: {error}"})
        return 0

    handler = functools.partial(SimpleHTTPRequestHandler, directory=str(args.dir.resolve()))
    server = ThreadingHTTPServer(("127.0.0.1", 0), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    captures = []
    errors = []
    try:
        with sync_playwright() as playwright:
            cached = Path.home() / "Library/Caches/ms-playwright"
            executables = sorted(cached.glob("chromium-*/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing"), reverse=True)
            options = {"headless": True, "args": ["--no-sandbox"]}
            if executables:
                options["executable_path"] = str(executables[0])
            try:
                browser = playwright.chromium.launch(**options)
            except Exception as error:
                write_report(report_path, {"status": "NOT_ASSESSED", "ok": False, "reason": f"Chromium unavailable: {error}"})
                return 0
            for dpr in [1, 2]:
                phases = {}
                mark = time.perf_counter()

                def lap(name):
                    nonlocal mark
                    now = time.perf_counter()
                    phases[name] = round((now - mark) * 1000, 3)
                    mark = now

                page = browser.new_page(viewport={"width": 960, "height": 832}, device_scale_factor=dpr)
                page.add_init_script("globalThis.__pxlPlaytest = true")
                page.on("console", lambda message: errors.append(message.text) if message.type == "error" else None)
                page.on("pageerror", lambda error: errors.append(str(error)))
                lap("page_open_ms")
                page.goto(f"http://127.0.0.1:{server.server_port}", wait_until="networkidle")
                lap("load_ms")
                page.wait_for_function("window.__pxlReady === true || window.__pxlError", timeout=15000)
                lap("first_frame_ms")
                failure = page.evaluate("window.__pxlError || null")
                if failure:
                    errors.append(failure)
                    continue
                if not page.evaluate("Boolean(window.__pxlTest)"):
                    errors.append("runtime test controls are unavailable")
                    continue
                # Browser frame cadence after warm-up. This is requestAnimationFrame
                # spacing, not GPU time, and the sample is small; the report says so.
                intervals = page.evaluate("""() => new Promise(resolve => {
                    const warmup = 30, wanted = 90, stamps = [];
                    let seen = 0;
                    const step = now => {
                        seen += 1;
                        if (seen > warmup) stamps.push(now);
                        if (stamps.length > wanted) {
                            resolve(stamps.slice(1).map((t, i) => t - stamps[i]));
                        } else {
                            requestAnimationFrame(step);
                        }
                    };
                    requestAnimationFrame(step);
                })""")
                lap("frame_sampling_ms")
                target_tick = page.evaluate("window.__pxlTest.status().capture_tick ?? window.__pxlTest.status().tick")
                capture = page.evaluate("""tick => {
                    const controls = window.__pxlTest;
                    controls.pause();
                    const current = controls.status().tick;
                    if (tick < current) throw new Error(`capture tick ${tick} precedes current tick ${current}`);
                    controls.advanceTicks(tick - current);
                    return controls.capture();
                }""", target_tick)
                lap("capture_ms")
                save_data_url(args.out / f"frames/logical-dpr{dpr}.png", capture["logical"])
                page.locator("#glcanvas").screenshot(path=str(args.out / f"frames/presentation-dpr{dpr}.png"))
                presentation_path = args.out / f"frames/presentation-dpr{dpr}.png"
                capture["presentation"] = f"frames/presentation-dpr{dpr}.png"
                capture["presentation_digest"] = "sha256:" + hashlib.sha256(presentation_path.read_bytes()).hexdigest()
                lap("screenshot_ms")
                captures.append({"dpr": dpr, **capture})
                captures[-1]["phases_ms"] = phases
                captures[-1]["frame_intervals_ms"] = [round(value, 3) for value in intervals]
                captures[-1]["elapsed_ms"] = round(sum(phases.values()), 3)
                page.close()
            browser.close()
        visual = [capture for capture in captures if capture.get("logical") is not None]
        digests = [stable_digest(capture.get("logical")) for capture in visual]
        stable = len(digests) < 2 or digests[0] == digests[1]
        ok = not errors and stable and bool(captures)
        status = "PASS" if ok else "FAIL"
        scene_status = captures[0]["status"] if captures else {}
        all_intervals = [value for capture in captures for value in capture.get("frame_intervals_ms", [])]
        phase_names = sorted({name for capture in captures for name in capture.get("phases_ms", {})})
        render_report = {
            "contract": "render-report/v1",
            "status": status,
            "captures": [
                {
                    "dpr": capture["dpr"],
                    "status": capture["status"],
                    "logical_digest": stable_digest(capture.get("logical")),
                    "presentation_digest": capture.get("presentation_digest"),
                    "phases_ms": capture.get("phases_ms", {}),
                }
                for capture in captures
            ],
            "stable_logical_across_dpr": stable,
            "evidence_layers": {
                "logical": "Canvas2D redraw of the scene description; not a read of the WebGL framebuffer",
                "presentation": "Playwright screenshot of the visible canvas at the given DPR",
            },
            "performance": {
                "browser": "chromium",
                "render_mode": scene_status.get("render_mode", "unknown"),
                "logical_size": scene_status.get("logical_size"),
                "lights": scene_status.get("lights", 0),
                "phases_ms": {
                    name: summarize([capture["phases_ms"][name] for capture in captures if name in capture.get("phases_ms", {})])
                    for name in phase_names
                },
                "raf_interval_ms": summarize(all_intervals),
                "note": "raf_interval_ms is browser frame spacing after a 30-frame warm-up on this machine; it is not GPU time and is not a device guarantee. Few samples make p95 indicative only.",
            },
            "audio": {"status": "NOT_ASSESSED", "listening": "NOT_ASSESSED"},
        }
        (args.out / "render-report.json").write_text(json.dumps(render_report, indent=2) + "\n")
        write_report(report_path, {
            "status": status,
            "ok": ok,
            "console_errors": errors,
            "captures": len(captures),
            "stable_logical_across_dpr": stable,
            "render_report": "render-report.json",
        })
        return 0 if ok else 1
    except Exception as error:
        write_report(report_path, {"status": "FAIL", "ok": False, "reason": str(error), "console_errors": errors})
        return 1
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)


if __name__ == "__main__":
    raise SystemExit(main())
