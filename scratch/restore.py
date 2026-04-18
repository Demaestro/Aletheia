import os
import re
import json

log_path = r'C:\Users\USER\.gemini\antigravity\brain\9c451da7-0813-402c-83aa-0dc847172207\.system_generated\logs\overview.txt'
with open(log_path, 'r', encoding='utf-8', errors='ignore') as f:
    text = f.read()

files_to_restore = ['commands.rs', 'audit.rs', 'health.rs', 'rehearsal.rs', 'dto.rs']

for fname in files_to_restore:
    print(f"--- Restoring {fname} ---")
    
    # We want to find the first time 'write_to_file' was called for this file. Or any time.
    # The log might contain: call:default_api:write_to_file{... "TargetFile": "...commands.rs", "CodeContent": "..." }
    # Or something like that. We can use regex to find default_api:write_to_file calls.
    
    # Let's extract all write_to_file tool calls
    # Usually they look like JSON arguments or formatted string.
    # The text usually has: "TargetFile":"c:\\Users\\USER\\OneDrive\\Desktop\\worship-production-interface\\src-tauri\\src\\commands.rs"
    # Followed by "CodeContent":"..."
    
    # Just look for the substring representation
    # Alternatively, find the content in the original lib.rs by seeing the split logic? 
    # Let's try to find TargetFile manually.
    
    pattern = r'write_to_file\{.*?"TargetFile":"[^"]*?' + re.escape(fname) + r'".*?"CodeContent":"(.*?)"'
    matches = list(re.finditer(pattern, text, re.DOTALL))
    
    if not matches:
        # maybe single quotes or no quotes in the log overview?
        # let's try finding CodeContent: followed by ```rust
        # the overview.txt format usually is:
        # call:default_api:write_to_file
        # {
        #   "TargetFile": "...commands.rs",
        #   "CodeContent": "..."
        # }
        pattern2 = r'TargetFile[^\n]*?' + re.escape(fname) + r'.*?CodeContent.*?[\'"](.*?)[\'"]\s*(?:,|})'
        matches2 = list(re.finditer(pattern2, text, re.DOTALL))
        if matches2:
            print(f"Found {len(matches2)} matches using approach 2")
            latest_match = matches2[-1].group(1)
            # unescape JSON string
            try:
                latest_match = json.loads('"' + latest_match + '"')
            except:
                pass
            with open(os.path.join('src-tauri/src', fname), 'w', encoding='utf-8') as out:
                out.write(latest_match)
            print(f"Restored {fname}! Size: {len(latest_match)}")
        else:
            print(f"No writes found for {fname}")
    else:
        print(f"Found {len(matches)} matches using approach 1")
        latest_match = matches[-1].group(1)
        try:
            latest_match = json.loads('"' + latest_match + '"')
        except:
            pass
        with open(os.path.join('src-tauri/src', fname), 'w', encoding='utf-8') as out:
            out.write(latest_match)
        print(f"Restored {fname}! Size: {len(latest_match)}")

