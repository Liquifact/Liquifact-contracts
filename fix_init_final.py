#!/usr/bin/env python3
"""Add exactly 3 &None parameters to all init calls."""

import re
from pathlib import Path

def fix_file(filepath):
    with open(filepath, 'r') as f:
        content = f.read()
    
    # Pattern: Match init blocks - specifically look for the closing &None followed by );
    # We want to add exactly 3 more &None before the );
    
    # This pattern matches init blocks that need fixing
    # It looks for blocks ending with at least one &None, then );
    pattern = r'(client\.init\([^)]*?&None,)\s*(\);)'
    
    def replacer(match):
        prefix = match.group(1)
        closing = match.group(2)
        # Count how many &None are already there
        none_count = prefix.count('&None,')
        
        # We need 14 total None values
        # If we have fewer, add what's needed
        # Get the indentation
        lines = prefix.split('\n')
        indent_match = re.match(r'^(\s+)', lines[-1]) if lines[-1].strip() else None
        indent = indent_match.group(1) if indent_match else '        '
        
        # We'll add exactly 3 more
        # Let's add them after the last &None,
        replacement = f'{prefix}\n{indent}&None,\n{indent}&None,\n{indent}&None,\n    {closing[:-2]}\n    );'
        return replacement
    
    new_content = re.sub(pattern, replacer, content, flags=re.DOTALL)
    
    if new_content != content:
        with open(filepath, 'w') as f:
            f.write(new_content)
        return True
    return False

test_dir = Path('/workspaces/Liquifact-contracts/escrow/src')

fixed_count = 0
for rs_file in sorted(test_dir.rglob('*.rs')):
    if rs_file.name.startswith('fix_init'):
        continue
    if fix_file(rs_file):
        rel_path = rs_file.relative_to(test_dir.parent)
        print(f"Fixed {rel_path}")
        fixed_count += 1

print(f"\nFixed {fixed_count} files")
