import os
import re

def make_pub(filepath):
    print(f"Processing {filepath}...")
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()

    # 1. Add pub to struct and enum definition
    content = re.sub(r'^(struct |enum )', r'pub \1', content, flags=re.MULTILINE)

    # 2. Add pub to fields inside structs (lines ending with , inside a struct that represent fields)
    # Actually, we can just replace lines that start with 4 spaces followed by a word then colon, 
    # and don't start with pub.
    def replace_field(m):
        if m.group(1).lstrip().startswith('pub '):
            return m.group(0)
        return f"{m.group(1)}pub {m.group(2)}"

    content = re.sub(r'^(\s+)([a-zA-Z0-9_]+\s*:)', replace_field, content, flags=re.MULTILINE)

    # 3. Add pub to functions
    content = re.sub(r'^(\s*async )?fn ([a-zA-Z_])', r'pub \1fn \2', content, flags=re.MULTILINE)

    with open(filepath, 'w', encoding='utf-8') as f:
        f.write(content)


src_dir = r"c:\Users\USER\OneDrive\Desktop\worship-production-interface\src-tauri\src"
files = ["dto.rs", "commands.rs", "audit.rs", "health.rs", "rehearsal.rs"]

for file in files:
    if os.path.exists(os.path.join(src_dir, file)):
        make_pub(os.path.join(src_dir, file))
