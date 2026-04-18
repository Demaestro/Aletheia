import re
import sys

def modify():
    try:
        with open('src-tauri/src/lib.rs', 'r') as f:
            content = f.read()

        # Find all structs ending with Dto
        pattern = r'(#\[derive\([^)]+\)\])\n(#\[serde\([^)]+\)\]\n)?(struct \w+Dto(?: \{|;))'
        
        def replacer(match):
            derive_line = match.group(1)
            serde_line = match.group(2) or ''
            struct_line = match.group(3)
            
            # Skip if already has TS
            if 'TS' in derive_line:
                return match.group(0)
                
            # Add ts_rs::TS to derive
            new_derive = derive_line.replace(')', ', ts_rs::TS)')
            export_attr = '#[ts(export, export_to = "../../src/gen/")]\n'
            
            return f"{new_derive}\n{export_attr}{serde_line}{struct_line}"

        new_content = re.sub(pattern, replacer, content)
        
        # Handle RedactionSummary field in SupportBundleExportDto
        # If the compiler complains about RedactionSummary not implementing TS,
        # we tell ts-rs what TS type to generate for it.
        new_content = new_content.replace('pub redaction_summary: RedactionSummary,', '#[ts(type = "any")]\npub redaction_summary: RedactionSummary,')
        new_content = new_content.replace('    redaction_summary: RedactionSummary,', '    #[ts(type = "any")]\n    redaction_summary: RedactionSummary,')

        with open('src-tauri/src/lib.rs', 'w') as f:
            f.write(new_content)
        print("lib.rs modified successfully.")
    except Exception as e:
        print(f"Error: {e}")

if __name__ == "__main__":
    modify() 
