#!/usr/bin/env python3
"""Add exactly 3 &None parameters to all init calls - fix version."""

import os
from pathlib import Path

def process_file(filepath):
    with open(filepath, 'r') as f:
        lines = f.readlines()
    
    new_lines = []
    i = 0
    changes_made = False
    
    while i < len(lines):
        line = lines[i]
        
        if 'client.init(' in line:
            # Start of an init block
            init_start = i
            init_block = [line]
            i += 1
            
            # Collect all lines until we find the closing );
            while i < len(lines):
                init_block.append(lines[i])
                if lines[i].strip() == ');':
                    # Found the end
                    break
                i += 1
            
            # Count the number of &None, in this block
            init_text = ''.join(init_block)
            
            # Count the arguments more carefully
            # After "client.init(" we have arguments until ");
            # Let's count by looking at commas at the argument level
            # Actually, just count &None, which should be our option parameters
            # Find the number of &None, in the block (being careful about other occurrences)
            
            # Split by lines and look for lines with &None,
            none_count = 0
            for init_line in init_block:
                if '&None,' in init_line:
                    # Count how many &None, are in this line
                    none_count += init_line.count('&None,')
            
            # We need 14 None values total (all the Option parameters)
            # Required: 14 - currently have = needed
            needed = 14 - none_count
            
            if needed > 0 and needed <= 3:
                # Add the needed lines before the );
                # Find the line with );
                for j in range(len(init_block) - 1, -1, -1):
                    if init_block[j].strip() == ');':
                        # Insert before this line
                        indent = '        '
                        for _ in range(needed):
                            init_block.insert(j, f'{indent}&None,\n')
                        changes_made = True
                        break
            
            # Add the init block to new_lines
            new_lines.extend(init_block)
            i += 1
        else:
            new_lines.append(line)
            i += 1
    
    if changes_made:
        new_content = ''.join(new_lines)
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
        import traceback
        traceback.print_exc()

print(f"\nTotal files fixed: {fixed_count}")
