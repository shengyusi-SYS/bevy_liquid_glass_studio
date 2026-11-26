#!/usr/bin/env python3
"""Inspect and drive ui_panel through its optional Bevy Remote Protocol bridge.

Run `./examples/preview.sh`, then use snapshot/find/settings/set/config,
move/click/drag/key, image, or capture. Coordinates are logical window pixels; node snapshot
centers are physical pixels, so divide them by the window's scale_factor.
`rpc` also exposes Bevy's standard reflected-component methods.
"""

import argparse
import json
import pathlib
import sys
import time
import urllib.request


class Bridge:
    def __init__(self, url):
        self.url = url

    def rpc(self, method, params=None):
        body = {"jsonrpc": "2.0", "id": 1, "method": method}
        if params is not None:
            body["params"] = params
        request = urllib.request.Request(
            self.url, json.dumps(body).encode(), {"Content-Type": "application/json"}
        )
        with urllib.request.urlopen(request, timeout=10) as response:
            result = json.load(response)
        if "error" in result:
            raise RuntimeError(json.dumps(result["error"], ensure_ascii=False))
        return result["result"]

    def snapshot(self):
        return self.rpc("glass/ui_snapshot")

    def settings(self):
        return self.rpc("world.get_resources", {"resource": "ui_panel::PreviewSettings"})

    def set(self, path, value):
        self.rpc("world.mutate_resources", {
            "resource": "ui_panel::PreviewSettings", "path": path, "value": value
        })
        self.frames()
        return self.snapshot()["preview"]

    def config(self):
        return self.rpc("glass/rust_config")["code"]

    def frames(self, count=2):
        start = self.snapshot()["frame"]
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            if (self.snapshot()["frame"] - start) % (2**32) >= count:
                return
            time.sleep(0.02)
        raise TimeoutError("Bevy did not advance frames within 10 seconds")

    def window(self):
        windows = self.snapshot()["windows"]
        if len(windows) != 1:
            raise RuntimeError("Expected one primary window")
        return windows[0]

    def event(self, variant, value):
        return self.rpc("glass/input", {variant: value})

    def move(self, x, y):
        self.event("CursorMoved", {
            "window": self.window()["entity"], "position": [x, y], "delta": None
        })
        self.frames()

    def click(self, x, y):
        self.move(x, y)
        window = self.window()["entity"]
        self.event("MouseButtonInput", {"window": window, "button": "Left", "state": "Pressed"})
        try:
            self.frames()
        finally:
            self.event("MouseButtonInput", {"window": window, "button": "Left", "state": "Released"})
        self.frames()

    def drag(self, x0, y0, x1, y1):
        self.move(x0, y0)
        window = self.window()["entity"]
        self.event("MouseButtonInput", {"window": window, "button": "Left", "state": "Pressed"})
        try:
            self.frames()
            self.move(x1, y1)
        finally:
            self.event("MouseButtonInput", {"window": window, "button": "Left", "state": "Released"})
        self.frames()

    def image(self, path):
        path = pathlib.Path(path).resolve()
        self.event("FileDragAndDrop", {"DroppedFile": {
            "window": self.window()["entity"], "path_buf": str(path)
        }})
        self.frames()
        return {"requested": str(path)}

    def key(self, code, logical):
        value = {"window": self.window()["entity"], "key_code": code,
                 "logical_key": logical, "repeat": False,
                 "text": logical.get("Character") if isinstance(logical, dict) else None}
        self.event("KeyboardInput", {**value, "state": "Pressed"})
        try:
            self.frames()
        finally:
            self.event("KeyboardInput", {**value, "state": "Released"})
        self.frames()

    def capture(self, path):
        ticket = self.rpc("glass/capture")["ticket"]
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            result = self.rpc("glass/capture_status", {"ticket": ticket})
            if result["status"] == "complete":
                png = bytes(result["png"])
                pathlib.Path(path).write_bytes(png)
                return {"path": str(pathlib.Path(path).resolve()), "bytes": len(png)}
            time.sleep(0.05)
        raise TimeoutError("Screenshot did not complete within 15 seconds")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:15704")
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("snapshot")
    commands.add_parser("settings", help="Read the canonical preview settings through official BRP")
    setting = commands.add_parser("set", help="Change a preview setting through official BRP")
    setting.add_argument("path", help="Reflected field path, e.g. style.glare_strength")
    setting.add_argument("value", help="JSON value, e.g. 0.13 or false")
    config = commands.add_parser("config", help="Print or save reusable Rust for the rendered effect")
    config.add_argument("path", nargs="?")
    find = commands.add_parser("find")
    find.add_argument("query", help="Find live nodes by name or displayed text")
    image = commands.add_parser("image", help="Import a local image using a file-drop event")
    image.add_argument("path")
    capture = commands.add_parser("capture")
    capture.add_argument("path")
    for command in ("move", "click"):
        pointer = commands.add_parser(command)
        pointer.add_argument("x", type=float, help="Logical window coordinates")
        pointer.add_argument("y", type=float)
    drag = commands.add_parser("drag", help="Hold and move, useful for sliders")
    for coordinate in ("x0", "y0", "x1", "y1"):
        drag.add_argument(coordinate, type=float)
    key = commands.add_parser("key")
    key.add_argument("code", help="Physical KeyCode, e.g. Space, Enter, KeyA")
    key.add_argument("--logical", help='JSON Key value, e.g. {"Character":"a"}')
    raw = commands.add_parser("rpc")
    raw.add_argument("method")
    raw.add_argument("params", nargs="?", default="null", help="JSON params")
    args = parser.parse_args()
    bridge = Bridge(args.url)
    result = None
    if args.command == "snapshot":
        result = bridge.snapshot()
    elif args.command == "settings":
        result = bridge.settings()
    elif args.command == "set":
        result = bridge.set(args.path, json.loads(args.value))
    elif args.command == "config":
        code = bridge.config()
        if args.path is None:
            print(code, end="")
            return
        path = pathlib.Path(args.path)
        path.write_text(code)
        result = {"path": str(path.resolve())}
    elif args.command == "find":
        query = args.query.casefold()
        result = [node for node in bridge.snapshot()["nodes"]
                  if query in (node["name"] or "").casefold()
                  or query in (node["text"] or "").casefold()]
    elif args.command == "image":
        result = bridge.image(args.path)
    elif args.command == "capture":
        result = bridge.capture(args.path)
    elif args.command == "move":
        bridge.move(args.x, args.y)
    elif args.command == "click":
        bridge.click(args.x, args.y)
    elif args.command == "drag":
        bridge.drag(args.x0, args.y0, args.x1, args.y1)
    elif args.command == "key":
        logical = args.code
        if args.code.startswith("Key") and len(args.code) == 4:
            logical = {"Character": args.code[-1].lower()}
        elif args.code.startswith("Digit") and len(args.code) == 6:
            logical = {"Character": args.code[-1]}
        bridge.key(args.code, json.loads(args.logical) if args.logical else logical)
    elif args.command == "rpc":
        result = bridge.rpc(args.method, json.loads(args.params))
    print(json.dumps(result if result is not None else {"ok": True}, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError, ValueError) as error:
        print(f"UI debug: {error}", file=sys.stderr)
        sys.exit(1)
