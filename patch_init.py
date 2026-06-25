from pathlib import Path
root = Path('escrow/src')
patches = 0
for path in root.rglob('*.rs'):
    text = path.read_text()
    out = []
    i = 0
    while True:
        idx = text.find('client.init(', i)
        if idx == -1:
            out.append(text[i:])
            break
        out.append(text[i:idx])
        j = idx + len('client.init(')
        depth = 1
        in_string = False
        escape = False
        while j < len(text) and depth > 0:
            c = text[j]
            if in_string:
                if escape:
                    escape = False
                elif c == '\\':
                    escape = True
                elif c == '"':
                    in_string = False
            else:
                if c == '"':
                    in_string = True
                elif c == '(':
                    depth += 1
                elif c == ')':
                    depth -= 1
            j += 1
        if depth != 0:
            out.append(text[idx:])
            break
        call_body = text[idx:j]
        if call_body.endswith(');'):
            inner = call_body[:-2]
            if ', &None' not in inner[-20:]:
                out.append(inner)
                out.append(', &None);')
                patches += 1
            else:
                out.append(call_body)
        else:
            out.append(call_body)
        i = j
    new_text = ''.join(out)
    if new_text != text:
        path.write_text(new_text)
print(f'Patched {patches} client.init call(s)')
