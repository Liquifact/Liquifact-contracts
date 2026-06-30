#!/usr/bin/env python3
"""Fix all client.init() calls by adding missing &None parameters."""

import re
import os
from pathlib import Path

# This pattern finds a client.init call ending with &None and );
# The key is: it should end with &None, followed by optional whitespace, then );
# We'll replace it with the same thing but add three &None lines before );

def fix_file(filepath):
    with open(filepath, 'r') as f:
        content = f.read()
    
    original_content = content
    
    # Pattern: Find init calls that end with    &None,\n    );
    # We need to be careful to match the indentation
    pattern = r'(client\.init\([^)]*?)(\s+&None,)(\s+\);)'
    
    def replacer(match):
        prefix = match.group(1)
        none_line = match.group(2)
        closing = match.group(3)
        
        # Count how many &None are already there
        none_count = prefix.count('&None,')
        
        # Need 14 &None total (from legal_hold_clear_delay to allowlist_active)
        # Actually we need to check the full list...
        # Let me just add 3 more &None before closing if there are 14
        
        # Check if this looks like a 14-argument call
        # Count from admin to legal_hold_clear_delay
        lines = prefix.split('\n')
        
        # Simple heuristic: if it ends with exactly one &None before );
        # then it needs 3 more
        if none_count == 14:
            # Add 3 more None values
            indent = '        '  # Standard 8-space indent
            replacement = f'{prefix}{none_line}{closing.replace(")", indent + "&None,\n" + indent + "&None,\n" + indent + "&None," + closing[:-2] + "\n    )")}'
            return replacement
        
        return match.group(0)
    
    # Actually, let me use a simpler approach: count &None in each init block
    # and add what's needed
    
    # Find all init call blocks
    init_pattern = r'client\.init\((.*?)\);'
    
    def fix_init_block(match):
        block = match.group(1)
        # Count &None occurrences
        none_count = block.count('&None,')
        
        # We need 14 None values total
        needed = 14 - none_count
        if needed > 0:
            # Get the indentation from the last line
            lines = block.strip().split('\n')
            last_line = lines[-1]
            indent = re.match(r'^(\s*)', last_line).group(1) if last_line else '        '
            
            # Add missing None values
            additional_nones = ''
            for _ in range(needed):
                additional_nones += f'\n{indent}&None,'
            
            return f'client.init({block}{additional_nones}\n    );'
        
        return match.group(0)
    
    content = re.sub(init_pattern, fix_init_block, content, flags=re.DOTALL)
    
    if content != original_content:
        with open(filepath, 'w') as f:
            f.write(content)
        return True
    return False

test_dir = Path('/workspaces/Liquifact-contracts/escrow/src')

fixed_count = 0
for rs_file in sorted(test_dir.rglob('*.rs')):
    if rs_file.name == 'fix_init_v2.py':
        continue
    if fix_file(rs_file):
        print(f"Fixed {rs_file.relative_to(test_dir.parent)}")
        fixed_count += 1

print(f"\nFixed {fixed_count} files")
