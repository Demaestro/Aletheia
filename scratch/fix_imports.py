import os
import re

lib_path = r"c:\Users\USER\OneDrive\Desktop\worship-production-interface\src-tauri\src\lib.rs"
with open(lib_path, 'r', encoding='utf-8') as f:
    lib_content = f.read()

# Extract all imports at the top of lib.rs
match = re.search(r'^(use .*?;[\s\S]*?)mod vault;', lib_content, re.MULTILINE)
if match:
    imports = match.group(1).strip()
else:
    imports = ""

mods = ["dto.rs", "commands.rs", "audit.rs", "health.rs", "rehearsal.rs"]
base_dir = r"c:\Users\USER\OneDrive\Desktop\worship-production-interface\src-tauri\src"

for mod in mods:
    mod_path = os.path.join(base_dir, mod)
    if os.path.exists(mod_path):
        with open(mod_path, 'r', encoding='utf-8') as f:
            content = f.read()
        
        # Prepend the imports, ensuring we don't duplicate them multiple times
        # Also add `use crate::*;` and `use crate::dto::*;` etc.
        header = imports + "\nuse crate::*;\nuse crate::dto::*;\nuse crate::commands::*;\nuse crate::audit::*;\nuse crate::health::*;\nuse crate::rehearsal::*;\n\n"
        
        # Only inject if not already injected
        if "use crate::rehearsal::*;" not in content:
            # We must make sure it stays at the top
            content = header + content
            with open(mod_path, 'w', encoding='utf-8') as f:
                f.write(content)

# In lib.rs we need to make sure we import everything from commands, to fix macro errors
if "use crate::commands::*;" not in lib_content:
    lib_content = lib_content.replace(
        "pub mod rehearsal;", 
        "pub mod rehearsal;\n\nuse crate::commands::*;\nuse crate::dto::*;\nuse crate::audit::*;\nuse crate::health::*;\nuse crate::rehearsal::*;\n"
    )
    with open(lib_path, 'w', encoding='utf-8') as f:
        f.write(lib_content)
