#!/usr/bin/env python3
"""Fix all client.init() calls to include the two new parameters."""

import re
import os
from pathlib import Path

# Pattern to find init calls: look for the closing of init() with &None parameters
pattern = re.compile(
    r'(client\.init\([^)]*?)([\s]*&None,[\s]*&None,[\s]*\);)',
    re.DOTALL
)

# The replacement adds the two missing &None parameters
replacement = r'\1        &None,\n        &None,\n    );'

test_dir = Path('/workspaces/Liquifact-contracts/escrow/src')

for rs_file in test_dir.rglob('*.rs'):
    with open(rs_file, 'r') as f:
        content = f.read()
    
    # Count matches before
    matches_before = len(pattern.findall(content))
    
    if matches_before > 0:
        # Replace all occurrences
        new_content = pattern.sub(replacement, content)
        
        # Write back
        with open(rs_file, 'w') as f:
            f.write(new_content)
        
        print(f"Fixed {matches_before} calls in {rs_file.relative_to(test_dir.parent)}")

print("Done!")
