import json
import re
import tomllib

print("package.json   :", json.load(open("package.json", encoding="utf-8"))["version"])
print("tauri.conf.json:", json.load(open("src-tauri/tauri.conf.json", encoding="utf-8"))["version"])
print("Cargo.toml     :", tomllib.load(open("src-tauri/Cargo.toml", "rb"))["package"]["version"])
lock = open("src-tauri/Cargo.lock", encoding="utf-8").read()
m = re.search(r'name = "tomatossh"\r?\nversion = "([^"]+)"', lock)
print("Cargo.lock     :", m.group(1) if m else "NOT FOUND")
