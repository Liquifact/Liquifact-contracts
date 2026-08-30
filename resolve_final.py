#!/usr/bin/env python3
"""
Process lib.rs line by line, properly handling nested conflict markers.
For each conflict, examine the full context and apply the correct resolution.
"""

def read_file(path):
    with open(path, 'r', encoding='utf-8') as f:
        return f.readlines()

def write_file(path, lines):
    with open(path, 'w', encoding='utf-8') as f:
        f.writelines(lines)

def find_matching_end(lines, start):
    """Find the matching >>>>>>> for <<<<<<< at start. Returns line index."""
    depth = 1
    for i in range(start + 1, len(lines)):
        l = lines[i]
        if l.startswith('<<<<<<< '):
            depth += 1
        elif l.startswith('>>>>>>> '):
            depth -= 1
            if depth == 0:
                return i
    return -1

def is_marker_line(l):
    return l.startswith('<<<<<<< ') or l.startswith('=======') or l.startswith('>>>>>>> ')

def process_file(path):
    lines = read_file(path)
    result = []
    i = 0
    total = len(lines)
    
    while i < total:
        line = lines[i]
        
        if line.startswith('<<<<<<< '):
            end = find_matching_end(lines, i)
            if end == -1:
                result.append(line)
                i += 1
                continue
            
            # Extract the conflict region (excluding markers)
            conflict_region = lines[i+1:end]
            
            # Recursively process inner conflicts first
            processed_region = process_region(conflict_region)
            
            # Now resolve this level of conflict
            resolved = choose_side(processed_region, lines, i)
            result.extend(resolved)
            
            i = end + 1
        else:
            result.append(line)
            i += 1
    
    return result

def process_region(lines):
    """Recursively process all nested conflicts in a region."""
    result = []
    i = 0
    while i < len(lines):
        line = lines[i]
        if line.startswith('<<<<<<< '):
            end = find_matching_end(lines, i)
            if end == -1:
                result.append(line)
                i += 1
                continue
            inner_region = lines[i+1:end]
            processed = process_region(inner_region)
            resolved = choose_side(processed, lines, i)
            result.extend(resolved)
            i = end + 1
        else:
            result.append(line)
            i += 1
    return result

def choose_side(lines, all_lines, original_line_num):
    """Choose which side of a conflict to keep.
    
    lines is the region between <<<<<<< and >>>>>>> (exclusive), with inner
    conflicts already resolved.
    
    Returns the lines to keep.
    """
    # Find the ======= separator at the top level
    depth = 0
    sep_idx = -1
    for i, l in enumerate(lines):
        if l.startswith('<<<<<<< '):
            depth += 1
        elif l.startswith('>>>>>>> '):
            depth -= 1
        elif l.startswith('=======') and depth == 0:
            sep_idx = i
            break
    
    if sep_idx == -1:
        # No separator found - this is just content
        return lines
    
    head = lines[:sep_idx]
    
    # Find where "their" section ends (last >>>>>>> at depth 0)
    their_start = sep_idx + 1
    their_end = len(lines)
    depth = 0
    for i in range(len(lines) - 1, their_start - 1, -1):
        l = lines[i]
        if l.startswith('>>>>>>> '):
            depth -= 1
            if depth == -1:
                their_end = i
                break
        elif l.startswith('<<<<<<< '):
            depth += 1
    
    their = lines[their_start:their_end] if their_end > their_start else []
    
    head_text = ''.join(head).strip()
    their_text = ''.join(their).strip()
    
    # Resolution rules based on context
    # Join the full context around this conflict for analysis
    context_before = ''.join(all_lines[max(0, original_line_num-5):original_line_num]).strip()
    
    # Rule: #![no_std] attribute - keep #![cfg_attr(not(test), no_std)]
    if '#![no_std]' in head_text or '#![cfg_attr(not(test), no_std)]' in context_before:
        # Both sides ultimately agree on #![cfg_attr(not(test), no_std)]
        if any('cfg_attr' in l for l in head):
            return head
        if any('cfg_attr' in l for l in their):
            return their
    
    # Rule: module declarations - keep keys module (has more complete exports)
    if 'pub mod' in head_text and 'pub mod' in their_text:
        # Keep the one with 'keys' module (it's the more complete version)
        if any('pub mod keys' in l for l in head):
            return head
        if any('pub mod keys' in l for l in their):
            return their
        # If neither has keys, keep non-empty
        if head and not their:
            return head
        if their and not head:
            return their
        return head
    
    # Rule: comment-only conflicts - keep the more descriptive one
    head_only_comments = all(l.startswith('//') or not l.strip() for l in head if l.strip())
    their_only_comments = all(l.startswith('//') or not l.strip() for l in their if l.strip())
    
    # Rule: DataKey enum vs comment - keep the comment (DataKey moved to keys.rs)
    if ('Storage keys are defined in keys.rs' in their_text or 
        'Storage keys are defined in keys.rs' in head_text):
        if 'Storage keys are defined in keys.rs' in their_text:
            return their
        return head
    
    # Rule: Keep non-empty over empty
    if not their_text and head_text:
        return head
    if not head_text and their_text:
        return their
    
    # Rule: For identical content, keep either
    if head_text == their_text:
        return head
    
    # Rule: Prefer centralized helpers (Self::collateral_pledge_*) over direct storage access
    if 'Self::collateral_pledge' in head_text and 'env.storage()' in their_text:
        return head
    if 'Self::collateral_pledge' in their_text and 'env.storage()' in head_text:
        return their
    
    # Rule: For setter functions, keep the HEAD side (has complete defs)
    if any('fn ' in l for l in head) and not their:
        return head
    
    # Default: keep HEAD
    return head

# Main
print("Processing lib.rs...")
lib_lines = process_file("escrow/src/lib.rs")
write_file("escrow/src/lib.rs", lib_lines)
print(f"Wrote {len(lib_lines)} lines to lib.rs")

# Fix keys.rs
print("Processing keys.rs...")
keys_content = open("escrow/src/keys.rs", 'r', encoding='utf-8').read()
keys_content = keys_content.rstrip()
if keys_content.endswith('>>>>>>> pr-982'):
    keys_content = keys_content[:keys_content.rfind('>>>>>>> pr-982')].rstrip()
with open("escrow/src/keys.rs", 'w', encoding='utf-8') as f:
    f.write(keys_content + '\n')
print("Fixed keys.rs")

# Verify
import subprocess
result = subprocess.run(['grep', '-rn', '<<<<<<<\\|=======\\|>>>>>>>', 'escrow/src/'], 
                       capture_output=True, text=True, timeout=10)
# Filter out comment lines (starting with //)
real_markers = [l for l in result.stdout.split('\n') 
                if l.strip() and not '// ====' in l and not '// =========' in l]
print(f"\nRemaining markers: {len(real_markers)}")
for m in real_markers:
    print(f"  {m}")
