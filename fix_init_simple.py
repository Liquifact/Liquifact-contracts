#!/usr/bin/env python3
"""Simpler approach: find the line with ); and add 3 None lines before it."""

import os
from pathlib import Path
import re

def process_file(filepath):
    with open(filepath, 'r') as f:
        lines = f.readlines()
    
    new_lines = []
    in_init = False
    init_start = -1
    
    for i, line in enumerate(lines):
        if 'client.init(' in line:
            in_init = True
            init_start = i
        
        if in_init and line.strip() == ');':
            # Found the end of init call
            # Count how many &None, appear in this init block
            init_text = ''.join(lines[init_start:i+1])
            none_count = init_text.count('&None,')
            
            # We need 14 None values total
            needed = 14 - none_count
            
            if needed > 0:
                # Add the needed lines before the );
                indent = '        '
                for _ in range(needed):
                    new_lines.append(f'{indent}&None,\n')
            
            in_init = False
        
        new_lines.append(line)
    
    new_content = ''.join(new_lines)
    
    with open(filepath, 'r') as f:
        old_content = f.read()
    
    if new_content != old_content:
        with open(filepath, 'w') as f:
            f.write(new_content)
        return True
    return False

test_dir = Path('/workspaces/Liquifact-contracts/escrow/src')
fixed_count = 0

for rs_file in sorted(test_dir.rglob('*.rs')):
    if rs_file.name.startswith('fix_init'):
        continue
    try:
        if process_file(rs_file):
            print(f"Fixed {rs_file.relative_to(test_dir.parent)}")
            fixed_count += 1
    except Exception as e:
        print(f"Error processing {rs_file}: {e}")

print(f"\nTotal files fixed: {fixed_count}")
