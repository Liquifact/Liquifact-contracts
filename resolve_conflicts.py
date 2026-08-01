#!/usr/bin/env python3
"""Resolve all Git merge conflict markers in the Liquifact escrow contract files."""

import re
import sys

def resolve_keys_rs():
    path = "escrow/src/keys.rs"
    with open(path, "r") as f:
        content = f.read()
    
    # Remove trailing >>>>>>> pr-982
    content = content.rstrip()
    if content.endswith(">>>>>>> pr-982"):
        content = content[:content.rfind(">>>>>>> pr-982")].rstrip()
    
    with open(path, "w") as f:
        f.write(content + "\n")
    print(f"Fixed {path}")

def resolve_conflict_simple(content, marker_pairs):
    """Resolve conflicts by specifying (start_marker_line_content, end_marker_line_content, replacement) tuples."""
    for old, new in marker_pairs:
        if old in content:
            content = content.replace(old, new)
        else:
            print(f"  WARNING: pattern not found: {old[:60]}...")
    return content

def resolve_lib_rs():
    path = "escrow/src/lib.rs"
    with open(path, "r") as f:
        content = f.read()
    
    lines = content.split('\n')
    result = []
    i = 0
    conflict_count = 0
    
    while i < len(lines):
        line = lines[i]
        
        # Detect conflict markers - all the different patterns
        if line.startswith('<<<<<<< HEAD') or line.startswith('<<<<<<< ') or line.startswith('=======') or line.startswith('>>>>>>> '):
            conflict_count += 1
            print(f"Processing conflict area starting at line {i+1}: {line[:40]}")
            
            # Parse the conflict. We need to handle nested conflicts.
            # Strategy: find the matching END of this conflict, then analyze inside
            
            # Find the end of this conflict
            depth = 1  # We're entering one level
            j = i + 1
            end_line = -1
            while j < len(lines):
                l = lines[j]
                if l.startswith('<<<<<<< '):
                    depth += 1
                elif l.startswith('>>>>>>> '):
                    depth -= 1
                    if depth == 0:
                        end_line = j
                        break
                j += 1
            
            if end_line == -1:
                print(f"  ERROR: Unmatched conflict marker at line {i+1}, stopping")
                result.extend(lines[i:])
                break
            
            # Extract the conflict region (between start marker and end marker)
            region = lines[i+1:end_line]
            
            # Recursively resolve any nested conflicts in the region
            region_str = '\n'.join(region)
            region_str_resolved = resolve_all_conflicts(region_str)
            region = region_str_resolved.split('\n')
            
            # Now parse the top-level conflict
            # Find the top-level ======= separator
            sep_idx = -1
            depth = 0
            for idx, l in enumerate(region):
                if l.startswith('<<<<<<< '):
                    depth += 1
                elif l.startswith('>>>>>>> '):
                    depth -= 1
                elif l.startswith('=======') and depth == 0:
                    sep_idx = idx
                    break
            
            # Get the source branch name from the markers
            start_marker = line
            # Find the end marker (might be nested)
            end_marker_idx = -1
            for idx in range(len(region) - 1, -1, -1):
                if region[idx].startswith('>>>>>>> '):
                    end_marker_idx = idx
                    break
            
            if sep_idx == -1:
                print(f"  WARNING: No ======= found at line {i+1}, keeping HEAD side")
                resolved = region
            else:
                head_side = region[:sep_idx]
                sep_line = region[sep_idx]
                their_side_start = sep_idx + 1
                if end_marker_idx >= their_side_start:
                    their_side = region[their_side_start:end_marker_idx]
                else:
                    their_side = []
                
                # Determine which side to keep based on context
                # Join both sides for analysis
                head_text = '\n'.join(head_side).strip()
                their_text = '\n'.join(their_side).strip()
                
                print(f"    HEAD side ({len(head_side)} lines): {head_text[:80]}...")
                print(f"    Their side ({len(their_side)} lines): {their_text[:80]}...")
                
                # Specific resolution rules based on context
                resolved = resolve_conflict_decision(head_text, their_text, head_side, their_side, result, lines, i)
            
            result.extend(resolved)
            
            # Skip past the end marker
            i = end_line + 1
        else:
            result.append(line)
            i += 1
    
    # Join and write
    new_content = '\n'.join(result)
    
    # Also fix any remaining duplicate functions
    
    with open(path, "w") as f:
        f.write(new_content)
    print(f"\nProcessed {conflict_count} conflict areas in lib.rs")
    return new_content

def resolve_all_conflicts(text):
    """Recursively resolve all conflicts in a text."""
    lines = text.split('\n')
    result = []
    i = 0
    
    while i < len(lines):
        line = lines[i]
        if line.startswith('<<<<<<< '):
            # Find the matching end
            depth = 1
            j = i + 1
            end_idx = -1
            while j < len(lines):
                l = lines[j]
                if l.startswith('<<<<<<< '):
                    depth += 1
                elif l.startswith('>>>>>>> '):
                    depth -= 1
                    if depth == 0:
                        end_idx = j
                        break
                j += 1
            
            if end_idx == -1:
                result.append(line)
                i += 1
                continue
            
            region = lines[i+1:end_idx]
            region_str = '\n'.join(region)
            resolved_region = resolve_all_conflicts(region_str)
            region = resolved_region.split('\n')
            
            # Find separator
            sep_idx = -1
            depth = 0
            for idx, l in enumerate(region):
                if l.startswith('<<<<<<< '):
                    depth += 1
                elif l.startswith('>>>>>>> '):
                    depth -= 1
                elif l.startswith('=======') and depth == 0:
                    sep_idx = idx
                    break
            
            if sep_idx == -1:
                result.extend(region)
            else:
                head_side = region[:sep_idx]
                their_side_start = sep_idx + 1
                
                end_marker_idx = -1
                for idx in range(len(region) - 1, -1, -1):
                    if region[idx].startswith('>>>>>>> '):
                        end_marker_idx = idx
                        break
                
                if end_marker_idx >= their_side_start:
                    their_side = region[their_side_start:end_marker_idx]
                else:
                    their_side = []
                
                head_text = '\n'.join(head_side).strip()
                their_text = '\n'.join(their_side).strip()
                
                resolved = resolve_simple_conflict(head_text, their_text, head_side, their_side)
                result.extend(resolved)
            
            i = end_idx + 1
        else:
            result.append(line)
            i += 1
    
    return '\n'.join(result)

def resolve_simple_conflict(head_text, their_text, head_side, their_side):
    """Decide which side to keep for a simple (non-contextual) conflict."""
    # If both sides are identical, keep either
    if head_text == their_text:
        return head_side
    
    # If one side is empty, keep the non-empty side
    if not head_text:
        return their_side
    if not their_text:
        return head_side
    
    # Default: keep HEAD side (contains merge results)
    return head_side

def resolve_conflict_decision(head_text, their_text, head_side, their_side, result_context, all_lines, current_line_idx):
    """Make context-aware conflict resolution decisions."""
    
    # Rule 1: Line 1-8 - #![no_std] attribute
    if '#![no_std]' in head_text or '#![cfg_attr' in head_text:
        if '#![cfg_attr(not(test), no_std)]' in head_text or '#![cfg_attr(not(test), no_std)]' in their_text:
            if '#![cfg_attr(not(test), no_std)]' in their_text:
                return ['#![cfg_attr(not(test), no_std)]']
            return head_side
    
    # Rule 2: Module declarations (pub mod)
    if any('pub mod' in l for l in head_side + their_side) and 'DataKey' in head_text + their_text:
        # Keep the version with keys module and DataKey re-export
        if 'pub mod keys' in their_text and 'pub use keys' in their_text:
            return their_side  # pr-982 has keys module
        elif 'pub mod keys' in head_text and 'pub use keys' in head_text:
            return head_side
        # If no keys module, add the full import
        result = []
        if 'pub mod external_calls' in head_text or 'pub mod external_calls' in their_text:
            pass  # external_calls will be in the result naturally
    
    # Rule 3: Schema version docstring area
    if 'Schema version' in head_text or 'Schema version' in their_text:
        # These are just docstring closing markers - remove them
        return []
    
    # Rule 4: Constants section (lines ~270-300)
    if 'MAX_PAUSE_READ_PAGE' in head_text or 'MAX_PAUSE_READ_PAGE' in their_text:
        if 'MAX_PAUSE_READ_PAGE' in head_text:
            return head_side  # HEAD has pause constants + settlement constants
        return their_side
    
    # Rule 5: DataKey enum or "Storage keys are defined in keys.rs"
    if 'DataKey' in [l.strip() for l in head_side[:5] + head_side[-5:]] and any('/// Storage discriminator' in l for l in head_side):
        # DataKey enum definition - keep the comment that it's in keys.rs
        if 'Storage keys are defined in keys.rs' in their_text:
            return their_side
        return head_side
    
    # Rule 6: PauseRecord and YieldTierPreview structs
    if 'PauseRecord' in head_text or 'YieldTierPreview' in head_text:
        if head_text:
            return head_side
    
    # Rule 7: Comment differences
    if head_text.startswith('//') and their_text.startswith('//'):
        # For comment-only conflicts, pick the more descriptive one
        if len(head_text) >= len(their_text):
            return head_side
        return their_side
    
    # Rule 8: set_yield_tiers admin method
    if 'fn set_yield_tiers' in head_text or 'fn set_yield_tiers' in their_text:
        if head_text:
            return head_side
        return their_side
    
    # Rule 9: collateral-related functions
    if 'collateral_pledge_get' in head_text or 'collateral_pledge_get' in their_text:
        # Prefer centralized helpers (Self::collateral_pledge_get)
        if 'Self::collateral_pledge_get' in head_text or 'Self::collateral_pledge_get' in their_text:
            if 'Self::collateral_pledge_get' in head_text:
                return head_side
            # Their side has it too
            return their_side if their_text else head_side
    
    if 'collateral_pledge_remove' in head_text or 'collateral_pledge_remove' in their_text:
        if 'Self::collateral_pledge_remove' in head_text:
            return head_side
        if 'Self::collateral_pledge_remove' in their_text:
            return their_side
    
    if 'collateral_pledge_set' in head_text or 'collateral_pledge_set' in their_text:
        if 'Self::collateral_pledge_set' in head_text:
            return head_side
        if 'Self::collateral_pledge_set' in their_text:
            return their_side
    
    # Rule 10: get_collateral_records - avoid duplicate, prefer the one using paginate_window helper
    if 'fn get_collateral_records' in head_text and 'fn get_collateral_records' in their_text:
        if 'paginate_window' in head_text:
            return head_side
        if 'paginate_window' in their_text:
            return their_side
    
    # Rule 11: Duplicate get_collateral_records (should be removed)
    if 'fn get_collateral_records' in head_text and not their_text:
        # This is a duplicate that appears in the wrong place
        # Keep their side (empty) to remove the duplicate
        return their_side
    
    # Rule 12: fund_impl vs unfund conflict (lines ~6431-6500)
    if 'simple_fund' in head_text and 'funding_token_or_fail' in their_text:
        # HEAD is fund_impl, their side is unfund body
        # Keep fund_impl (HEAD) since we're in the fund_impl function
        return head_side
    
    if 'simple_fund' in head_text or 'EscrowUnfunded' in head_text or 'simple_fund' in their_text or 'EscrowUnfunded' in their_text:
        if 'simple_fund' in head_text:
            return head_side
        return their_side
    
    # Rule 13: set_settlement_limit vs get_distributed_principal + ReconciliationView
    if 'fn set_settlement_limit' in head_text and 'fn get_distributed_principal' in their_text:
        # Merge both - keep HEAD side (settlement_limit) then add their side's content
        # But we need to be careful about what comes after
        return head_side + [''] + their_side
    
    # Rule 14: yield tier setter
    if 'fn set_yield_tiers' in head_text:
        return head_side
    if 'fn set_yield_tiers' in their_text:
        return their_side
    
    # Generic fallback: prefer HEAD (contains merged code)
    if head_text == their_text:
        return head_side
    if not their_text:
        return head_side
    if not head_text:
        return their_side
    
    print(f"  USING DEFAULT: keeping HEAD side ({len(head_side)} lines)")
    return head_side

def remove_duplicate_paginate_window(content):
    """Remove the duplicate paginate_window function."""
    lines = content.split('\n')
    result = []
    i = 0
    found_first = False
    
    while i < len(lines):
        line = lines[i]
        
        # Check for fn paginate_window
        stripped = line.strip()
        if 'fn paginate_window' in stripped and not found_first:
            found_first = True
            result.append(line)
            i += 1
        elif 'fn paginate_window' in stripped and found_first:
            # Skip this duplicate and its body
            print(f"  Skipping duplicate paginate_window at line (approx)")
            i += 1
            # Skip the function body (increase indentation body)
            while i < len(lines):
                if lines[i].strip().startswith('fn ') and 'paginate_window' not in lines[i]:
                    break
                if i + 1 < len(lines) and not lines[i+1].startswith(' ') and not lines[i+1].startswith('\t') and lines[i+1].strip():
                    # Check if next non-empty, non-indented line is a new function/definition
                    # Actually, paginate_window body is inside an impl block, so check for closing patterns
                    pass
                i += 1
        else:
            result.append(line)
            i += 1
    
    return '\n'.join(result)

if __name__ == '__main__':
    print("=== Resolving keys.rs ===")
    resolve_keys_rs()
    
    print("\n=== Resolving lib.rs ===")
    new_content = resolve_lib_rs()
    
    print("\nDone!")
