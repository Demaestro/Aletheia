import os
import re

def undo_pub(filepath):
    print(f"Processing {filepath}...")
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()

    # We want to revert fields back to normal in all non-dto files
    # E.g., `    pub state: &DesktopState,` -> `    state: &DesktopState,`
    # The regex originally was: `^(\s+)pub ([a-zA-Z0-9_]+\s*:)`
    content = re.sub(r'^(\s+)pub ([a-zA-Z0-9_]+\s*:)', r'\1\2', content, flags=re.MULTILINE)

    with open(filepath, 'w', encoding='utf-8') as f:
        f.write(content)

src_dir = r"c:\Users\USER\OneDrive\Desktop\worship-production-interface\src-tauri\src"
files = ["commands.rs", "audit.rs", "health.rs", "rehearsal.rs"]

for file in files:
    path = os.path.join(src_dir, file)
    if os.path.exists(path):
        undo_pub(path)
