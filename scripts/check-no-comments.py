import pathlib
import re
import subprocess
import sys


def rust_comments(text):
    found = []
    i = 0
    line = 1
    length = len(text)
    while i < length:
        c = text[i]
        if c == "\n":
            line += 1
            i += 1
            continue
        if text.startswith("//", i) or text.startswith("/*", i):
            found.append(line)
            end = text.find("\n", i) if text.startswith("//", i) else text.find("*/", i)
            if end == -1:
                break
            line += text.count("\n", i, end)
            i = end + (0 if text.startswith("//", i) else 2)
            continue
        raw = re.match(r'[bc]?r(#*)"', text[i:])
        if raw and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            closing = '"' + raw.group(1)
            end = text.find(closing, i + raw.end())
            end = length if end == -1 else end + len(closing)
            line += text.count("\n", i, end)
            i = end
            continue
        if c == '"':
            j = i + 1
            while j < length and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            line += text.count("\n", i, j)
            i = j + 1
            continue
        if c == "'":
            char = re.match(r"'(\\.|\\u\{[0-9a-fA-F]+\}|\\x[0-9a-fA-F]{2}|[^\\'])'", text[i:])
            i += char.end() if char else 1
            continue
        i += 1
    return found


def svelte_comments(text):
    found = [text.count("\n", 0, m.start()) + 1 for m in re.finditer(r"<!--", text)]
    for block in re.finditer(r"<style[^>]*>(.*?)</style>", text, re.S):
        found += [text.count("\n", 0, block.start(1) + m.start()) + 1 for m in re.finditer(r"/\*", block.group(1))]
    return found


def css_comments(text):
    return [text.count("\n", 0, m.start()) + 1 for m in re.finditer(r"/\*", text)]


def main():
    top = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True, check=True
    ).stdout.strip()
    listing = subprocess.run(["git", "ls-files", "-z"], capture_output=True, text=True, check=True, cwd=top)
    files = [name for name in listing.stdout.split("\0") if name]
    problems = []
    for name in files:
        path = pathlib.Path(top) / name
        checker = {".rs": rust_comments, ".svelte": svelte_comments, ".css": css_comments}.get(path.suffix)
        if checker is None:
            continue
        for line in checker(path.read_text(encoding="utf-8")):
            problems.append(f"{name}:{line}")
    if problems:
        print("Comments are not allowed in code:")
        print("\n".join(problems))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
