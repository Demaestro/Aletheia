import os, re

files=['commands.rs', 'audit.rs', 'health.rs', 'rehearsal.rs']
cmd_map = {}
for f in files:
    m = f[:-3]
    with open('src-tauri/src/'+f, 'r', encoding='utf-8') as file:
        content = file.read()
    
    # find lines with tauri::command and the next line with fn
    for match in re.finditer(r'#\[tauri::command\]\s+(?:pub\s+)?(?:async\s+)?fn\s+([a-zA-Z0-9_]+)', content):
        name = match.group(1)
        cmd_map[name] = m

print(cmd_map)

# Rewrite lib.rs
lib_path = 'src-tauri/src/lib.rs'
with open(lib_path, 'r', encoding='utf-8') as f:
    lib = f.read()

def replacer(match):
    # the middle part is a comma separated list of functions
    inner = match.group(1)
    parts = inner.split(',')
    new_parts = []
    for part in parts:
        clean = part.strip()
        if not clean:
            continue
        # if already has module path, skip? Assumes no module path yet
        if clean in cmd_map:
            mod = cmd_map[clean]
            new_parts.append(f'\n            {mod}::{clean}')
        else:
            new_parts.append(f'\n            {clean}')
    
    return 'tauri::generate_handler![' + ','.join(new_parts) + '\n        ]'

lib = re.sub(r'tauri::generate_handler\!\[\s*([^\]]+)\s*\]', replacer, lib)
with open(lib_path, 'w', encoding='utf-8') as f:
    f.write(lib)
print('lib.rs rewritten successfully.')
