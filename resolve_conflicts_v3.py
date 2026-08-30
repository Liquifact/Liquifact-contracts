#!/usr/bin/env python3
"""
Resolve all Git merge conflict markers in escrow/src/lib.rs and escrow/src/keys.rs.

Handles nested conflicts correctly by parsing the marker structure.
"""

import re
import sys

def read_file(path):
    with open(path, 'r', encoding='utf-8') as f:
        return f.read()

def write_file(path, content):
    with open(path, 'w', encoding='utf-8') as f:
        f.write(content)

def find_conflict_end(lines, start):
    """Find the matching >>>>>>> for a <<<<<<< at 'start'. Returns the line index."""
    depth = 1
    i = start + 1
    while i < len(lines):
        l = lines[i]
        if l.startswith('<<<<<<< '):
            depth += 1
        elif l.startswith('>>>>>>> '):
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return -1

def parse_conflict(lines, start, end):
    """Parse a conflict region (between start marker and end marker).
    Returns (head_lines, their_lines, separator_line).
    If no separator found, returns (region_lines, [], '')."""
    region = lines[start+1:end]
    
    # First, recursively resolve any nested conflicts in the region
    region = resolve_all_conflicts(region)
    
    # Now find the top-level ======= separator
    depth = 0
    sep_idx = -1
    for idx, l in enumerate(region):
        if l.startswith('<<<<<<< '):
            depth += 1
        elif l.startswith('>>>>>>> '):
            depth -= 1
        elif l.startswith('=======') and depth == 0:
            sep_idx = idx
            break
    
    if sep_idx == -1:
        return (region, [], '')
    
    # Find end marker in the region (last >>>>>>> at depth 0)
    end_marker_idx = -1
    depth = 0
    for idx in range(len(region) - 1, -1, -1):
        l = region[idx]
        if l.startswith('>>>>>>> '):
            depth -= 1
            if depth == -1:
                end_marker_idx = idx
                break
        elif l.startswith('<<<<<<< '):
            depth += 1
    
    head = region[:sep_idx]
    
    if end_marker_idx >= sep_idx + 1:
        their = region[sep_idx+1:end_marker_idx]
    else:
        their = []
    
    return (head, their, region[sep_idx])

def resolve_all_conflicts(lines):
    """Recursively resolve all conflicts in a list of lines."""
    result = []
    i = 0
    while i < len(lines):
        line = lines[i]
        if line.startswith('<<<<<<< '):
            end = find_conflict_end(lines, i)
            if end == -1:
                result.append(line)
                i += 1
                continue
            
            head, their, sep = parse_conflict(lines, i, end)
            
            # Decide which side to keep
            kept = resolve_conflict_choice(head, their, lines, i)
            result.extend(kept)
            
            i = end + 1
        else:
            result.append(line)
            i += 1
    return result

def context_matches(lines, idx, patterns, look_back=0, look_ahead=0):
    """Check if lines[idx] matches patterns considering context."""
    for i in range(max(0, idx-look_back), min(len(lines), idx+look_ahead+1)):
        for pat in patterns:
            if pat in lines[i]:
                return True
    return False

def resolve_conflict_choice(head, their, all_lines, conflict_start_line_index):
    """Choose which side of a conflict to keep based on context."""
    head_text = '\n'.join(head).strip()
    their_text = '\n'.join(their).strip()
    
    # If one side is empty, keep the non-empty side
    if not head_text and their_text:
        return their
    if not their_text and head_text:
        return head
    
    # If both are identical (after stripping), keep either
    if head_text == their_text:
        return head
    
    # Check for comment-only conflicts
    head_is_comment = all(l.strip().startswith('//') or not l.strip() for l in head)
    their_is_comment = all(l.strip().startswith('//') or not l.strip() for l in their)
    
    if head_is_comment and their_is_comment:
        # Keep the longer/more descriptive comment
        if len(their_text) > len(head_text):
            return their
        return head
    
    # If HEAD side is already the merged product (has both sides' content), keep it
    # We determine this heuristically
    
    return head  # Default: keep HEAD side

def fix_keys_rs():
    path = "escrow/src/keys.rs"
    content = read_file(path)
    content = content.rstrip()
    if content.endswith('>>>>>>> pr-982'):
        content = content[:content.rfind('>>>>>>> pr-982')].rstrip()
    write_file(path, content + '\n')
    print("Fixed escrow/src/keys.rs")

def fix_lib_rs():
    path = "escrow/src/lib.rs"
    content = read_file(path)
    lines = content.split('\n')
    
    # Apply targeted replacements for specific conflicts
    # These are determined by our analysis of each conflict
    
    # First, fix the simple conflicts with exact string matches
    replacements = []
    
    # Conflict 1: #![no_std] attribute - HEAD has nested merge, everything resolves to #![cfg_attr(not(test), no_std)]
    r1_old = '<<<<<<< HEAD\n<<<<<<< HEAD\n#![no_std]\n=======\n#![cfg_attr(not(test), no_std)]\n=======\n#![cfg_attr(not(test), no_std)]\n>>>>>>> 973c262 (feat(collateral): add admin parameter setter)'
    r1_new = '#![cfg_attr(not(test), no_std)]'
    if r1_old in content:
        content = content.replace(r1_old, r1_new)
        print("Applied conflict 1")
    
    # Conflict 2: Docstring closing - just leftover markers
    r2_old = '=======\n>>>>>>> pr-982'
    if r2_old in content:
        # Need to be careful - this pattern may appear multiple times
        # Only do this once
        content = content.replace(r2_old, '', 1)
        print("Applied conflict 2a")
    
    # Actually, let me just use a completely different approach.
    # Let me write the file from scratch with the correct content.
    # But the file is 6000+ lines...
    
    # Let me try line-by-line processing instead
    write_file(path, content)
    print("Partial fixes applied to lib.rs")

if __name__ == '__main__':
    print("This script handles the overall approach. Let me use a direct line-based approach instead.")
    print("See resolve_conflicts_v4.py for the real solution.")
