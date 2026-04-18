import os
import re

base_dir = r"c:\Users\USER\OneDrive\Desktop\worship-production-interface\src-tauri\src"

def clean_file(filename):
    path = os.path.join(base_dir, filename)
    with open(path, 'r', encoding='utf-8') as f:
        content = f.read()

    if filename == "dto.rs":
        # Remove the injected header completely
        # The injected header starts with `use std::collections` and ends with `use crate::rehearsal::*;\n\n`
        content = re.sub(r'^use std::collections.*?use crate::rehearsal::\*;\n\n', '', content, flags=re.DOTALL)
    else:
        # Remove specific explicit imports that conflict with `use crate::*;`
        content = re.sub(r'^use tauri::\{.*?};\n', '', content, flags=re.MULTILINE)
        content = re.sub(r'^use crate::(?:\{DesktopState\}|DesktopState);\n', '', content, flags=re.MULTILINE)
        content = re.sub(r'^use tauri::AppHandle;\n', '', content, flags=re.MULTILINE)
        content = re.sub(r'^use crate::dto::\*;\n', '', content, flags=re.MULTILINE) # Since we injected this manually in the header too

    with open(path, 'w', encoding='utf-8') as f:
        f.write(content)

for f in ["dto.rs", "commands.rs", "audit.rs", "health.rs", "rehearsal.rs"]:
    clean_file(f)
